fn main() {
    // The application MANIFEST is supplied by the LINKER, not the
    // resource compiler — tauri issue #13419's official workaround
    // (verified in DiskGenie production): `WindowsAttributes::
    // new().app_manifest(...)` embeds a manifest the SxS loader rejects
    // (0xC0000139 / side-by-side 14001), and the resource-embedded
    // form never reaches cargo's test binaries either. Linker embedding
    // covers every target.
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
    )
    .expect("failed to run tauri-build");

    // Embed app.manifest (requireAdministrator for raw disk access +
    // the Common-Controls v6 side-by-side dependency — the exe imports
    // v6-only comctl32 exports like TaskDialogIndirect; without the
    // dependency the process dies at init with
    // STATUS_ENTRYPOINT_NOT_FOUND) for ALL targets via the linker.
    //
    // `cargo test` on Windows fails to LOAD the unit-test exe with the
    // same 0xC0000139 otherwise: the manifest must reach test binaries
    // too, and cargo has no link-arg directive scoped to the lib's
    // unit-test harness (`rustc-link-arg-tests` covers tests/*.rs
    // targets only).
    #[cfg(windows)]
    {
        let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
        let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
        if target_os == "windows" && target_env == "msvc" {
            let manifest = std::env::current_dir()
                .expect("build script cwd")
                .join("app.manifest");
            println!("cargo:rerun-if-changed={}", manifest.display());
            // Embed the Windows application manifest file.
            println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
            println!(
                "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
                manifest.to_str().expect("utf-8 manifest path")
            );
        }
    }
}
