//! Keep the workspace manifest, owned lockfile entries, and README badge in sync.
use semver::Version;
use std::{collections::BTreeSet, fs, path::Path};
use toml_edit::{DocumentMut, value};

fn next(current: &Version, change: &str) -> Result<Version, String> {
    let increment = |n: u64| {
        n.checked_add(1)
            .ok_or("Version number overflow".to_string())
    };
    match change {
        "sync" => Ok(current.clone()),
        "patch" => Ok(Version::new(
            current.major,
            current.minor,
            increment(current.patch)?,
        )),
        "minor" => Ok(Version::new(current.major, increment(current.minor)?, 0)),
        "major" => Ok(Version::new(increment(current.major)?, 0, 0)),
        explicit if explicit.len() <= 64 => Version::parse(explicit).map_err(|e| e.to_string()),
        _ => Err("Version must be a semantic version of at most 64 characters.".into()),
    }
}

fn badge(version: &str) -> String {
    let width = 76 + version.len() * 8;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="20" role="img" aria-label="version: {version}">
<title>version: {version}</title>
<rect width="{width}" height="20" fill="#35445c"/>
<rect x="64" width="{}" height="20" fill="#28ad83"/>
<g fill="#fff" text-anchor="middle" font-family="Verdana,sans-serif" font-size="11">
<text x="32" y="14">version</text>
<text x="{}" y="14">{version}</text>
</g>
</svg>
"##,
        width - 64,
        64 + (width - 64) / 2
    )
}

pub(super) fn check_badge(root: &Path, version: &str) -> Result<(), String> {
    let current =
        fs::read_to_string(root.join("assets/branding/version.svg")).map_err(|e| e.to_string())?;
    if current != badge(version) {
        return Err(
            "Version badge is stale. Run cargo app-version sync, commit, and push before uploading."
                .into(),
        );
    }
    Ok(())
}

pub(super) fn run(change: Option<&str>) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Missing workspace")?;
    let manifest_path = root.join("Cargo.toml");
    let source = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    let mut manifest: DocumentMut = source
        .parse()
        .map_err(|e: toml_edit::TomlError| e.to_string())?;
    let current = Version::parse(
        manifest["workspace"]["package"]["version"]
            .as_str()
            .ok_or("Missing workspace version")?,
    )
    .map_err(|e| e.to_string())?;
    let Some(change) = change else {
        println!("{current}");
        return Ok(());
    };
    let updated = next(&current, change)?.to_string();
    let mut names = BTreeSet::new();
    for member in manifest["workspace"]["members"]
        .as_array()
        .ok_or("Missing workspace members")?
    {
        let path = root
            .join(member.as_str().ok_or("Invalid workspace member")?)
            .join("Cargo.toml");
        let document: DocumentMut = fs::read_to_string(path)
            .map_err(|e| e.to_string())?
            .parse()
            .map_err(|e: toml_edit::TomlError| e.to_string())?;
        if document["package"]["version"]["workspace"].as_bool() == Some(true) {
            names.insert(
                document["package"]["name"]
                    .as_str()
                    .ok_or("Missing package name")?
                    .to_owned(),
            );
        }
    }
    let lock_path = root.join("Cargo.lock");
    let original_lock = fs::read_to_string(&lock_path).map_err(|e| e.to_string())?;
    let mut lock: DocumentMut = original_lock
        .parse()
        .map_err(|e: toml_edit::TomlError| e.to_string())?;
    for package in lock["package"]
        .as_array_of_tables_mut()
        .ok_or("Missing lockfile packages")?
        .iter_mut()
    {
        if package.get("source").is_none()
            && package
                .get("name")
                .and_then(|item| item.as_str())
                .is_some_and(|name| names.remove(name))
        {
            package["version"] = value(&updated);
        }
    }
    if !names.is_empty() {
        return Err(
            "Workspace package missing from Cargo.lock. Refresh the lockfile first.".into(),
        );
    }
    manifest["workspace"]["package"]["version"] = value(&updated);
    let badge_path = root.join("assets/branding/version.svg");
    let original_badge = fs::read(&badge_path).ok();
    let result = (|| -> std::io::Result<()> {
        fs::write(&manifest_path, manifest.to_string())?;
        fs::write(&lock_path, lock.to_string())?;
        fs::write(&badge_path, badge(&updated))
    })();
    if let Err(error) = result {
        fs::write(&manifest_path, source).map_err(|e| format!("Restore manifest: {e}"))?;
        fs::write(&lock_path, original_lock).map_err(|e| format!("Restore lockfile: {e}"))?;
        if let Some(bytes) = original_badge {
            fs::write(&badge_path, bytes).map_err(|e| format!("Restore badge: {e}"))?;
        }
        return Err(error.to_string());
    }
    println!("{current} -> {updated}; workspace, lockfile, and badge updated.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_bumps_clear_old_prereleases_and_detect_overflow() {
        let version = Version::parse("2.3.4-rc.1+build.7").unwrap();
        for (change, expected) in [
            ("patch", "2.3.5"),
            ("minor", "2.4.0"),
            ("major", "3.0.0"),
            ("sync", "2.3.4-rc.1+build.7"),
            ("4.0.0-beta.2", "4.0.0-beta.2"),
        ] {
            assert_eq!(next(&version, change).unwrap().to_string(), expected);
        }
        assert!(next(&version, "invalid").is_err());
        assert!(next(&Version::new(0, 0, u64::MAX), "patch").is_err());
    }
}
