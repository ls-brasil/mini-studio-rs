fn main() {
    println!("cargo:rerun-if-changed=assets/mini-studio.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=package.json");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let icon = root
        .join("assets/mini-studio.ico")
        .display()
        .to_string()
        .replace('\\', "\\\\");
    let manifest = root
        .join("assets/app.manifest")
        .display()
        .to_string()
        .replace('\\', "\\\\");
    let package: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("package.json")).expect("Falha ao ler package.json"),
    )
    .expect("package.json inválido");
    let version = package["version"]
        .as_str()
        .expect("version ausente no package.json");
    let parts: Vec<_> = version.split('.').collect();
    assert!(
        parts.len() == 3
            && parts.iter().all(|part| {
                !part.is_empty()
                    && part.bytes().all(|byte| byte.is_ascii_digit())
                    && part.parse::<u16>().is_ok()
            }),
        "version do package.json deve ter três componentes numéricos entre 0 e 65535"
    );
    let numeric = format!("{},0", version.replace('.', ","));
    let resource = format!(
        r#"
#pragma code_page(65001)
#include <windows.h>
1 ICON "{icon}"
1 24 "{manifest}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0
FILEOS 0x40004L
FILETYPE 0x1L
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "041604b0"
  BEGIN
   VALUE "CompanyName", "LS Brasil\0"
   VALUE "FileDescription", "Mini Studio\0"
   VALUE "FileVersion", "{version}.0\0"
   VALUE "ProductName", "Mini Studio\0"
   VALUE "ProductVersion", "{version}\0"
   VALUE "OriginalFilename", "mini-studio-rs.exe\0"
   VALUE "LegalCopyright", "LS Brasil\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x0416, 1200
 END
END
"#
    );
    let path = out.join("mini-studio.rc");
    std::fs::write(&path, resource).unwrap();
    embed_resource::compile(path, embed_resource::NONE)
        .manifest_required()
        .unwrap();
}
