fn main() {
    let manifest =
        tauri_build::AppManifest::new().commands(&["core_health", "crm_request", "mail_request"]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("could not build Tauri application manifest");
}
