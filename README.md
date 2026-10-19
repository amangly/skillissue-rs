<p align="center">
  <img src="assets/branding/readme-banner.svg" alt="skillissue logo" width="96" height="96">
</p>

<h1 align="center">skillissue-rs</h1>

<p align="center">
  Native Windows assistant, built in Rust.<br>
  Transcribe conversations, ask about your screen, and keep sessions locally.
</p>

<p align="center">
  <a href="Cargo.toml"><img src="assets/branding/version.svg" alt="Workspace version"></a>
  <a href="rust-toolchain.toml"><img src="https://img.shields.io/badge/Rust-1.99.1-dea584?style=flat-square&amp;logo=rust" alt="Rust 1.99.1"></a>
  <a href="#get-started"><img src="https://img.shields.io/badge/Windows-11-649bea?style=flat-square" alt="Windows 11"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-28ad83?style=flat-square" alt="Apache-2.0 license"></a>
</p>

<p align="center">
  <a href="https://github.com/amangly/skillissue-rs/releases">Download</a> ·
  <a href="#get-started">Build</a> ·
  <a href="docs/providers.md">Providers</a> ·
  <a href="CONTRIBUTING.md">Contribute</a>
</p>

Use [Palepale](https://palepale.tech/), an affordable OpenAI-compatible API for
coding tools and apps, or bring your own API keys (BYOK).

In **Settings → AI providers → BYOK**, choose a provider and model for Text,
Audio, and Images separately. For example, use DeepSeek for Text and Palepale for Audio.
Palepale rows use your connected account; other providers use your saved API key.
Compare input, cached input, and output rates on [Palepale](https://palepale.tech/),
or explore its plans. Provider usage costs apply.

## Why I built this

I built this for people with skill issues. Like me.

## Features

Live transcription, screen context, and AI assistance in a native floating overlay.

- Floating overlay, tray controls, and global shortcuts.
- Text, Audio, and Images providers with model pickers, key tests, and reasoning effort controls.
- Palepale pairing, pass allowances, balances, and usage charts.
- Local transcripts, session history, notes, and TXT/PDF/DOCX references.
- Capture protection with shortcut access to hidden windows.

The app opens visibly by default. Stealth mode starts hidden and requests capture
exclusion. Support depends on the screen-sharing or recording software.

## Get started

Download the Windows x64 setup executable from [Releases](https://github.com/amangly/skillissue-rs/releases).
It installs for your user in `%LOCALAPPDATA%\Programs\skillissue` and adds a Start
menu shortcut. Open the app and follow **Set up skillissue**: connect AI, test
audio if needed, and review shortcuts. The standalone executable is also available.

The app checks for newer stable releases at startup and every six hours.
Settings → General lets you check manually, download a verified installer, and
open setup when your session is finished. Preview releases are excluded.

To build, use Windows 11, Visual Studio C++ Build Tools, the Windows SDK, and the
Rust toolchain pinned in [rust-toolchain.toml](rust-toolchain.toml).

```console
cargo desktop
cargo desktop-build
cargo installer
```

The build is `target/release/skillissue.exe`. Open AI Providers to connect
Palepale or choose Text, Audio, and Images providers and enter your API keys.
Keys use Windows Credential Manager. Sessions stay in
`%APPDATA%/com.skillissue.desktop`; an absolute `SKILLISSUE_CONFIG_DIR` selects
another profile. Selected context is sent to your configured provider.

Building the installer requires [Inno Setup](https://jrsoftware.org/isdl.php) 6.3+
(`ISCC` can point to `ISCC.exe`). Uninstall from Windows Settings; local history
and saved keys are preserved. The login entry is removed during uninstall.
Disconnect providers in the app first to remove their saved credentials.

## Development

```console
cargo fmt --all -- --check
cargo lint
cargo verify
cargo upload
```

`cargo upload` builds locally and uploads an unsigned prerelease with matching
source and checksums. Commit and push to `main` first. Use `--prepare-only` to
prepare files without uploading. There is no CI/CD pipeline.

Live provider, audio-device, capture, and
long-session checks remain open. Read [verification](docs/verification.md),
[architecture](docs/architecture.md), and [upload details](docs/releasing.md).
Coding agents should start with [AGENTS.md](AGENTS.md).

Versions come from `Cargo.toml`. `cargo app-version` shows the current version;
`cargo app-version patch`, `minor`, `major`, or an explicit version updates the
workspace, lockfile, and badge together.

## License

Copyright © 2026 amangly.

[Apache-2.0](LICENSE). Third-party code, fonts, and assets retain their own
licenses; see [notices](THIRD_PARTY_NOTICES.md). Builds are unsigned;
[signing is deferred](docs/code-signing-policy.md).

<sub>Cluely is a trademark of its respective owner. skillissue-rs is an independent,
open-source project and is not affiliated with, sponsored by, or endorsed by Cluely.
The name is used only to describe similar workflows.</sub>

