fn main() {
    let mut attrs = tauri_build::Attributes::new();

    #[cfg(target_os = "windows")]
    {
        // Embed an admin-elevation manifest. Without this, our non-elevated
        // process can launch elevated apps (via UAC) but can't reposition
        // their windows (SetWindowPlacement is blocked by UIPI). Tauri-build
        // already embeds a default manifest; passing app_manifest replaces it.
        let manifest = std::fs::read_to_string("app.manifest")
            .expect("read app.manifest");
        attrs = attrs.windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(manifest),
        );
    }

    tauri_build::try_build(attrs).expect("tauri_build failed");
}
