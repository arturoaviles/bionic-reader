# Distribution review against Glow

Reviewed [Glow's README](https://github.com/charmbracelet/glow),
[build workflow](https://github.com/charmbracelet/glow/blob/main/.github/workflows/build.yml),
[tag workflow](https://github.com/charmbracelet/glow/blob/main/.github/workflows/goreleaser.yml),
[shared release workflow](https://github.com/charmbracelet/meta/blob/main/.github/workflows/goreleaser.yml)
and [release configuration](https://github.com/charmbracelet/meta/blob/main/goreleaser-glow.yaml).
Glow keeps its introduction, installation, reader, CLI and configuration brief.
Its native commands depend on real distribution infrastructure: a Homebrew tap,
signed Linux packages/repositories and publishing identities. Its tag workflow
uses a shared GoReleaser setup for archives, package formats, checksums, SBOMs,
provenance/signing and additional platform channels.

Bionic Reader retains its existing Rust matrix, Miri/fuzz/coverage gates, six
native runtime checks, dependency/security policies, exact-source candidate
checksums and main CI attestations. It does not need GoReleaser Pro or Glow's
Go-specific build setup. Copying only the visual installation commands would
advertise unavailable packages.

| Area | Adjustment |
| --- | --- |
| README | Short introduction and examples; every manager remains represented by native commands. |
| Candidate snapshots | Retain per-commit native workflow artifacts and main provenance; publish registries only after a normal release. |
| Detailed docs | INSTALL, USAGE, CONFIGURATION, SHELL, BUILD and RELEASE guides. |
| User installer | Remove the Python dispatcher; native managers own platform selection, updates and removal. |
| Linux downloads | Add stable architecture-specific asset aliases; use native architecture queries in download commands. |
| npm registries | Derive a launcher and six optional native packages from verified target bundles. |
| Homebrew | Generate release formula and add a scoped tap update job. |
| PyPI | Add OIDC publication of all six wheels through the pinned PyPA action. |
| GitHub Packages | Add a separate npm-registry job with Packages write only there. |
| Publication boundary | Verify the public release and original CI inputs before independent opt-in channel jobs receive credentials. |
| Recovery | Channel-specific retries; npm skips existing uploads only after complete tarball integrity comparison. |

Public tap creation, namespace ownership, trusted-publisher setup and environment
credentials still need maintainer configuration. No credentials, signing
identities, remote repositories or public releases were created during this work.
Signed hosted Linux repositories, Snap Store approval, minimum-macOS validation
and notarization remain explicit gates. These do not prevent preparing the
native package-manager publication code for review.
