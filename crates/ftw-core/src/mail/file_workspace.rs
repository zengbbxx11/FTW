use super::{attachments, check_text, new_id, Failure, MailService, Reply, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use ftw_ipc::{
    agent::{DocumentUploadChunk, MailAttachmentSource},
    crm::Source,
    mail::CustomerMailFile,
};
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, SqlitePool};

const CHUNK: u32 = 192 * 1024;

pub(crate) const FILES_SQL: &str = "
    SELECT 'outbound' origin,d.id message_id,j.key+json_array_length(d.attachments_json) file_index,a.filename,a.content_type,a.size,a.sha256,
           d.contact_id,d.subject,d.updated_at occurred_at,NULL document_id,a.agent_allowed
    FROM email_drafts d JOIN json_each(d.asset_ids_json) j JOIN mail_assets a ON a.id=j.value
    UNION ALL
    SELECT 'outbound',d.id,j.key,json_extract(j.value,'$.filename'),NULL,NULL,NULL,
           d.contact_id,d.subject,d.updated_at,NULL,1
    FROM email_drafts d JOIN json_each(d.attachments_json) j WHERE j.type='object'
    UNION ALL
    SELECT 'inbox',i.id,j.key,j.value,NULL,NULL,NULL,i.contact_id,i.subject,i.received_at,NULL,1
    FROM inbox_messages i JOIN json_each(i.attachment_names) j WHERE i.contact_id IS NOT NULL AND j.type='text'
    UNION ALL
    SELECT 'document','',0,d.filename,d.content_type,d.size,d.sha256,d.contact_id,'',d.created_at,d.id,1
    FROM customer_documents d";

pub(super) async fn list(
    pool: &SqlitePool,
    contact: Option<String>,
    keyword: &str,
    direction: &str,
    offset: u32,
    document_id: Option<String>,
) -> Result<Reply> {
    let keyword = keyword.trim();
    if !keyword.is_empty() {
        check_text(keyword, 320)?;
    }
    if !matches!(direction, "" | "inbox" | "outbound" | "document") {
        return Err(Failure::Code("invalid_mail_file_direction"));
    }
    let mut query = QueryBuilder::new("WITH files AS (");
    query.push(FILES_SQL).push(
        ") SELECT f.*,c.id resolved_contact_id,c.display_name contact_name,d.created_by,d.source,a.name created_by_name
          FROM files f JOIN contacts original ON original.id=f.contact_id
          JOIN contacts c ON c.id=COALESCE(original.merged_into,original.id)
          LEFT JOIN customer_documents d ON d.id=f.document_id
          LEFT JOIN mail_agent_clients a ON a.id=d.created_by
          WHERE f.filename IS NOT NULL AND f.filename<>''",
    );
    if let Some(contact) = contact {
        query.push(" AND c.id=").push_bind(contact);
    }
    if let Some(id) = document_id {
        query.push(" AND f.document_id=").push_bind(id);
    }
    if !direction.is_empty() {
        query.push(" AND f.origin=").push_bind(direction);
    }
    if !keyword.is_empty() {
        query.push(" AND (");
        let mut predicates = query.separated(" OR ");
        for field in [
            "f.filename",
            "f.subject",
            "c.display_name",
            "c.company_name",
        ] {
            predicates
                .push(format!("instr(lower({field}),lower("))
                .push_bind_unseparated(keyword)
                .push_unseparated("))>0");
        }
        query.push(")");
    }
    query
        .push(" ORDER BY f.occurred_at DESC,f.origin,f.message_id,f.file_index,f.document_id LIMIT 21 OFFSET ")
        .push_bind(i64::from(offset));
    let mut rows = query
        .build()
        .map(|row: SqliteRow| CustomerMailFile {
            origin: row.get("origin"),
            message_id: row.get("message_id"),
            document_id: row.get("document_id"),
            index: row.get("file_index"),
            filename: row.get("filename"),
            content_type: row.get("content_type"),
            size: row.get("size"),
            sha256: row.get("sha256"),
            contact_id: row.get("resolved_contact_id"),
            contact_name: row.get("contact_name"),
            subject: row.get("subject"),
            occurred_at: row.get("occurred_at"),
            created_by: row.get("created_by"),
            created_by_name: row.get("created_by_name"),
            source: row.get("source"),
        })
        .fetch_all(pool)
        .await?;
    let has_more = rows.len() > 20;
    rows.truncate(20);
    Ok(Reply::FileWorkspace { rows, has_more })
}

impl MailService {
    pub(super) async fn import_customer_documents(
        &self,
        contact: &str,
        paths: Vec<String>,
    ) -> Result<Reply> {
        let exists = "SELECT EXISTS(SELECT 1 FROM contacts WHERE id=? AND archived_at IS NULL)";
        let active: bool = sqlx::query_scalar(exists)
            .bind(contact)
            .fetch_one(&self.pool)
            .await?;
        if !active {
            return Err(Failure::Code("crm_not_found"));
        }
        let files = attachments::import_objects(self.objects()?, paths).await?;
        let mut count = 0;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        for file in files {
            let (_, inserted) =
                insert_document(&mut tx, contact, file, Source::Manual, "desktop-user").await?;
            count += usize::from(inserted);
        }
        tx.commit().await?;
        Ok(Reply::ImportedDocuments { count })
    }

    pub(super) async fn upload_document(
        &self,
        contact: &str,
        actor: &str,
        filename: String,
        content: &str,
    ) -> Result<Reply> {
        // Keep Base64 and the enclosing request below the one MiB IPC limit.
        if content.len() > 768 * 1024 {
            return Err(Failure::Code("attachment_limit"));
        }
        let bytes = STANDARD
            .decode(content)
            .map_err(|_| Failure::Code("invalid_attachments"))?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        active_contact(&mut tx, contact).await?;
        let file = attachments::import_bytes(self.objects()?, &filename, &bytes).await?;
        let (id, _) = insert_document(&mut tx, contact, file, Source::Agent, actor).await?;
        tx.commit().await?;
        Ok(Reply::Saved { id })
    }

    pub(super) async fn read_document(&self, id: &str, offset: u32) -> Result<Reply> {
        let (sha, size) = self.document_metadata(id).await?;
        if offset > size {
            return Err(Failure::Code("invalid_document_offset"));
        }
        let bytes = attachments::read_object(self.objects()?, &sha, size as usize)
            .await
            .map_err(attachments::file_error)?;
        data_page(id, offset, &bytes, sha)
    }

    pub(super) async fn preview_customer_image(&self, id: &str, offset: u32) -> Result<Reply> {
        let image: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM customer_documents WHERE id=?
            AND content_type IN ('image/png','image/jpeg','image/webp'))",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        if !image {
            return Err(Failure::Code("document_preview_unavailable"));
        }
        self.read_document(id, offset).await
    }

    pub(super) async fn upload_document_chunk(
        &self,
        actor: &str,
        input: DocumentUploadChunk,
    ) -> Result<Reply> {
        let DocumentUploadChunk {
            upload_id,
            contact_id,
            filename,
            total_size,
            offset,
            content_base64,
        } = input;
        let upload_id = uuid::Uuid::parse_str(&upload_id)
            .map_err(|_| Failure::Code("invalid_upload_id"))?
            .to_string();
        check_text(&filename, 180)?;
        if filename.contains(['/', '\\', ':'])
            || total_size as usize > attachments::LIMIT
            || offset >= total_size
            || offset % CHUNK != 0
            || content_base64.len() > CHUNK as usize / 3 * 4
        {
            return Err(Failure::Code("invalid_upload_chunk"));
        }
        let bytes = STANDARD
            .decode(content_base64)
            .map_err(|_| Failure::Code("invalid_attachments"))?;
        if bytes.len() != (total_size - offset).min(CHUNK) as usize {
            return Err(Failure::Code("invalid_upload_chunk"));
        }
        let hash = attachments::hash(&bytes);
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("DELETE FROM customer_document_uploads WHERE updated_at<unixepoch()-86400")
            .execute(&mut *tx)
            .await?;
        let session: Option<(String, String, u32, u32, Option<String>)> = sqlx::query_as(
            "SELECT contact_id,filename,total_size,next_offset,document_id FROM customer_document_uploads
             WHERE actor_id=? AND upload_id=?").bind(actor).bind(&upload_id).fetch_optional(&mut *tx).await?;
        let progress = |next_offset, document_id| Reply::DocumentUploadProgress {
            upload_id: upload_id.clone(),
            next_offset,
            document_id,
        };
        if let Some((owner, name, size, next, document)) = session {
            if owner != contact_id || name != filename || size != total_size || offset > next {
                return Err(Failure::Code("upload_chunk_conflict"));
            }
            if offset < next {
                let previous: Option<String> = sqlx::query_scalar(
                    "SELECT sha256 FROM customer_document_upload_parts WHERE actor_id=? AND upload_id=? AND byte_offset=?")
                    .bind(actor).bind(&upload_id).bind(offset).fetch_optional(&mut *tx).await?;
                if previous.as_deref() != Some(&hash) {
                    return Err(Failure::Code("upload_chunk_conflict"));
                }
                return Ok(progress(next, document));
            }
        } else {
            if offset != 0 {
                return Err(Failure::Code("upload_chunk_conflict"));
            }
            active_contact(&mut tx, &contact_id).await?;
            let open: i64 = sqlx::query_scalar("SELECT count(*) FROM customer_document_uploads WHERE actor_id=? AND document_id IS NULL")
                .bind(actor).fetch_one(&mut *tx).await?;
            if open >= 4 {
                return Err(Failure::Code("attachment_limit"));
            }
            sqlx::query("INSERT INTO customer_document_uploads(actor_id,upload_id,contact_id,filename,total_size) VALUES(?,?,?,?,?)")
                .bind(actor).bind(&upload_id).bind(&contact_id).bind(&filename).bind(total_size).execute(&mut *tx).await?;
        }
        let next = offset + bytes.len() as u32;
        sqlx::query("INSERT INTO customer_document_upload_parts(actor_id,upload_id,byte_offset,sha256,bytes) VALUES(?,?,?,?,?)")
            .bind(actor).bind(&upload_id).bind(offset).bind(hash).bind(bytes).execute(&mut *tx).await?;
        let document = if next == total_size {
            let parts: Vec<Vec<u8>> = sqlx::query_scalar(
                "SELECT bytes FROM customer_document_upload_parts
                WHERE actor_id=? AND upload_id=? ORDER BY byte_offset",
            )
            .bind(actor)
            .bind(&upload_id)
            .fetch_all(&mut *tx)
            .await?;
            let bytes = parts.concat();
            if bytes.len() != total_size as usize {
                return Err(Failure::Code("invalid_upload_chunk"));
            }
            let file = attachments::import_bytes(self.objects()?, &filename, &bytes).await?;
            let (document, _) =
                insert_document(&mut tx, &contact_id, file, Source::Agent, actor).await?;
            sqlx::query("UPDATE customer_document_upload_parts SET bytes=X'' WHERE actor_id=? AND upload_id=?")
                .bind(actor).bind(&upload_id).execute(&mut *tx).await?;
            Some(document)
        } else {
            None
        };
        sqlx::query("UPDATE customer_document_uploads SET next_offset=?,document_id=?,updated_at=unixepoch()
            WHERE actor_id=? AND upload_id=?")
            .bind(next).bind(&document).bind(actor).bind(&upload_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(progress(next, document))
    }

    pub(super) async fn read_mail_attachment(
        &self,
        source: MailAttachmentSource,
        id: &str,
        index: u16,
        offset: u32,
    ) -> Result<Reply> {
        let bytes = self.mail_attachment_bytes(source, id, index, true).await?;
        data_page(id, offset, &bytes, attachments::hash(&bytes))
    }

    async fn mail_attachment_bytes(
        &self,
        source: MailAttachmentSource,
        id: &str,
        index: u16,
        agent: bool,
    ) -> Result<Vec<u8>> {
        let bytes = match source {
            MailAttachmentSource::Inbox => self.inbox_attachment_bytes(id, index).await?,
            MailAttachmentSource::Outbound => {
                let mut db = self.pool.acquire().await?;
                let row = sqlx::query(
                    "SELECT attachments_json,asset_ids_json FROM email_drafts WHERE id=?",
                )
                .bind(id)
                .fetch_optional(&mut *db)
                .await?
                .ok_or(Failure::Code("mail_not_found"))?;
                let inline: Vec<ftw_ipc::mail::Attachment> =
                    serde_json::from_str(row.get("attachments_json"))
                        .map_err(|_| Failure::Code("attachment_not_available"))?;
                let index = usize::from(index);
                if let Some(item) = inline.get(index) {
                    STANDARD
                        .decode(&item.content_base64)
                        .map_err(|_| Failure::Code("attachment_not_available"))?
                } else {
                    let ids = attachments::ids(&row)?;
                    let asset_id = ids
                        .get(index - inline.len())
                        .ok_or(Failure::Code("attachment_not_available"))?;
                    let assets =
                        attachments::select(&mut db, std::slice::from_ref(asset_id), agent).await?;
                    attachments::read_asset(self.objects()?, &assets[0]).await?
                }
            }
        };
        Ok(bytes)
    }

    async fn document_metadata(&self, id: &str) -> Result<(String, u32)> {
        sqlx::query_as("SELECT sha256,size FROM customer_documents WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Failure::Code("mail_not_found"))
    }

    pub(super) async fn export_customer_document(
        &self,
        id: &str,
        destination: &str,
    ) -> Result<Reply> {
        let (sha, size) = self.document_metadata(id).await?;
        let bytes = attachments::read_object(self.objects()?, &sha, size as usize)
            .await
            .map_err(attachments::file_error)?;
        attachments::write_new(destination, &bytes).await?;
        Ok(Reply::Saved { id: id.to_owned() })
    }

    pub(super) async fn export_outbound_attachment(
        &self,
        id: &str,
        index: u16,
        destination: &str,
    ) -> Result<Reply> {
        let bytes = self
            .mail_attachment_bytes(MailAttachmentSource::Outbound, id, index, false)
            .await?;
        attachments::write_new(destination, &bytes).await?;
        Ok(Reply::Saved { id: id.to_owned() })
    }
}

fn data_page(id: &str, offset: u32, bytes: &[u8], sha: String) -> Result<Reply> {
    let size = bytes.len() as u32;
    if offset > size {
        return Err(Failure::Code("invalid_document_offset"));
    }
    let end = size.min(offset + CHUNK);
    Ok(Reply::DocumentChunk {
        id: id.to_owned(),
        content_base64: STANDARD.encode(&bytes[offset as usize..end as usize]),
        offset,
        next_offset: (end < size).then_some(end),
        size,
        sha256: sha,
    })
}

async fn active_contact(db: &mut sqlx::SqliteConnection, contact: &str) -> Result<()> {
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM contacts WHERE id=? AND archived_at IS NULL)",
    )
    .bind(contact)
    .fetch_one(db)
    .await?;
    if !active {
        return Err(Failure::Code("crm_not_found"));
    }
    Ok(())
}

async fn insert_document(
    db: &mut sqlx::SqliteConnection,
    contact: &str,
    file: attachments::ImportedObject,
    source: Source,
    actor: &str,
) -> Result<(String, bool)> {
    active_contact(db, contact).await?;
    let id = new_id();
    let inserted = sqlx::query("INSERT INTO customer_documents(id,contact_id,filename,content_type,sha256,size,created_by,source)
        VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(contact_id,filename,sha256) DO NOTHING")
        .bind(&id).bind(contact).bind(&file.filename).bind(file.content_type).bind(&file.sha256).bind(file.size)
        .bind(actor).bind(source.as_str()).execute(&mut *db).await?.rows_affected() == 1;
    if inserted {
        super::audit_as(db, source, actor, "customer_document", &id, "import", None).await?;
        return Ok((id, true));
    }
    let existing = sqlx::query_scalar(
        "SELECT id FROM customer_documents WHERE contact_id=? AND filename=? AND sha256=?",
    )
    .bind(contact)
    .bind(file.filename)
    .bind(file.sha256)
    .fetch_one(db)
    .await?;
    Ok((existing, false))
}
