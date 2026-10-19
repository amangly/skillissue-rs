# Code signing policy

**Status:** Deferred until release preparation. No signing integration is configured,
and no application has been submitted or accepted.

Planned service after acceptance: Free code signing provided by SignPath.io,
certificate by SignPath Foundation.

## Scope

This policy covers the Windows `skillissue.exe` release artifact. The project
team signs only binaries built from this repository's source and build scripts.
Third-party libraries and upstream assets keep their own licenses and are not
represented as project-authored code.

## Roles

- **Maintainer / author:** [amangly](https://github.com/amangly),
  [amaralkaff@gmail.com](mailto:amaralkaff@gmail.com).
- **Reviewer:** the maintainer reviews outside contributions before merging.
- **Release approver:** the maintainer, subject to assignment of the corresponding
  role in SignPath. Every signing request requires manual approval.

Source repository: [amangly/skillissue-rs](https://github.com/amangly/skillissue-rs).
It must be public before applying to the foundation program. GitHub and SignPath
accounts must use multi-factor authentication.

## Build and release

Maintainers run formatting, Clippy, tests, and release builds locally through Cargo.
`cargo upload` uploads an unsigned development preview from committed source.
There is no GitHub Actions workflow. A future signing workflow must
verify source origin, restrict allowed release commits, and require manual
approval for each signing request. Unsigned previews must be labelled as unsigned.
See [`releasing.md`](releasing.md) for the remaining release work.

## Privacy

Audio, screenshots, selected document excerpts, and conversation context go to the
provider selected by the user when its associated feature is used. Reopening a
configured hosted connection can refresh catalog and account status. See
[`providers.md`](providers.md) for network behavior. Local history and credentials
remain on the device; the configured service's privacy and retention terms govern
submitted content.

## Verification

Users can verify the Authenticode signature on `skillissue.exe` with Windows
Explorer or `Get-AuthenticodeSignature`. Publish checksums and the matching source
bundle beside each binary. An unsigned preview is never presented as signed.
