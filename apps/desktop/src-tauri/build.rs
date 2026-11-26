fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "core_health",
        "mcp_status",
        "export_support_report",
        "report_webview_failure",
        "core_changes",
        "crm_request",
        "mail_request",
        "choose_mail_assets",
        "import_customer_documents",
        "export_customer_document",
        "save_inbox_attachment",
        "save_outbound_attachment",
        "inbox_request",
        "data_request",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("could not build Tauri application manifest");
}
