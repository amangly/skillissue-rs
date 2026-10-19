# Verification

The repository separates deterministic tests, native startup checks, visual review,
and real-device or service acceptance.

## Automated checks

Run these commands from a clean Windows MSVC environment:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features skillissue-desktop/ui-review --locked -- -D warnings
cargo test --workspace --features skillissue-desktop/ui-review --locked -- --test-threads=1
```

The tests cover settings rollback, shortcut conflicts, profile credentials,
durable history, notes revisions, document limits, provider HTTP/SSE framing,
pairing cancellation, context bounds, capture draining, request cancellation,
and GPUI input/layout behavior. Loopback fixtures do not contact production
providers or send paid requests.

## History backup and recovery

Settings > General > Back up saved history writes a standalone SQLite snapshot
of committed transcripts, answers, and notes. It excludes provider credentials,
settings, reference documents, and unfinished requests or transcription jobs.
The backup is plaintext and should be stored like the session contents it holds.
An existing destination is never replaced. The operation stages, checks, and
flushes the database before publishing it; the destination filesystem must support
hard links (such as NTFS). Failure leaves existing backups and live history intact.

Settings > General > Restore saved history imports a backup into an empty history.
It refuses occupied history, active capture, pending transcription, and an active
answer. It reads the backup without modifying it, validates its version, integrity,
and serialized records, and commits all imported rows together. It imports data
into the application's schema rather than installing backup schema objects.
Backups larger than 256 MiB are currently rejected. Interrupted capture and notes
generation are marked as interrupted or errored rather than restarted.

To recover without changing an existing profile, close the app and launch with
`SKILLISSUE_CONFIG_DIR` set to an absolute new empty profile directory and run
`cargo desktop`, then
use Restore saved history and check History. Configure providers and reimport
documents separately. Keep the original profile and backup until recovered
sessions have been checked. Settings and document migration are not included.

The `backup_recovers_committed_wal_content_without_replacing_existing_files` test
opens the backup independently and verifies saved content and overwrite refusal.
The restore regression also checks malformed-record rollback and refusal to
replace occupied history. The Settings pickers still require interactive acceptance.

## Native build checks

Run `cargo desktop-build` to build `target/release/skillissue.exe` with the pinned
Rust toolchain. Run formatting, Clippy, workspace tests, and the release build
locally through Cargo; GitHub Actions is disabled. `cargo upload` rebuilds locally
and publishes a development prerelease with matching committed source and
checksums. It does not run tests for you. Successful compilation does not establish
runtime, capture, audio, or provider acceptance. Signing is not automated.

## Acceptance still required

Export reveal was checked on the local Windows desktop on 2026-10-19 using the
platform adapter's explicitly ignored `native_reveal_selects_acceptance_file`
test. Explorer opened the fixture's parent folder and visibly selected
`Meeting notes, 验证.md`; its accessibility status reported one item selected.
This verifies native selection with spaces, comma, and Unicode. It does not
verify the complete dashboard/overlay save-dialog flow or a release executable.
The test requires `SKILLISSUE_REVEAL_TEST_FILE` pointing to a disposable existing
absolute file and is excluded from normal test runs because it opens Explorer.

Real Windows checks remain separate: microphone accuracy and device recovery,
IME and clipboard behavior, shortcuts while another application is focused,
mixed-monitor DPI, capture exclusion in receiving applications, long sessions,
authenticated provider inference, account revocation, billing, and signed release
installation. The current executable is an engineering preview and is unsigned.

## Third-party notices

Windows installer checks: Inno Setup compilation, silent per-user install into
a temporary workspace directory, same-version reinstall, and uninstall passed
on the build host. Required executable, runtime DLLs, notices, and uninstaller
were present. The app was not launched during this smoke test, and no real
profile was accessed. Clean-machine launch, interactive shortcuts, and upgrades
between different releases still need Windows VM acceptance.

The setup flow has a GPUI test for compact bounds in both themes, one visible
step at a time, settings navigation, Back/Next, completion, and reopening.
Branding uses the maintainer's supplied logo.

Supplemental upstream texts in `assets/licenses/dependencies/` retain commit URLs
and hashes. `THIRD_PARTY_NOTICES.md` lists the bundled code, fonts, and artwork.
There is no automatic license collector. A complete inventory for the locked
Windows dependency closure and resolution of missing license texts remain
required before distributing the first release.
