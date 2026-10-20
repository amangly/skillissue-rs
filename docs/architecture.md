# Architecture

The application has four one-way dependency layers:

```text
skillissue-desktop -> skillissue-core -> skillissue-domain
                   -> skillissue-platform -> skillissue-domain
```

Only the desktop crate depends on GPUI. Domain, core, and platform do not depend
on UI widgets or the desktop binary.

## Domain

`skillissue-domain` defines serializable settings, session records, provider
capabilities, application identity, and the shared shortcut parser and conflict
policy. It contains no runtime, filesystem, network, or Windows registration.

## Core

`skillissue-core` owns `NativeEngine` and all UI-independent work:

- `engine/` coordinates configuration, credentials, pairing, account readiness,
  sessions, capture, context, assistance, documents, and lifecycle events.
- `provider/` implements bounded HTTP, SSE, chat, transcription, notes, catalog,
  Palepale pairing, account usage, and provider-specific Smart reasoning parameters.
- `history.rs` stores sessions in SQLite with revision guards.
- `history/recovery.rs` creates standalone backups and restores validated rows
  into an empty journal in one transaction; it never installs backup schemas.
- `documents.rs` stores bounded TXT/PDF/DOCX references and selected excerpts.
- `audio.rs` and `screenshot.rs` contain capture and media validation adapters.

The engine serializes transitions with a lock, snapshots provider/session state
before asynchronous work, and rejects replies from obsolete provider or session
generations. Accepted transcription jobs drain before capture or session changes
complete.

Audio capture uses an embedded Earshot neural detector per device stream.
Streaming resampling produces mono 16 kHz audio and fixed 16 ms inference
frames independently of WASAPI packet boundaries. Segments retain 300 ms of
pre-roll, end after 320 ms of detected non-speech, and obey the configured
maximum duration. Before a question, the app flushes the pending audio segment
without resetting the detector.

The transcript drawer shows queued, transcribing, saved, empty, and failed
segments. The engine tracks pending work and up to 32 completed jobs, then clears
that list when switching sessions. Progress records contain no audio or transcript text.

Questions flush capture and wait up to four seconds for jobs already accepted.
Later speech does not extend the wait. If those jobs remain pending, the app
keeps the question in the composer for the user to send again. It sends no answer
request. Users can cancel context preparation, which times out after ten seconds.
Only the configured Audio adapter receives raw audio.

## Desktop and platform

`skillissue-desktop` owns the GPUI root, dashboard, overlay, settings, modes,
keyboard dispatch, background reply handling, and review fixtures. Tokio work
returns owned replies and the engine publishes `EngineEvent` values; rendering
never performs provider I/O.

The workspace has no sidebar. New session, Modes, and Settings share a wrapping
home toolbar; search and window controls stay in the title bar.
Settings fill the window, with GPUI Kit's underline TabBar above a separate
scrolling content area. Form fields align labels and controls; narrow windows
stack them. Provider connections use GroupBox, switches use Switch, and shortcuts
use Kbd. Shared action heights and radii live in `ui/style.rs`.
The overlay has a full-width toolbar while chat is visible and a compact toolbar
when collapsed. Controls use GPUI Kit defaults and Segoe UI; the existing
blue-gray palette and supplied logo are preserved. The workspace SVGs use subtle
geometric planes and lines rather than the former radial glow artwork.

`skillissue-platform` keeps Windows message-loop objects on the UI thread and
provides global shortcut reconciliation, tray actions, window movement, focus,
capture protection, and application identity.

## Local data

Each profile contains settings, a profile lock, session history, document storage,
and latency logs. Provider secrets are stored in Windows Credential
Manager under a profile-specific namespace. A second process cannot open the same
profile simultaneously.

## Local tooling

The separate Rust `xtask` package implements `cargo upload`. It builds the
desktop app locally and uploads matching source and checksums through Git and
the GitHub CLI. It has no dependency on the application crates and is not part
of the shipped executable. GitHub Actions workflows are disabled.

Screen context and hiding the widget with chat are enabled on normal startup.
The Windows installer registers the per-user login item and removes it on
uninstall. These defaults are no longer exposed as Advanced settings switches.
The desktop reconciles older saved values through the normal preference
transaction; review and smoke modes do not perform that reconciliation.

Desktop components share the application palette in `apps/skillissue-desktop/src/theme.rs`.
Main windows open at 780 by 760 logical pixels with a tinted Windows blur backdrop.
Menus retain readable blue-gray surfaces. The session widget and chat use
matching opaque surfaces without Windows blur, with shared widths and insets. Heroicons are
embedded locally and mapped to GPUI icon paths so built-in controls use the
same icon family. The product logo and workspace artwork remain separate.

Modes use a top toolbar and wrapping selector above a full-width editor. Session
options link to the dedicated keyboard settings instead of repeating all shortcuts.
