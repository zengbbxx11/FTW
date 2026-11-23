fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "core_health",
        "core_changes",
        "crm_request",
        "mail_request",
        "inbox_request",
        "data_request",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("could not build Tauri application manifest");
}
