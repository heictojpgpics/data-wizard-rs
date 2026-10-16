fn main() {
    // Custom Windows app manifest: `requireAdministrator` — raw disk
    // access (physical drives and volumes) needs an elevated token.
    // Without this the app launches unelevated, every raw open fails
    // with access denied, and the UI shows an empty/broken disk list —
    // the "nothing works" release failure. Same mechanism every tool
    // in this category (EaseUS DRW, R-Studio, DMDE) uses.
    tauri_build::try_build(
        tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
        ),
    )
    .expect("failed to run tauri-build");
}
