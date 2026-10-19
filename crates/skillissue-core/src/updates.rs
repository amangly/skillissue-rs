//! Stable GitHub release checks and bounded, digest-verified installer downloads.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Duration};

const REPO: &str = "https://github.com/amangly/skillissue-rs/releases/download/";
const MAX_INSTALLER: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Update {
    pub version: String,
    url: String,
    digest: String,
    size: usize,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
    size: usize,
}

fn select(release: Release, current: &str) -> Result<Option<Update>, String> {
    let version = semver::Version::parse(release.tag_name.trim_start_matches('v'))
        .map_err(|_| "Release has an invalid version")?;
    let current = semver::Version::parse(current).map_err(|e| e.to_string())?;
    if release.draft || release.prerelease || !version.pre.is_empty() || version <= current {
        return Ok(None);
    }
    let name = format!("skillissue-{version}-windows-x64-setup.exe");
    let asset = release
        .assets
        .into_iter()
        .find(|a| a.name == name)
        .ok_or("This release has no Windows installer")?;
    if asset.browser_download_url != format!("{REPO}{}/{name}", release.tag_name)
        || asset.size == 0
        || asset.size > MAX_INSTALLER
    {
        return Err("Release installer metadata is invalid".into());
    }
    let digest = asset
        .digest
        .and_then(|d| d.strip_prefix("sha256:").map(str::to_owned))
        .filter(|d| d.len() == 64 && d.bytes().all(|c| c.is_ascii_hexdigit()))
        .ok_or("Release installer has no valid SHA-256 digest")?;
    Ok(Some(Update {
        version: version.to_string(),
        url: asset.browser_download_url,
        digest,
        size: asset.size,
    }))
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("skillissue/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| e.to_string())
}

async fn bounded(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    response = response.error_for_status().map_err(|e| e.to_string())?;
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("Update response exceeds the size limit".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err("Update response exceeds the size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn check(current: &str) -> Result<Option<Update>, String> {
    let response = client()?
        .get("https://api.github.com/repos/amangly/skillissue-rs/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(
            "Update feed unavailable. The repository must expose a public stable release.".into(),
        );
    }
    let release = serde_json::from_slice(&bounded(response, 1024 * 1024).await?)
        .map_err(|e| e.to_string())?;
    select(release, current)
}

fn verify(update: &Update, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() != update.size
        || !format!("{:x}", Sha256::digest(bytes)).eq_ignore_ascii_case(&update.digest)
    {
        return Err("Installer verification failed; nothing was installed".into());
    }
    Ok(())
}

pub async fn download(update: Update) -> Result<PathBuf, String> {
    let response = client()?
        .get(&update.url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let bytes = bounded(response, update.size).await?;
    verify(&update, &bytes)?;
    tokio::task::spawn_blocking(move || {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("skillissue-update-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("skillissue-setup.exe");
        let result = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            file.write_all(&bytes)?;
            file.sync_all()
        })();
        if let Err(error) = result {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_dir(&dir);
            return Err(error.to_string());
        }
        Ok(path)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Recheck the staged file before executing it, including after a delayed install.
pub fn verify_file(update: &Update, path: &std::path::Path) -> Result<(), String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(update.size as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    verify(update, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "downloads the public GitHub installer; never executes it"]
    async fn live_release_download_verifies() {
        let update = check("0.0.0").await.unwrap().expect("published release");
        let path = download(update.clone()).await.unwrap();
        let verification = verify_file(&update, &path);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(path.parent().unwrap()).unwrap();
        verification.unwrap();
    }
    fn release(version: &str) -> Release {
        let name = format!("skillissue-{version}-windows-x64-setup.exe");
        Release {
            tag_name: format!("v{version}"),
            draft: false,
            prerelease: false,
            assets: vec![Asset {
                browser_download_url: format!("{REPO}v{version}/{name}"),
                name,
                digest: Some(format!("sha256:{:x}", Sha256::digest(b"installer"))),
                size: 9,
            }],
        }
    }
    #[test]
    fn stable_versions_only_and_no_downgrades() {
        for version in ["0.0.9", "0.1.0", "0.2.0-dev.abc"] {
            assert!(select(release(version), "0.1.0").unwrap().is_none());
        }
        assert!(select(release("0.10.0"), "0.9.0").unwrap().is_some());
        let mut draft = release("0.2.0");
        draft.draft = true;
        assert!(select(draft, "0.1.0").unwrap().is_none());
    }
    #[test]
    fn rejects_missing_digest_foreign_url_and_corrupt_download() {
        let mut invalid = release("0.2.0");
        invalid.assets[0].digest = None;
        assert!(select(invalid, "0.1.0").is_err());
        let mut invalid = release("0.2.0");
        invalid.assets[0].browser_download_url = "https://example.com/setup.exe".into();
        assert!(select(invalid, "0.1.0").is_err());
        let update = select(release("0.2.0"), "0.1.0").unwrap().unwrap();
        assert!(verify(&update, b"installer").is_ok());
        assert!(verify(&update, b"corrupted").is_err());
        assert!(verify(&update, b"short").is_err());
    }
}
