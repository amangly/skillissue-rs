# Third-party notices

Project-owned code and maintainer-authorized artwork use Apache-2.0. Existing
third-party code, fonts, and notices retain their original licenses.

- Tokio Tungstenite and Tungstenite provide Deepgram's WebSocket transport.
  Their MIT notices are retained in `assets/licenses/dependencies/`.

- Earshot 1.2.2 provides embedded neural speech detection under the MIT license.
  Upstream commit: `c400f15bd232dda30a297eec90235bd09066cab7`.
  License: `assets/licenses/dependencies/earshot-MIT.txt`, copied from
  [upstream](https://github.com/pykeio/earshot/blob/c400f15bd232dda30a297eec90235bd09066cab7/LICENSE-MIT).
- The GPUI Windows backend retains its Apache-2.0 license in
  `vendor/gpui-pre-windows/LICENSE-APACHE`; renderer changes are documented beside it.
- Microsoft's adapted alpha-correction shader retains its MIT copyright and
  license notice. See `vendor/gpui-pre-windows/src/alpha_correction.hlsl` and
  `vendor/gpui-pre-windows/LICENSE-MIT`.
- JetBrains Mono retains SIL OFL 1.1 in `assets/licenses/JetBrainsMono-OFL.txt`.
- Windows installers include Microsoft's x64 Visual C++ runtime DLLs from the
  build machine's Visual Studio redistributable directory. These retain Microsoft's
  license; see [Visual C++ redistribution terms](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files).
- The maintainer supplies and authorizes `assets/branding/source/logo.png`.
  Runtime variants preserve the supplied logo.

Supplemental upstream notices and their source hashes live under
`assets/licenses/dependencies`. Review the dependency versions in `Cargo.lock`
and their upstream license files before distributing a binary. This index and
the checked-in supplemental files do not establish complete license coverage.
A complete dependency license inventory remains required before the first release.

## Heroicons

Interface icons use Heroicons v2.2.0 (MIT), by Tailwind Labs:
https://github.com/tailwindlabs/heroicons/tree/v2.2.0
Source commit: `0435d4ca364a608cc75e2f8683d374e55abbae26`.
The SVGs are embedded locally; GPUI icon paths are compatibility aliases.
The license is included in `assets/licenses/heroicons-LICENSE`.
