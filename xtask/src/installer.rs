use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn runtime_directory() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("SKILLISSUE_VC_REDIST_DIR") {
        let path = PathBuf::from(path);
        validate_runtime(&path)?;
        return Ok(path);
    }
    let program_files = std::env::var_os("ProgramFiles(x86)").ok_or("Missing ProgramFiles(x86)")?;
    let vswhere =
        PathBuf::from(program_files).join("Microsoft Visual Studio/Installer/vswhere.exe");
    let install = super::output(
        vswhere.to_str().ok_or("Invalid vswhere path")?,
        &[
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ],
    )?;
    let redist = Path::new(&install).join("VC/Redist/MSVC");
    let mut versions = std::fs::read_dir(redist)
        .map_err(|e| format!("Visual C++ redistributable directory: {e}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter_map(|path| {
            Some((
                semver::Version::parse(path.file_name()?.to_str()?).ok()?,
                path,
            ))
        })
        .collect::<Vec<_>>();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, version) in versions {
        if let Ok(entries) = std::fs::read_dir(version.join("x64")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with(".CRT"))
                    && validate_runtime(&path).is_ok()
                {
                    return Ok(path);
                }
            }
        }
    }
    Err("Install the Visual C++ x64 redistributable build files or set SKILLISSUE_VC_REDIST_DIR to their x64 Microsoft.VC*.CRT directory.".into())
}

fn validate_runtime(path: &Path) -> Result<(), String> {
    for name in ["vcruntime140.dll", "vcruntime140_1.dll"] {
        let bytes = std::fs::read(path.join(name)).map_err(|e| format!("{name}: {e}"))?;
        let offset = bytes
            .get(0x3c..0x40)
            .and_then(|v| v.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or("Invalid runtime PE header")? as usize;
        if bytes.get(..2) != Some(b"MZ") || bytes.get(offset..offset + 6) != Some(b"PE\0\0\x64\x86")
        {
            return Err(format!("{name} must be an x64 PE binary."));
        }
    }
    Ok(())
}

pub fn compiler() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("ISCC") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err("ISCC must name an existing Inno Setup compiler executable.".into());
    }
    for base in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
        if let Some(base) = std::env::var_os(base) {
            for folder in [
                "Inno Setup 6",
                "Inno Setup 7",
                "Programs/Inno Setup 6",
                "Programs/Inno Setup 7",
            ] {
                let path = PathBuf::from(&base).join(folder).join("ISCC.exe");
                if path.is_file() {
                    return Ok(path);
                }
            }
        }
    }
    if Command::new("ISCC.exe").arg("/?").output().is_ok() {
        return Ok("ISCC.exe".into());
    }
    Err(
        "Install Inno Setup 6.3+ or set ISCC to its ISCC.exe path, then run cargo installer."
            .into(),
    )
}

pub fn build(
    root: &Path,
    target: &Path,
    version: &str,
    directory: &Path,
) -> Result<PathBuf, String> {
    let compiler = compiler()?;
    let runtime = runtime_directory()?;
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let status = Command::new(compiler)
        .arg(format!("/DRuntimeDir={}", runtime.display()))
        .arg(format!("/DAppVersion={version}"))
        .arg(format!("/DSourceRoot={}", root.display()))
        .arg(format!(
            "/DAppExe={}",
            target.join("release/skillissue.exe").display()
        ))
        .arg(format!("/DOutputPath={}", directory.display()))
        .arg(root.join("packaging/windows/skillissue.iss"))
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("Inno Setup compilation failed.".into());
    }
    let path = directory.join(format!("skillissue-{version}-windows-x64-setup.exe"));
    if !path.is_file() {
        return Err("Inno Setup did not create the expected installer.".into());
    }
    Ok(path)
}

pub fn run(root: &Path) -> Result<(), String> {
    compiler()?;
    if !cfg!(all(windows, target_arch = "x86_64", target_env = "msvc"))
        || std::env::var_os("CARGO_BUILD_TARGET").is_some()
    {
        return Err("Build on x64 Windows MSVC with CARGO_BUILD_TARGET unset.".into());
    }
    let metadata: serde_json::Value = serde_json::from_str(&super::output(
        "cargo",
        &["metadata", "--locked", "--no-deps", "--format-version", "1"],
    )?)
    .map_err(|e| e.to_string())?;
    let target = Path::new(
        metadata["target_directory"]
            .as_str()
            .ok_or("Missing target directory")?,
    );
    let version = metadata["packages"]
        .as_array()
        .and_then(|p| p.iter().find(|p| p["name"] == "skillissue-desktop"))
        .and_then(|p| p["version"].as_str())
        .ok_or("Missing app version")?;
    super::run(
        "cargo",
        &["build", "--locked", "--release", "-p", "skillissue-desktop"],
    )?;
    let path = build(root, target, version, &target.join("installer"))?;
    println!("Installer: {}", path.display());
    Ok(())
}
