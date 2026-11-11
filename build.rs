fn main() {
    println!("cargo:rerun-if-changed=logo.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var_os("LSHOT_SKIP_RESOURCES").is_some() {
        return;
    }
    if !std::path::Path::new("logo.ico").exists() {
        return;
    }

    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut numbers = version
        .split('.')
        .map(|part| part.parse::<u16>().unwrap_or(0));
    let major = numbers.next().unwrap_or(1);
    let minor = numbers.next().unwrap_or(0);
    let patch = numbers.next().unwrap_or(0);
    let numeric = format!("{major},{minor},{patch},0");

    let icon = std::path::Path::new("logo.ico")
        .canonicalize()
        .map(|path| path.to_string_lossy().replace('\\', "\\\\"))
        .unwrap_or_else(|_| "logo.ico".to_string());

    let resource = format!(
        "1 ICON \"{icon}\"\n\
         1 VERSIONINFO\n\
         FILEVERSION {numeric}\n\
         PRODUCTVERSION {numeric}\n\
         FILEFLAGSMASK 0x3fL\n\
         FILEFLAGS 0x0L\n\
         FILEOS 0x40004L\n\
         FILETYPE 0x1L\n\
         FILESUBTYPE 0x0L\n\
         BEGIN\n\
         BLOCK \"StringFileInfo\"\n\
         BEGIN\n\
         BLOCK \"040904B0\"\n\
         BEGIN\n\
         VALUE \"CompanyName\", \"Lshot\"\n\
         VALUE \"FileDescription\", \"Lshot\"\n\
         VALUE \"FileVersion\", \"{version}\"\n\
         VALUE \"InternalName\", \"lshot\"\n\
         VALUE \"OriginalFilename\", \"lshot.exe\"\n\
         VALUE \"ProductName\", \"Lshot\"\n\
         VALUE \"ProductVersion\", \"{version}\"\n\
         END\n\
         END\n\
         BLOCK \"VarFileInfo\"\n\
         BEGIN\n\
         VALUE \"Translation\", 0x409, 1200\n\
         END\n\
         END\n"
    );

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    let resource_path = out_dir.join("lshot.rc");
    if std::fs::write(&resource_path, resource).is_err() {
        return;
    }

    let result = embed_resource::compile(&resource_path, embed_resource::NONE);
    match result {
        embed_resource::CompilationResult::Ok | embed_resource::CompilationResult::NotWindows => {}
        embed_resource::CompilationResult::NotAttempted(reason) => {
            println!("cargo:warning=Lshot icon and version resources were not embedded: {reason}");
        }
        embed_resource::CompilationResult::Failed(error) => {
            println!("cargo:warning=Lshot resources were not embedded: {error:?}");
        }
    }
}