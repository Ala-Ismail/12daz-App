use std::env;
use std::fs;
use std::path::Path;

fn windows_resource(output: &Path, binary: &str, description: &str, icon: Option<&Path>) {
    let version = env!("CARGO_PKG_VERSION");
    let numbers = [
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH"),
        "0",
    ]
    .join(",");
    let rc = output.join(format!("{binary}.rc"));
    let icon_line = icon.map_or_else(String::new, |icon| {
        format!("101 ICON \"{}\"", icon.to_string_lossy().replace('\\', "/"))
    });
    fs::write(&rc, format!(r#"
{icon_line}
1 VERSIONINFO
FILEVERSION {numbers}
PRODUCTVERSION {numbers}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0
FILEOS 0x40004L
FILETYPE 1
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904B0"
        BEGIN
            VALUE "CompanyName", "Lambda Technologies Pty Ltd\0"
            VALUE "FileDescription", "{description}\0"
            VALUE "FileVersion", "{version}\0"
            VALUE "InternalName", "{binary}\0"
            VALUE "LegalCopyright", "Copyright (c) 2026 Lambda Technologies Pty Ltd ABN 22 647 165 160\0"
            VALUE "OriginalFilename", "{binary}.exe\0"
            VALUE "ProductName", "12dAZ Converter\0"
            VALUE "ProductVersion", "{version}\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#)).expect("write Windows version resource");
    embed_resource::compile_for(rc, [binary], embed_resource::NONE)
        .manifest_required()
        .expect("compile Windows version resource");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=resources/logo.png");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let output = env::var_os("OUT_DIR").expect("Cargo supplies OUT_DIR");
    let output = Path::new(&output);
    windows_resource(output, "az12-converter", "12dAZ Converter CLI", None);
    if env::var_os("CARGO_FEATURE_DESKTOP").is_some() {
        let png = include_bytes!("resources/logo.png");
        let icon = output.join("logo.ico");
        let mut data = vec![0, 0, 1, 0, 1, 0, 64, 64, 0, 0, 1, 0, 32, 0];
        data.extend((png.len() as u32).to_le_bytes());
        data.extend(22_u32.to_le_bytes());
        data.extend(png);
        fs::write(&icon, data).expect("write Windows icon");
        windows_resource(output, "12dAZ-Converter", "12dAZ Converter", Some(&icon));
    }
}
