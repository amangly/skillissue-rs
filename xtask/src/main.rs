//! Local builds and GitHub prerelease uploads. No hosted runner is involved.
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

const REPO: &str = "amangly/skillissue-rs";
mod installer;
mod version;

fn output(program: &str, args: &[&str]) -> Result<String, String> {
    let result = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if !result.status.success() {
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    String::from_utf8(result.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|e| e.to_string())
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let result = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| format!("{program}: {e}"))?;
    if !result.success() {
        return Err(format!("{program} failed with {result}"));
    }
    Ok(())
}

fn clean_commit() -> Result<String, String> {
    if !output("git", &["status", "--porcelain"])?.is_empty() {
        return Err("Commit changes before uploading so the download has matching source.".into());
    }
    output("git", &["rev-parse", "HEAD"])
}

fn upload(prepare_only: bool) -> Result<(), String> {
    if !cfg!(all(windows, target_arch = "x86_64", target_env = "msvc")) {
        return Err("Build the Windows preview on x86_64 Windows with the MSVC toolchain.".into());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Missing workspace")?;
    std::env::set_current_dir(root).map_err(|e| e.to_string())?;
    let commit = clean_commit()?;
    if std::env::var_os("CARGO_BUILD_TARGET").is_some() {
        return Err("Unset CARGO_BUILD_TARGET before uploading the native Windows build.".into());
    }
    if !prepare_only {
        run("gh", &["auth", "status"])?;
        let remote = output(
            "git",
            &[
                "ls-remote",
                &format!("https://github.com/{REPO}.git"),
                "refs/heads/main",
            ],
        )?;
        if remote.split_whitespace().next() != Some(commit.as_str()) {
            return Err("Push this commit to main before uploading.".into());
        }
    }
    let metadata: serde_json::Value = serde_json::from_str(&output(
        "cargo",
        &["metadata", "--locked", "--no-deps", "--format-version", "1"],
    )?)
    .map_err(|e| e.to_string())?;
    let target = Path::new(
        metadata["target_directory"]
            .as_str()
            .ok_or("Missing Cargo target directory")?,
    );
    let version = metadata["packages"]
        .as_array()
        .and_then(|packages| packages.iter().find(|p| p["name"] == "skillissue-desktop"))
        .and_then(|package| package["version"].as_str())
        .ok_or("Missing desktop version")?;
    version::check_badge(root, version)?;
    installer::compiler()?;
    run(
        "cargo",
        &["build", "--locked", "--release", "-p", "skillissue-desktop"],
    )?;
    if clean_commit()? != commit {
        return Err("Source changed during build. Rebuild before uploading.".into());
    }
    let tag = format!("v{version}-dev.{}", &commit[..12]);
    let directory = target.join("uploads").join(&tag);
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let exe = "skillissue-windows-x64.exe";
    fs::copy(target.join("release/skillissue.exe"), directory.join(exe))
        .map_err(|e| e.to_string())?;
    let source = directory.join("skillissue-source.zip");
    run(
        "git",
        &[
            "archive",
            "--format=zip",
            "--prefix=skillissue-rs/",
            "-o",
            source.to_str().ok_or("Invalid source path")?,
            &commit,
        ],
    )?;
    for file in ["LICENSE", "THIRD_PARTY_NOTICES.md"] {
        fs::copy(root.join(file), directory.join(file)).map_err(|e| e.to_string())?;
    }
    let installer_path = installer::build(root, target, version, &directory)?;
    let installer_name = installer_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid installer name")?;
    let assets = [
        exe,
        installer_name,
        "skillissue-source.zip",
        "LICENSE",
        "THIRD_PARTY_NOTICES.md",
    ];
    let mut sums = String::new();
    for file in assets {
        let bytes = fs::read(directory.join(file)).map_err(|e| e.to_string())?;
        sums.push_str(&format!("{:x}  {file}\n", Sha256::digest(bytes)));
    }
    fs::write(directory.join("SHA256SUMS.txt"), sums).map_err(|e| e.to_string())?;
    let notes = format!(
        "Locally built Windows x64 development preview, version {}.\n\nSource commit: {commit}\n\nDownload `{installer_name}` to install for your Windows user, or `{exe}` to run without installation. Open the setup checklist on the home screen to connect AI providers and test audio. `SHA256SUMS.txt` covers the installer, executable, source archive, and notices.\n\nUnsigned prerelease. Live provider, audio-device, capture, and long-session acceptance remain open. The notices index and supplemental texts are in the source; complete dependency license coverage remains unresolved.\n\nBuilt locally with Cargo; no GitHub Actions runner was used.\n",
        version
    );
    let notes_path = directory.join("release-notes.md");
    fs::write(&notes_path, notes).map_err(|e| e.to_string())?;
    if prepare_only {
        println!("Prepared {}", directory.display());
        return Ok(());
    }
    // Create an unpublished draft first. A failed upload leaves a resumable draft.
    let existing = Command::new("gh")
        .args([
            "release",
            "view",
            &tag,
            "--repo",
            REPO,
            "--json",
            "targetCommitish,isPrerelease,isDraft",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if existing.status.success() {
        let release: serde_json::Value =
            serde_json::from_slice(&existing.stdout).map_err(|e| e.to_string())?;
        if release["targetCommitish"] != commit
            || release["isPrerelease"] != true
            || release["isDraft"] != true
        {
            return Err(format!(
                "{tag} already exists. Published previews are preserved; upload a new source commit."
            ));
        }
    } else {
        run(
            "gh",
            &[
                "release",
                "create",
                &tag,
                "--repo",
                REPO,
                "--target",
                &commit,
                "--draft",
                "--prerelease",
                "--title",
                &format!("skillissue {} development preview", version),
                "--notes-file",
                notes_path.to_str().ok_or("Invalid notes path")?,
            ],
        )?;
    }
    let mut command = Command::new("gh");
    command.args(["release", "upload", &tag, "--repo", REPO, "--clobber"]);
    for file in assets.into_iter().chain(["SHA256SUMS.txt"]) {
        command.arg(directory.join(file));
    }
    if !command.status().map_err(|e| e.to_string())?.success() {
        return Err("Upload failed. The draft is preserved; rerun cargo upload to retry.".into());
    }
    run(
        "gh",
        &[
            "release",
            "edit",
            &tag,
            "--repo",
            REPO,
            "--draft=false",
            "--prerelease",
            "--latest=false",
        ],
    )?;
    println!("https://github.com/{REPO}/releases/tag/{tag}");
    Ok(())
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    std::env::set_current_dir(root).expect("open workspace");
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [action] if action == "installer" => installer::run(root),
        [action] if action == "upload" => upload(false),
        [action, flag] if action == "upload" && flag == "--prepare-only" => upload(true),
        [action] if action == "version" => version::run(None),
        [action, change] if action == "version" => version::run(Some(change)),
        _ => Err("Usage: cargo installer | cargo upload [--prepare-only] | cargo app-version [sync|patch|minor|major|<semver>]".into()),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
