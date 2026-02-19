/// Build script for songwalker_vsti.
///
/// On Windows, embeds media/icon.ico as the application icon in the PE binary,
/// so the .exe and .dll show the correct icon in Explorer / taskbar.
fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("media/icon.ico");
        res.compile().expect("Failed to compile Windows resources");
    }
}
