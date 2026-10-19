//! Resolve process configuration before constructing the UI or starting workers.
use skillissue_domain::identity::{APPLICATION_ID, CONFIG_DIR_ENV};
use std::path::PathBuf;

pub fn data_directory() -> Result<PathBuf, String> {
    let configured = std::env::var_os(CONFIG_DIR_ENV).map(PathBuf::from);
    let roaming = std::env::var_os("APPDATA").map(PathBuf::from);
    resolve_directory(
        configured,
        roaming,
        std::env::current_dir().map_err(|error| error.to_string())?,
    )
}

fn resolve_directory(
    configured: Option<PathBuf>,
    roaming: Option<PathBuf>,
    working: PathBuf,
) -> Result<PathBuf, String> {
    let directory = configured.unwrap_or_else(|| {
        roaming
            .map(|path| path.join(APPLICATION_ID))
            .unwrap_or_else(|| working.join("skillissue-data"))
    });
    if !directory.is_absolute() {
        return Err(format!("{CONFIG_DIR_ENV} must name an absolute directory."));
    }
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_profile_has_precedence_and_relative_paths_are_rejected() {
        let working = std::env::temp_dir();
        let explicit = working.join("explicit-profile");
        assert_eq!(
            resolve_directory(
                Some(explicit.clone()),
                Some(working.join("roaming")),
                working.clone()
            )
            .unwrap(),
            explicit
        );
        assert!(resolve_directory(Some(PathBuf::from("relative-profile")), None, working).is_err());
    }

    #[test]
    fn fallback_is_absolute_and_does_not_create_a_profile() {
        let working =
            std::env::temp_dir().join(format!("skillissue-bootstrap-{}", uuid::Uuid::new_v4()));
        assert_eq!(
            resolve_directory(None, Some(working.clone()), working.clone()).unwrap(),
            working.join(APPLICATION_ID)
        );
        assert_eq!(
            resolve_directory(None, None, working.clone()).unwrap(),
            working.join("skillissue-data")
        );
        assert!(!working.exists());
    }
}
