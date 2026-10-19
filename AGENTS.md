# skillissue-rs agent guide

## Project

`skillissue-rs` is the repository; `skillissue` is the Windows application and
executable name. Read the workspace version from `Cargo.toml`; the license is Apache-2.0.
This is a development preview, not a production-qualified release.

The app uses native Rust, GPUI Kit, Tokio, and SQLite. Keep the application and
project tooling in Rust. Do not reintroduce Tauri, a web frontend, Python, or
PowerShell helper scripts. Windows SDK resource compilation and GPU shaders
are part of the native build.

## Code map

| Path | Responsibility |
| --- | --- |
| `crates/skillissue-domain` | Settings, records, identity, provider metadata, shortcut policy |
| `crates/skillissue-core` | `NativeEngine`, persistence, credentials, capture, documents, provider HTTP/SSE |
| `crates/skillissue-platform` | Windows tray, native windows, global shortcuts, shell integration |
| `apps/skillissue-desktop` | GPUI application and UI; `main.rs` calls the library entry point |
| `apps/skillissue-desktop/src/ui/settings` | BYOK connections, Palepale account charts, general settings |
| `xtask` | Local Cargo build and GitHub prerelease upload |
| `packaging/windows` | Inno Setup per-user installer recipe |
| `vendor/gpui-pre-windows` | Patched Windows renderer; preserve upstream notices |
| `assets` | Runtime logo, fonts, and license files |

Dependencies flow from desktop to core/platform, then domain. Only desktop uses
GPUI. Rendering must not perform network or filesystem work. Return owned
background replies and reject stale generations after provider or session changes.

## Commands

Use Windows x64, MSVC C++ Build Tools, the Windows SDK, and the toolchain pinned
in `rust-toolchain.toml`. Run commands from the workspace root.

```console
cargo desktop
cargo desktop-build
cargo installer
cargo app-version
cargo fmt --all -- --check
cargo lint
cargo verify
```

Start with checks for the changed layer:

```console
cargo core-check
cargo core-test
cargo platform-check
cargo platform-test
cargo desktop-check
cargo desktop-test hosted_account_usage -- --test-threads=1
```

Aliases live in `.cargo/config.toml`. Share Cargo's normal target directory;
avoid `cargo clean` during routine iteration. UI tests use the `ui-review`
feature and a test platform. Passing tests does not verify live Windows behavior.

## Product behavior

- Normal startup shows the app. Capture protection is the stealth setting:
  enabled startup hides it. Toggling it applies capture affinity without hiding existing windows.
  Shortcuts and the tray can reveal windows again. Quit is available in the tray.
- BYOK has Text, Audio, and Images rows, with provider, key, model, and Test.
  Settings sync across windows. Unfinished keys stay in their editing window.
- Saved credentials remain in Windows Credential Manager, scoped to profile
  and endpoint. Keep saved keys out of input values, logs, events, and history.
  The UI shows a masked saved-key indicator instead.
- A separate Images provider handles screenshot questions. Text handles ordinary
  questions and notes. Audio uses its configured transcription adapter.
- Smart means Low/High reasoning effort for Text and Images. It does not start
  web research. Use supported provider parameters and never switch model IDs
  silently to enable it. Audio and generated notes have separate policies.
- Model dropdowns filter by role and trusted provider metadata. Model-list access
  does not prove inference or media support. Explicit Test sends synthetic data
  and may consume provider credits; Save must not trigger inference.
- Palepale pairing and metering use the fixed hosted service. Account charts
  project `/api/me` and `/api/me/overview?days=0`; wallet and pass balances stay
  separate. Money is integer microdollars; missing data is unavailable, not zero.
  The account panel uses GPUI chart/progress components and a Details toggle.

## Data and request rules

The default profile is `%APPDATA%/com.skillissue.desktop`. An absolute
`SKILLISSUE_CONFIG_DIR` selects another profile. Do not inspect, delete, migrate,
or publish a user's real profile as part of routine cleanup or tests.

Preserve atomic settings and credential rollback, profile locking, bounded input
and response sizes, cancellation, and provider/session revision checks. Accepted
requests may be billed: never automatically replay a failed POST. Drain pending
transcription before changing its configuration or ending capture.

History backup excludes credentials and settings. Restore validates records and
requires empty history. Preserve existing files on failed imports and exports.

## Changes and publishing

Use Conventional Commits, such as `fix(byok): preserve model selection`,
`refactor(ui): compact account charts`, or `docs: update agent guide`.
Keep user changes intact. Generated `target/`, `artifacts/`, `publish/`, and
`dist/` directories are ignored; do not commit their contents, credentials,
session data, or reference applications.

GitHub repository: `amangly/skillissue-rs`, branch `main`. There is no CI/CD
workflow. Run checks locally. For an authorized upload, commit and push first,
then use:

```console
cargo upload --prepare-only
cargo upload
```

The Rust task builds locally, packages matching committed source and checksums,
and uploads an unsigned GitHub prerelease through `gh`. It refuses dirty source,
requires remote `main` to match, preserves published previews, and can resume a
failed draft. Preview tags include the workspace version and commit. Upload does
not run tests; verify first. Do not change repository visibility or publish a
stable release unless the user requests it.

Version source: `[workspace.package].version` in `Cargo.toml`. Use
`cargo app-version patch`, `cargo app-version minor`, `cargo app-version major`,
or an explicit semantic version when a bump is requested.
The Rust command updates inherited workspace versions, Cargo.lock entries, and
the README badge together. `cargo app-version sync` repairs a manual version edit.
Keep versions out of prose and do not bump versions on ordinary builds.

## Verification limits and references

Preserve the user's logo in `assets/branding/source/logo.png` and its runtime
variants. Do not redesign it or import Cluely assets. `cargo installer` requires
Inno Setup 6.3+; `ISCC` may point to its compiler. Keep the installer AppId stable,
use Cargo's version, and preserve profiles and credentials during uninstall.

Live provider inference, physical audio devices, receiving capture software,
shortcuts outside the app, DPI/IME behavior, and long sessions need separate
acceptance. Signing and complete dependency license coverage remain open.
Report local tests, live checks, and deployed behavior separately.

Read `docs/architecture.md`, `docs/providers.md`, `docs/verification.md`, and
`docs/releasing.md` for details. A sibling `palepale-server` checkout may provide
the server contract; read its own agent instructions before working there.
