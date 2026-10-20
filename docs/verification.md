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

## Audio workflow checks

The capture tests check neural silence rejection at 8–192 kHz and resampling
across packet boundaries. Scripted speech decisions check pre-roll, silence
cutoffs, short-segment rejection, duration limits, and partial flushes separately
from model accuracy.
Loopback integration tests verify that delayed transcription blocks the answer
request, the explicit retry includes the saved transcript, and cancellation
during preparation never sends an answer POST. GPUI tests cover progress states
and stale/cancelled preparation messages.

Run the offline benchmark with optional mono/stereo WAV fixtures:

```console
cargo run --locked --release -p skillissue-core --example audio_benchmark -- C:\fixtures\quiet-speech.wav C:\fixtures\overlap.wav
```

Without arguments, the benchmark runs synthetic silence, a tone, and broadband
noise. For each WAV, it also tests quieter audio (-20 dB), added clicks, and a mix
with a copy shifted by three seconds. JSON output reports processing time,
segment duration, and when each segment became available, to the nearest 10 ms
input packet. The benchmark reads only the supplied files and uses no profile,
microphone, or provider connection.

Use the same recordings and hashes for comparisons. Real recordings of quiet
speech, keyboards, overlapping speakers, and long utterances still need testing.
This benchmark does not measure transcription accuracy or provider latency.

An initial local check rejected the five-second silence and 440 Hz tone
fixtures. Broadband noise produced a false positive: a 528 ms startup segment.
A 60-second public speech fixture produced segments within the duration limit.
These checks did not score missed speech or word accuracy.
Fixture source: [Silero English sample](https://models.silero.ai/vad_models/en.wav).
SHA-256: `89f17d9c94c4b31eb320f424628bcbc920abaddbee6e2760fd868bfb1d9a2e47`.
The downloaded recording and benchmark output stay in the ignored `target/`
directory.

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
