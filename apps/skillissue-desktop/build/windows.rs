use std::{env, error::Error, fs, path::PathBuf, process::Command};

pub fn embed_resources() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build/windows.rs");
    println!("cargo:rerun-if-changed=../../assets/branding/icon.ico");
    println!("cargo:rerun-if-env-changed=SKILLISSUE_RC");
    let icon =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("missing manifest directory")?)
            .join("../../assets/branding/icon.ico")
            .canonicalize()?;
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("missing build output directory")?);
    let script = output.join("skillissue.rc");
    let resource = output.join("skillissue.res");
    let version = env::var("CARGO_PKG_VERSION")?;
    let numbers = [
        "CARGO_PKG_VERSION_MAJOR",
        "CARGO_PKG_VERSION_MINOR",
        "CARGO_PKG_VERSION_PATCH",
    ]
    .map(|name| env::var(name).expect("Cargo version component"))
    .join(",");
    let icon_path = icon.to_string_lossy().replace('\\', "\\\\");
    fs::write(
        &script,
        format!(
            r#"
1 ICON "{icon_path}"
1 VERSIONINFO
FILEVERSION {numbers},0
PRODUCTVERSION {numbers},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "amangly\0"
      VALUE "LegalCopyright", "Copyright (c) 2026 amangly\0"
      VALUE "FileDescription", "skillissue desktop assistant\0"
      VALUE "FileVersion", "{version}\0"
      VALUE "InternalName", "skillissue\0"
      VALUE "OriginalFilename", "skillissue.exe\0"
      VALUE "ProductName", "skillissue\0"
      VALUE "ProductVersion", "{version}\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#
        ),
    )?;
    let result = Command::new(resource_compiler()?)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg(&script)
        .output()?;
    if !result.status.success() {
        return Err(format!(
            "rc.exe failed: {}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    println!("cargo:rustc-link-arg-bin=skillissue={}", resource.display());
    Ok(())
}

fn resource_compiler() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = env::var_os("SKILLISSUE_RC") {
        return Ok(PathBuf::from(path));
    }
    if let Ok(result) = Command::new("where.exe").arg("rc.exe").output()
        && result.status.success()
        && let Some(path) = String::from_utf8_lossy(&result.stdout).lines().next()
    {
        return Ok(PathBuf::from(path.trim()));
    }
    let sdk = env::var_os("WindowsSdkDir")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                env::var_os("ProgramFiles(x86)").unwrap_or_else(|| "C:/Program Files (x86)".into()),
            )
            .join("Windows Kits/10")
        });
    let mut versions = fs::read_dir(sdk.join("bin"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let version = name
                .split('.')
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            Some((version, entry.path()))
        })
        .collect::<Vec<_>>();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    let architecture = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    versions
        .into_iter()
        .map(|(_, path)| path.join(architecture).join("rc.exe"))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            "Windows SDK rc.exe was not found; set SKILLISSUE_RC to its absolute path".into()
        })
}
