# Contributing

Discuss substantial product or architecture changes in an issue before opening a
large pull request. Small fixes and focused improvements can go directly through
the normal pull-request flow.

Before submitting a change, run:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features skillissue-desktop/ui-review --locked -- -D warnings
cargo test --workspace --features skillissue-desktop/ui-review --locked -- --test-threads=1
```

During development, check the layer you changed first:

```console
cargo core-check
cargo core-test history::tests -- --test-threads=1
cargo platform-check
cargo platform-test -- --test-threads=1
cargo desktop-check
cargo desktop-test pending_audio_notice -- --test-threads=1
```

Core and domain checks exclude GPUI and the desktop crate. `check` avoids linking
an executable; use the filtered tests for the behavior you changed. Keep normal
debug builds in the same target directory and avoid `cargo clean` during routine
iteration. Run `cargo lint` and `cargo verify` for the full gates above before
shipping. These aliases are in `.cargo/config.toml`.
Checks run locally; there is no CI/CD workflow. After checks pass, commit and push
to `main`, then use `cargo upload` to build and upload a development prerelease.

Use `cargo desktop` to run the app and `cargo desktop-build` for a release build.
Keep release builds for runtime measurements and distribution preparation.
The pinned standard Rust toolchain is the supported build path.

Use Conventional Commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, or
`chore:`, followed by a short description. An optional scope identifies the area,
for example `fix(byok): preserve saved model selection`.

Keep the crate boundaries in [`docs/architecture.md`](docs/architecture.md).
Provider work belongs in `skillissue-core`; GPUI entities and Windows message-loop
objects stay in the desktop or platform crates. Do not add credentials, profiles,
recordings, generated artifacts, or build output to a commit.

Pull requests should explain the user-visible behavior, affected data or provider
boundaries, tests run, and any Windows-only acceptance still needed. Changes to
release workflows, build scripts, or signing configuration require the same review
as source code.

By submitting project-owned contributions, you agree to license them under
Apache-2.0. Preserve all third-party copyright and license notices.
