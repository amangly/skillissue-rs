use super::runtime::ShortcutRegistrationRetry;
use skillissue_domain::shortcuts::{ShortcutContext, default_shortcuts};
use std::time::{Duration, Instant};

#[test]
fn registration_conflict_retries_at_deadline_and_stops_after_recovery() {
    let now = Instant::now();
    let requested = (default_shortcuts(), ShortcutContext::default());
    let mut retry = ShortcutRegistrationRetry::default();
    let mut error = None;

    assert!(retry.due(None, &requested, now));
    assert!(retry.complete(Err("Another app owns Ctrl+Enter".into()), now, &mut error));
    assert!(!retry.due(Some(&requested), &requested, now));
    assert!(!retry.due(
        Some(&requested),
        &requested,
        now + Duration::from_millis(1999)
    ));
    let recovered_at = now + Duration::from_secs(2);
    assert!(retry.due(Some(&requested), &requested, recovered_at));
    assert!(retry.complete(Ok(()), recovered_at, &mut error));
    assert!(error.is_none());
    assert!(!retry.due(
        Some(&requested),
        &requested,
        recovered_at + Duration::from_secs(100)
    ));
}

#[test]
fn continuing_registration_conflict_backs_off_without_repainting_same_error() {
    let now = Instant::now();
    let requested = (default_shortcuts(), ShortcutContext::default());
    let mut retry = ShortcutRegistrationRetry::default();
    let mut error = None;
    assert!(retry.complete(Err("Shortcut unavailable".into()), now, &mut error));
    let second_attempt = now + Duration::from_secs(2);
    assert!(!retry.complete(
        Err("Shortcut unavailable".into()),
        second_attempt,
        &mut error
    ));
    assert!(!retry.due(Some(&requested), &requested, second_attempt));
    assert!(!retry.due(
        Some(&requested),
        &requested,
        now + Duration::from_millis(3999)
    ));
    assert!(retry.due(Some(&requested), &requested, now + Duration::from_secs(4)));
}

#[test]
fn editing_binding_and_entering_or_leaving_recorder_attempt_immediately() {
    let now = Instant::now();
    let configured = (default_shortcuts(), ShortcutContext::default());
    let mut retry = ShortcutRegistrationRetry::default();
    let mut error = None;
    retry.complete(Err("Shortcut unavailable".into()), now, &mut error);

    let mut edited = configured.clone();
    edited.0.insert("settings".into(), "Ctrl+Alt+F11".into());
    assert!(retry.due(Some(&configured), &edited, now));

    let mut recording = configured.clone();
    recording.1.recording = true;
    assert!(retry.due(Some(&configured), &recording, now));
    assert!(retry.due(Some(&recording), &configured, now));
    assert!(retry.due(None, &configured, now));
}

#[test]
fn successful_registration_preserves_unrelated_ui_error() {
    let now = Instant::now();
    let mut retry = ShortcutRegistrationRetry::default();
    let mut error = None;
    retry.complete(Err("Shortcut unavailable".into()), now, &mut error);
    error = Some("Provider unavailable".into());
    assert!(!retry.complete(Ok(()), now + Duration::from_secs(2), &mut error));
    assert_eq!(error.as_deref(), Some("Provider unavailable"));
}

#[test]
fn successful_registration_does_not_restore_a_dismissed_error() {
    let now = Instant::now();
    let mut retry = ShortcutRegistrationRetry::default();
    let mut error = None;
    retry.complete(Err("Shortcut unavailable".into()), now, &mut error);
    error = None;
    assert!(!retry.complete(Ok(()), now + Duration::from_secs(2), &mut error));
    assert!(error.is_none());
}

#[test]
fn success_with_no_registration_failure_preserves_existing_error() {
    let now = Instant::now();
    let mut retry = ShortcutRegistrationRetry::default();
    let mut error = Some("Audio device unavailable".into());
    assert!(!retry.complete(Ok(()), now, &mut error));
    assert_eq!(error.as_deref(), Some("Audio device unavailable"));
}
