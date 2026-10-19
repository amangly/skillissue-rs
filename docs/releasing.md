# Release preparation

Builds are development previews. Checks and builds run locally through
Cargo. GitHub Actions is disabled, and signing is not configured.

## Upload a local preview

Install Inno Setup 6.3+ on the build machine (or set `ISCC` to `ISCC.exe`).
`cargo installer` builds the release app and a per-user Windows 11 x64 installer
under `target/installer`. Its filename and version come from Cargo metadata.
`cargo upload` includes this installer in the preview and SHA256SUMS.txt.

The installer uses `%LOCALAPPDATA%\Programs\skillissue`, adds a Start menu
shortcut, offers an optional desktop shortcut, and can open the app at the end.
It does not add a PATH entry or enable autostart. Windows versions below 11 and
non-x64 systems are rejected. Upgrades use a stable AppId and ask to close the app.
Uninstall removes installed files, preserving profiles and credentials.

The installer bundles `vcruntime140.dll` and `vcruntime140_1.dll` from Visual
Studio's x64 redistributable directory. `SKILLISSUE_VC_REDIST_DIR` can select
that directory explicitly; the builder rejects non-x64 PE files. These files
remain under Microsoft's redistribution terms. The standalone executable needs
the Visual C++ x64 runtime already installed. Inno Setup has separate commercial
licensing terms; see its [download page](https://jrsoftware.org/isdl.php).

Validate install, upgrade, shortcut launch, and uninstall on a clean Windows VM
before a stable release. Local compilation does not establish clean-machine support.

Run local checks, commit, and push to `main`. With Git and the GitHub CLI installed
and authenticated, run `cargo upload` on x64 Windows with the pinned MSVC toolchain.
The Rust `xtask` builds the desktop executable and uploads it, a matching Git
source archive, the license and notices index, and SHA-256 checksums to a GitHub
prerelease. It does not run checks or install tools. Use `cargo upload --prepare-only`
to prepare files under Cargo's `target/uploads/` without contacting GitHub.

The command refuses uncommitted changes and source changes during compilation.
Upload requires the commit to match remote `main`. Tags contain the version and
commit, in the form `v<workspace-version>-dev.<commit>`. Uploads start as drafts and publish only
after all assets are uploaded. Failed drafts can be retried; published previews
are preserved. The repository's visibility controls access to these downloads.

## Versioning

`Cargo.toml` under `[workspace.package]` is the version source. All workspace
packages inherit it. The UI, Windows metadata, and upload titles/tags use Cargo
metadata. `cargo app-version` displays it; `cargo app-version patch`, `minor`, `major`,
or `<semver>` updates the manifest, owned lockfile entries, and local README
badge together. Commit and push these changes before uploading.

After editing the manifest by hand, run `cargo app-version sync` to refresh the
lockfile and badge. Upload rejects a stale badge. The badge is local, so it
also works while this repository is private. Version bumps are explicit;
ordinary builds do not change the version.

## Before a stable release

The app checks GitHub's latest stable release at startup and every six hours.
The release feed must be public; a private repository returns an unavailable
status to the app. Never embed a GitHub access token in distributed binaries.
Publish a strictly newer semantic version to offer an update; replacing assets
under the same version does not trigger one. Preview tags are excluded.
Include `skillissue-{version}-windows-x64-setup.exe` with GitHub's SHA-256 asset
digest. The app validates the repository URL, size and digest before staging the
installer in a uniquely named `skillissue-update-*` directory in Windows Temp,
then verifies it again before launch. This checks integrity against GitHub
metadata; it is not Authenticode signing. The user opens the interactive installer
after recording and requests finish. Profiles and credentials remain untouched.

1. Run `cargo fmt --all -- --check`, `cargo lint`, `cargo verify`, and
   `cargo desktop-build` on Windows with the pinned MSVC Rust toolchain.
2. Complete the device, capture, provider, and UI acceptance checks in
   [verification.md](verification.md).
3. Resolve missing dependency license texts and prepare complete notices for the
   locked Windows dependency closure. Preserve upstream copyright notices.
4. Prepare matching source, executable, notices, and checksums from one reviewed
   commit. Verify the executable's product version matches `Cargo.toml`.
5. Set up and review signing before presenting any binary as signed. Until then,
   identify downloadable previews as unsigned. See [code-signing-policy.md](code-signing-policy.md).

The repository is [amangly/skillissue-rs](https://github.com/amangly/skillissue-rs).
Project contact: [amaralkaff@gmail.com](mailto:amaralkaff@gmail.com).

## Portable removal

Disable Open at login in Settings, disconnect providers, quit the app, and remove
the extracted application directory. Remove `%APPDATA%/com.skillissue.desktop`
only if local settings, documents, and session history should also be deleted.
For a custom profile use its configured directory. Local disconnect removes saved
credentials; server-side device credentials may require revocation with the provider.
