# Versioned GitHub Release packages

Distribute prebuilt archives, Linux system packages, Python wheels, and
self-contained Node tarballs through
GitHub Releases so users can install without a Rust toolchain or a checkout.
Promote checksum-verified and attested artifacts from successful main CI at the
exact version tag, rather than recompiling native binaries during publication. Registry credentials
are isolated in optional publication jobs; hosted signed package repositories
remain separate work.

Cargo owns the version and tags use `vMAJOR.MINOR.PATCH`, beginning at `v0.1.0`.
Published assets and tags are never replaced; fixes require a new version.
The compatibility contract and initial-development version rules are documented
in [the release policy](../RELEASE.md#version-policy). This extends ADR 0002's
repository-based distribution constraint to repository-owned GitHub Releases;
public registries remain separate work. Release preparation creates a draft so
platform validation and release notes can be completed before publication.

The README retains native package-manager entry points and a brief getting-started
path; detailed installation, usage, configuration and shell integration live in
dedicated guides. A separate, opt-in post-release workflow publishes the verified
payloads to Homebrew, PyPI, npm, GitHub Packages, crates.io and the Go module proxy. This supersedes the initial
Python-dispatcher/local-tap approach and the decision to defer every registry:
package managers should own platform selection, upgrades and removal. Publication
credentials and external namespaces must be configured before enabling a channel.
Cargo publication uses GitHub OIDC in the `crates-io` environment and the original
successful main CI crate candidate. Repackaging the release commit must reproduce
that candidate before credentials are requested; registry checksums must match
after publication and on retries. Snap retains its repository route; hosted
signed Linux repositories and Snap Store approval remain separate gates.

Go uses the same root release tags, containing a complete Go module and its
generated embedded engine. CI reproduces and attests that engine separately;
release preparation binds it to the tagged source. The Go publication channel
requests proxy indexing with checksum-database verification and tests an
independent consumer. It needs no registry token or write-capable job. ADR 0006
defines the Go runtime and API boundary.
