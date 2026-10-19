//! Credential storage boundary. Profiles share neither keyring entries nor environment keys.

use crate::{config, models::Settings};
use sha2::{Digest, Sha256};
use skillissue_domain::identity;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub(super) trait Credentials: Send + Sync {
    fn load_saved(&self, scope: &str) -> Result<Option<String>, String>;
    fn save(&self, scope: &str, key: &str) -> Result<(), String>;
    fn load_provider(&self, settings: &Settings) -> Result<String, String>;
}

pub(super) struct SystemCredentials {
    service: String,
    default_profile: bool,
    store: Arc<dyn ServiceStore>,
}

trait ServiceStore: Send + Sync {
    fn load(&self, service: &str, scope: &str) -> Result<Option<String>, String>;
    fn save(&self, service: &str, scope: &str, key: &str) -> Result<(), String>;
}

struct SystemStore;

impl ServiceStore for SystemStore {
    fn load(&self, service: &str, scope: &str) -> Result<Option<String>, String> {
        config::load_saved_key_for_service(service, scope)
    }

    fn save(&self, service: &str, scope: &str, key: &str) -> Result<(), String> {
        config::save_key_for_service(service, scope, key)
    }
}

impl SystemCredentials {
    pub(super) fn for_directory(directory: &Path) -> Result<Self, String> {
        let default = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .map(|path| path.join(identity::APPLICATION_ID));
        Self::for_directory_with_default(directory, default.as_deref())
    }

    fn for_directory_with_default(
        directory: &Path,
        default: Option<&Path>,
    ) -> Result<Self, String> {
        let canonical = std::fs::canonicalize(directory)
            .map_err(|error| format!("Resolve credential profile directory: {error}"))?;
        let canonical = canonical_identity(&canonical);
        let default_profile = default
            .filter(|path| path.is_absolute())
            .and_then(|path| std::fs::canonicalize(path).ok())
            .is_some_and(|path| canonical_identity(&path) == canonical);
        let service = if default_profile {
            identity::CREDENTIAL_SERVICE.to_owned()
        } else {
            let mut digest = Sha256::new();
            digest.update(b"skillissue-credential-profile-v1\0");
            digest.update(canonical);
            format!(
                "{}:profile:{:x}",
                identity::CREDENTIAL_SERVICE,
                digest.finalize()
            )
        };
        Ok(Self {
            service,
            default_profile,
            store: Arc::new(SystemStore),
        })
    }

    fn resolve_provider(
        &self,
        settings: &Settings,
        saved: impl FnOnce(&str) -> Result<Option<String>, String>,
        environment: impl FnOnce(&str) -> String,
    ) -> Result<String, String> {
        if let Some(key) = saved(&config::provider_credential_id(settings))? {
            return Ok(key);
        }
        if self.default_profile && !settings.is_palepale() {
            return Ok(environment(&settings.api_base_url));
        }
        Ok(String::new())
    }
}

fn canonical_identity(path: &Path) -> Vec<u8> {
    let identity = path.as_os_str().as_encoded_bytes().to_vec();
    #[cfg(windows)]
    {
        // Canonicalization resolves filesystem aliases and actual filename case.
        // Normalize drive/prefix ASCII casing without lossy Unicode conversion.
        identity.to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    identity
}

impl Credentials for SystemCredentials {
    fn load_saved(&self, scope: &str) -> Result<Option<String>, String> {
        self.store.load(&self.service, scope)
    }

    fn save(&self, scope: &str, key: &str) -> Result<(), String> {
        self.store.save(&self.service, scope, key)
    }

    fn load_provider(&self, settings: &Settings) -> Result<String, String> {
        self.resolve_provider(
            settings,
            |scope| self.load_saved(scope),
            config::environment_key,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProviderMode;
    use std::{collections::HashMap, sync::Mutex};

    #[derive(Default)]
    struct MemoryStore(Mutex<HashMap<(String, String), String>>);

    impl ServiceStore for MemoryStore {
        fn load(&self, service: &str, scope: &str) -> Result<Option<String>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(service.into(), scope.into()))
                .cloned())
        }

        fn save(&self, service: &str, scope: &str, key: &str) -> Result<(), String> {
            let mut entries = self.0.lock().unwrap();
            let id = (service.into(), scope.into());
            if key.is_empty() {
                entries.remove(&id);
            } else {
                entries.insert(id, key.into());
            }
            Ok(())
        }
    }

    struct Profiles {
        root: PathBuf,
        default: PathBuf,
        other: PathBuf,
        another: PathBuf,
    }

    impl Profiles {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "skillissue-credential-profiles-{}",
                uuid::Uuid::new_v4()
            ));
            let default = root.join(identity::APPLICATION_ID);
            let other = root.join("acceptance");
            let another = root.join("acceptance-two");
            std::fs::create_dir_all(&default).unwrap();
            std::fs::create_dir_all(&other).unwrap();
            std::fs::create_dir_all(&another).unwrap();
            Self {
                root,
                default,
                other,
                another,
            }
        }

        fn credentials(&self, path: &Path) -> SystemCredentials {
            SystemCredentials::for_directory_with_default(path, Some(&self.default)).unwrap()
        }

        fn with_store(&self, path: &Path, store: Arc<dyn ServiceStore>) -> SystemCredentials {
            let mut credentials = self.credentials(path);
            credentials.store = store;
            credentials
        }
    }

    impl Drop for Profiles {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir(&self.other);
            let _ = std::fs::remove_dir(&self.another);
            let _ = std::fs::remove_dir(&self.default);
            let _ = std::fs::remove_dir(&self.root);
        }
    }

    #[test]
    fn default_profile_retains_historical_service_and_byok_environment_fallback() {
        let profiles = Profiles::new();
        let credentials = profiles.credentials(&profiles.default);
        assert_eq!(credentials.service, identity::CREDENTIAL_SERVICE);
        let settings = Settings::default();
        assert_eq!(
            credentials
                .resolve_provider(&settings, |_| Ok(None), |_| "synthetic-environment".into())
                .unwrap(),
            "synthetic-environment"
        );
        let settings = Settings {
            provider_mode: ProviderMode::Palepale,
            ..Settings::default()
        };
        assert!(
            credentials
                .resolve_provider(
                    &settings,
                    |_| Ok(None),
                    |_| panic!("hosted mode never inherits an environment key")
                )
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn nondefault_profile_has_deterministic_service_and_never_reads_environment() {
        let profiles = Profiles::new();
        let credentials = profiles.credentials(&profiles.other);
        assert!(!credentials.default_profile);
        assert!(credentials.service.starts_with("skillissue:profile:"));
        assert_eq!(credentials.service.len(), "skillissue:profile:".len() + 64);
        assert_eq!(
            credentials.service,
            profiles.credentials(&profiles.other).service
        );
        assert_ne!(
            credentials.service,
            profiles.credentials(&profiles.default).service
        );
        for provider_mode in [ProviderMode::Byok, ProviderMode::Palepale] {
            let settings = Settings {
                provider_mode,
                ..Settings::default()
            };
            assert!(
                credentials
                    .resolve_provider(
                        &settings,
                        |_| Ok(None),
                        |_| panic!("isolated profiles never inherit keys")
                    )
                    .unwrap()
                    .is_empty()
            );
        }
    }

    #[test]
    fn canonical_aliases_share_one_profile_identity() {
        let profiles = Profiles::new();
        assert_eq!(
            profiles.credentials(&profiles.other).service,
            profiles.credentials(&profiles.other.join(".")).service
        );
        assert_eq!(
            profiles.credentials(&profiles.default.join(".")).service,
            identity::CREDENTIAL_SERVICE
        );
        #[cfg(windows)]
        assert_eq!(
            profiles
                .credentials(&PathBuf::from(
                    profiles.default.to_str().unwrap().to_uppercase()
                ))
                .service,
            identity::CREDENTIAL_SERVICE
        );
    }

    #[test]
    fn unresolved_default_fails_closed_to_profile_namespace() {
        let profiles = Profiles::new();
        for default in [
            None,
            Some(profiles.root.join("missing")),
            Some(PathBuf::from("relative")),
        ] {
            let credentials =
                SystemCredentials::for_directory_with_default(&profiles.other, default.as_deref())
                    .unwrap();
            assert!(!credentials.default_profile);
            assert!(credentials.service.starts_with("skillissue:profile:"));
        }
    }

    #[test]
    fn provider_scopes_and_saved_empty_keys_are_preserved_without_environment_fallback() {
        let profiles = Profiles::new();
        let credentials = profiles.credentials(&profiles.other);
        for provider_mode in [ProviderMode::Byok, ProviderMode::Palepale] {
            let settings = Settings {
                provider_mode,
                ..Settings::default()
            };
            let expected = config::provider_credential_id(&settings);
            assert_eq!(
                credentials
                    .resolve_provider(
                        &settings,
                        |scope| {
                            assert_eq!(scope, expected);
                            Ok(Some("synthetic-profile-key".into()))
                        },
                        |_| panic!("saved key wins")
                    )
                    .unwrap(),
                "synthetic-profile-key"
            );
            assert!(
                credentials
                    .resolve_provider(
                        &settings,
                        |_| Ok(Some(String::new())),
                        |_| panic!("saved empty key wins")
                    )
                    .unwrap()
                    .is_empty()
            );
        }
        let hosted = config::provider_credential_id(&Settings {
            provider_mode: ProviderMode::Palepale,
            ..Settings::default()
        });
        assert_eq!(
            super::super::pairing::pending_scope(),
            format!("{hosted}:paired-pending")
        );
    }

    #[test]
    fn missing_profile_is_rejected_before_selecting_a_credential_service() {
        let profiles = Profiles::new();
        assert!(
            SystemCredentials::for_directory_with_default(
                &profiles.root.join("missing"),
                Some(&profiles.default)
            )
            .is_err()
        );
    }

    #[test]
    fn separate_profile_loads_reopens_and_deletes_preserve_other_profiles_and_provider_scopes() {
        let profiles = Profiles::new();
        let store = Arc::new(MemoryStore::default());
        let paths = [&profiles.default, &profiles.other, &profiles.another];
        let credentials: Vec<_> = paths
            .iter()
            .map(|path| profiles.with_store(path, store.clone()))
            .collect();
        let scopes = [
            config::provider_credential_id(&Settings::default()),
            config::provider_credential_id(&Settings {
                provider_mode: ProviderMode::Palepale,
                ..Settings::default()
            }),
            super::super::pairing::pending_scope(),
        ];
        for (profile, credentials) in credentials.iter().enumerate() {
            for (scope, id) in scopes.iter().enumerate() {
                credentials
                    .save(id, &format!("synthetic-profile-{profile}-scope-{scope}"))
                    .unwrap();
            }
        }
        for (profile, path) in paths.iter().enumerate() {
            let reopened = profiles.with_store(&path.join("."), store.clone());
            for (scope, id) in scopes.iter().enumerate() {
                assert_eq!(
                    reopened.load_saved(id).unwrap(),
                    Some(format!("synthetic-profile-{profile}-scope-{scope}"))
                );
            }
        }

        // Hosted disconnect removes both approved scopes from only its own profile.
        for id in &scopes[1..] {
            credentials[1].save(id, "").unwrap();
            credentials[1].save(id, "").unwrap();
        }
        // Deleting the default BYOK credential must leave isolated BYOK credentials intact.
        credentials[0].save(&scopes[0], "").unwrap();
        for (profile, path) in paths.iter().enumerate() {
            let reopened = profiles.with_store(path, store.clone());
            for (scope, id) in scopes.iter().enumerate() {
                let deleted = (profile == 1 && scope > 0) || (profile == 0 && scope == 0);
                let expected =
                    (!deleted).then(|| format!("synthetic-profile-{profile}-scope-{scope}"));
                assert_eq!(reopened.load_saved(id).unwrap(), expected);
            }
        }
    }

    #[test]
    fn credential_store_failure_does_not_fall_back_to_an_environment_key() {
        let profiles = Profiles::new();
        let credentials = profiles.credentials(&profiles.default);
        let error = credentials
            .resolve_provider(
                &Settings::default(),
                |_| Err("synthetic store failure".into()),
                |_| panic!("store failure must not silently select another credential"),
            )
            .unwrap_err();
        assert_eq!(error, "synthetic store failure");
    }
}
