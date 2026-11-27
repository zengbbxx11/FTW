use super::{audit, check_text, new_id, Failure, MailService, Reply, Result};
use ftw_ipc::{crm::Source, mail::Asset};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};
use std::{
    collections::HashSet,
    io,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};

pub const LIMIT: usize = 10 * 1024 * 1024;
pub const INBOX_LIMIT: usize = 16 * 1024 * 1024;

impl MailService {
    pub(super) async fn import_assets(&self, paths: Vec<String>) -> Result<Reply> {
        let imported = import_objects(self.objects()?, paths).await?;
        let mut tx = self.pool.begin().await?;
        let mut rows = Vec::new();
        for file in imported {
            let row = sqlx::query(
                "INSERT INTO mail_assets(id,filename,sha256,size,content_type) VALUES (?,?,?,?,?)
                ON CONFLICT(sha256,filename) DO UPDATE SET filename=excluded.filename RETURNING *",
            )
            .bind(new_id())
            .bind(file.filename)
            .bind(file.sha256)
            .bind(file.size)
            .bind(file.content_type)
            .map(asset)
            .fetch_one(&mut *tx)
            .await?;
            audit(&mut tx, Source::Manual, "mail_asset", &row.id, "import").await?;
            rows.push(row);
        }
        tx.commit().await?;
        Ok(Reply::Assets { rows })
    }

    pub(super) async fn list_assets(&self, agent: bool) -> Result<Reply> {
        let rows = sqlx::query(
            "SELECT * FROM mail_assets WHERE NOT ? OR agent_allowed ORDER BY filename,id LIMIT 100",
        )
        .bind(agent)
        .map(asset)
        .fetch_all(&self.pool)
        .await?;
        Ok(Reply::Assets { rows })
    }

    pub(super) async fn prepare_draft_assets(&self, id: &str, revision: i64) -> Result<Reply> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let draft = sqlx::query(
            "SELECT attachments_json,asset_ids_json,revision,state FROM email_drafts WHERE id=?",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Failure::Code("mail_not_found"))?;
        if draft.get::<i64, _>("revision") != revision
            || !matches!(
                draft.get::<&str, _>("state"),
                "draft" | "pending_approval" | "approved"
            )
        {
            return Err(Failure::Code("approval_snapshot_changed"));
        }
        let mut rows = Vec::new();
        for inline in super::attachments(&draft)? {
            let bytes = super::attachment_bytes(&inline)?;
            check_text(&inline.filename, 180)?;
            if inline.filename.contains(['/', '\\']) || bytes.is_empty() || bytes.len() > LIMIT {
                return Err(Failure::Code("invalid_attachments"));
            }
            // Older drafts may contain inline files accepted before the current asset type filter.
            let content_type =
                mime_for(&inline.filename, &bytes).unwrap_or("application/octet-stream");
            let file = ImportedObject {
                filename: inline.filename,
                size: bytes.len() as i64,
                content_type,
                sha256: store(self.objects()?, &bytes).await.map_err(file_error)?,
            };
            let row = sqlx::query(
                "INSERT INTO mail_assets(id,filename,sha256,size,content_type) VALUES (?,?,?,?,?)
                ON CONFLICT(sha256,filename) DO UPDATE SET filename=excluded.filename RETURNING *",
            )
            .bind(new_id())
            .bind(file.filename)
            .bind(file.sha256)
            .bind(file.size)
            .bind(file.content_type)
            .map(asset)
            .fetch_one(&mut *tx)
            .await?;
            audit(&mut tx, Source::Manual, "mail_asset", &row.id, "import").await?;
            rows.push(row);
        }
        rows.extend(select(&mut tx, &ids(&draft)?, false).await?);
        tx.commit().await?;
        Ok(Reply::Assets { rows })
    }

    pub(super) async fn customer_document_asset(
        &self,
        contact_id: &str,
        document_id: &str,
        agent: bool,
    ) -> Result<Reply> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let document = sqlx::query("SELECT filename,sha256,size,content_type FROM customer_documents WHERE id=? AND contact_id=?")
            .bind(document_id).bind(contact_id).fetch_optional(&mut *tx).await?
            .ok_or(Failure::Code("attachment_not_available"))?;
        let filename: String = document.get("filename");
        let sha: String = document.get("sha256");
        let size: i64 = document.get("size");
        read_object(self.objects()?, &sha, size as usize)
            .await
            .map_err(file_error)?;
        let row = if agent {
            sqlx::query("SELECT * FROM mail_assets WHERE filename=? AND sha256=? AND agent_allowed")
                .bind(&filename)
                .bind(&sha)
                .map(asset)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Failure::Code("attachment_not_available"))?
        } else {
            let row = sqlx::query(
                "INSERT INTO mail_assets(id,filename,sha256,size,content_type) VALUES (?,?,?,?,?)
                ON CONFLICT(sha256,filename) DO UPDATE SET filename=excluded.filename RETURNING *",
            )
            .bind(new_id())
            .bind(filename)
            .bind(sha)
            .bind(size)
            .bind(document.get::<String, _>("content_type"))
            .map(asset)
            .fetch_one(&mut *tx)
            .await?;
            audit(&mut tx, Source::Manual, "mail_asset", &row.id, "import").await?;
            row
        };
        tx.commit().await?;
        Ok(Reply::Assets { rows: vec![row] })
    }

    pub(super) async fn attach_agent_assets(
        &self,
        identity: super::policy::AgentIdentity<'_>,
        id: &str,
        revision: i64,
        ids: &[String],
    ) -> Result<Reply> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let agent = super::policy::resolve(&mut tx, identity).await?;
        let assets = select(&mut tx, ids, true).await?;
        for asset in &assets {
            read_asset(self.objects()?, asset).await?;
        }
        let changed = sqlx::query(
            "UPDATE email_drafts SET asset_ids_json=?,attachments_json='[]',revision=revision+1,
                updated_at=unixepoch() WHERE id=? AND revision=? AND state='draft'",
        )
        .bind(serde_json::json!(ids).to_string())
        .bind(id)
        .bind(revision)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(Failure::Code("approval_snapshot_changed"));
        }
        super::audit_as(
            &mut tx,
            Source::Agent,
            &agent,
            "email_draft",
            id,
            "attach_assets",
            None,
        )
        .await?;
        tx.commit().await?;
        Ok(Reply::Saved { id: id.to_owned() })
    }

    pub(super) async fn disable_asset_access(&self, id: String) -> Result<Reply> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE mail_assets SET agent_allowed=0 WHERE id=?")
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            Source::Manual,
            "mail_asset",
            &id,
            "revoke_agent_access",
        )
        .await?;
        tx.commit().await?;
        Ok(Reply::Saved { id })
    }

    pub(super) fn objects(&self) -> Result<&Path> {
        self.object_root
            .as_deref()
            .ok_or(Failure::Code("attachment_storage_unavailable"))
    }
}

pub(super) struct ImportedObject {
    pub filename: String,
    pub size: i64,
    pub content_type: &'static str,
    pub sha256: String,
}

pub(super) async fn import_objects(root: &Path, paths: Vec<String>) -> Result<Vec<ImportedObject>> {
    if paths.is_empty() || paths.len() > 4 {
        return Err(Failure::Code("attachment_limit"));
    }
    let mut imported = Vec::new();
    let mut remaining = LIMIT;
    for path in paths {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(Failure::Code("invalid_attachments"));
        }
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or(Failure::Code("invalid_attachments"))?;
        let data = read_bounded(&path, remaining).await.map_err(file_error)?;
        remaining -= data.len();
        imported.push(import_bytes(root, filename, &data).await?);
    }
    Ok(imported)
}

pub(super) async fn import_bytes(
    root: &Path,
    filename: &str,
    bytes: &[u8],
) -> Result<ImportedObject> {
    check_text(filename, 180)?;
    if filename.contains(['/', '\\', ':']) || bytes.len() > LIMIT {
        return Err(Failure::Code("invalid_attachments"));
    }
    let content_type = mime_for(filename, bytes).ok_or(Failure::Code("attachment_type"))?;
    Ok(ImportedObject {
        filename: filename.to_owned(),
        size: bytes.len() as i64,
        content_type,
        sha256: store(root, bytes).await.map_err(file_error)?,
    })
}

pub(super) async fn write_new(destination: &str, bytes: &[u8]) -> Result<()> {
    let path = Path::new(destination);
    if !path.is_absolute() {
        return Err(Failure::Code("invalid_attachments"));
    }
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .await
        .map_err(|_| Failure::Code("attachment_destination_unavailable"))?;
    let result = async {
        file.write_all(bytes).await?;
        file.sync_all().await
    }
    .await;
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(path).await;
        return Err(Failure::Code("attachment_destination_unavailable"));
    }
    Ok(())
}

pub(super) async fn select(
    db: &mut SqliteConnection,
    ids: &[String],
    agent: bool,
) -> Result<Vec<Asset>> {
    let mut bytes = 0;
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    if ids.len() > 4 {
        return Err(Failure::Code("attachment_limit"));
    }
    for id in ids {
        if !seen.insert(id) {
            return Err(Failure::Code("invalid_attachments"));
        }
        let row = sqlx::query("SELECT * FROM mail_assets WHERE id=? AND (NOT ? OR agent_allowed)")
            .bind(id)
            .bind(agent)
            .map(asset)
            .fetch_optional(&mut *db)
            .await?
            .ok_or(Failure::Code("attachment_not_available"))?;
        bytes += row.size;
        if bytes > LIMIT as i64 {
            return Err(Failure::Code("attachment_limit"));
        }
        rows.push(row);
    }
    Ok(rows)
}

pub(super) fn ids(row: &sqlx::sqlite::SqliteRow) -> Result<Vec<String>> {
    serde_json::from_str(row.get("asset_ids_json"))
        .map_err(|_| Failure::Code("invalid_attachments"))
}

pub(super) async fn read_asset(root: &Path, asset: &Asset) -> Result<Vec<u8>> {
    read_object(root, &asset.sha256, asset.size as usize)
        .await
        .map_err(file_error)
}

pub fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

pub async fn store(root: &Path, data: &[u8]) -> io::Result<String> {
    let sha = hash(data);
    fs::create_dir_all(root).await?;
    let target = object_path(root, &sha).await?;
    if fs::try_exists(&target).await? {
        read_object(root, &sha, data.len()).await?;
        return Ok(sha);
    }
    let pending = root.join(format!(".{}.partial", new_id()));
    let result = async {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .await?;
        file.write_all(data).await?;
        file.sync_all().await?;
        drop(file);
        fs::rename(&pending, &target).await
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&pending).await;
    }
    result?;
    Ok(sha)
}

pub async fn read_object(root: &Path, sha: &str, size: usize) -> io::Result<Vec<u8>> {
    let path = object_path(root, sha).await?;
    let data = read_bounded(&path, size.min(INBOX_LIMIT)).await?;
    if data.len() != size || hash(&data) != sha {
        return Err(io::Error::other("mail object checksum mismatch"));
    }
    Ok(data)
}

async fn object_path(root: &Path, sha: &str) -> io::Result<PathBuf> {
    if sha.len() != 64
        || !sha.bytes().all(|c| c.is_ascii_hexdigit())
        || reparse(&fs::symlink_metadata(root).await?)
    {
        return Err(io::Error::other("invalid mail object path"));
    }
    Ok(root.join(sha))
}

async fn read_bounded(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).await?;
    if !metadata.is_file() || reparse(&metadata) {
        return Err(io::Error::other("not a regular file"));
    }
    if metadata.len() > limit as u64 {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let mut data = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)
        .await?
        .take(limit as u64 + 1)
        .read_to_end(&mut data)
        .await?;
    if data.len() > limit {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    Ok(data)
}

fn reparse(meta: &std::fs::Metadata) -> bool {
    meta.file_attributes() & 0x400 != 0
}

pub(super) fn file_error(error: io::Error) -> Failure {
    Failure::Code(if error.kind() == io::ErrorKind::FileTooLarge {
        "attachment_limit"
    } else {
        "attachment_storage_unavailable"
    })
}

fn asset(row: sqlx::sqlite::SqliteRow) -> Asset {
    Asset {
        id: row.get("id"),
        filename: row.get("filename"),
        sha256: row.get("sha256"),
        size: row.get("size"),
        content_type: row.get("content_type"),
        agent_allowed: row.get("agent_allowed"),
    }
}

fn mime_for(name: &str, data: &[u8]) -> Option<&'static str> {
    match name.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
        "pdf" if data.starts_with(b"%PDF-") => Some("application/pdf"),
        "png" if data.starts_with(b"\x89PNG\r\n\x1a\n") => Some("image/png"),
        "jpg" | "jpeg" if data.starts_with(b"\xff\xd8\xff") => Some("image/jpeg"),
        "webp" if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") => {
            Some("image/webp")
        }
        _ => None,
    }
}
