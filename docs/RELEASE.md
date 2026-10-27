# Releases and package publication

The release pipeline promotes tested bytes from main CI. Package managers then
install those bytes through their normal commands; no Python dispatcher is
required by users. No public release or registry publication exists yet.

## Version policy

[Semantic Versioning 2.0.0](https://semver.org/) governs versions. `Cargo.toml` is
the version authority; the root package entry in `Cargo.lock` must agree. Wheels,
archives, and system packages derive their version from Cargo. Git tags add a
leading `v`: version `0.1.0` corresponds to tag `v0.1.0`.

The public compatibility contract covers the exported Rust API, documented CLI
commands/flags and exit behavior, configuration keys and precedence, output-format
and byte/control handling guarantees, and shell initialization/session controls.
Changes to documented defaults or minimum supported Rust/OS versions must be
considered when choosing the version and called out in release notes. Terminal
presentation may improve without promising pixel-identical output or better
reading speed.

- Start at `0.1.0`. During initial `0.x` development, use patch bumps for compatible
  fixes, and minor bumps for new features or breaking changes. Describe every
  breaking change and its migration in the notes.
- From `1.0.0`, increment major for incompatible changes, minor for compatible
  features/deprecations, and patch for compatible fixes. Reset lower components.
- Never replace a published version's assets or move its tag. Ship corrections
  under a new version.
- The initial automation accepts normal `vMAJOR.MINOR.PATCH` tags only. Prerelease
  suffixes and build metadata require packaging/version mapping work before use.
  A `0.x` version is initial development; it does not itself require GitHub's
  prerelease flag.

## Prepare a release

1. Update Cargo.toml, run `cargo check` to refresh the root Cargo.lock version,
   inspect the diff, then run `cargo check --locked`. Merge through normal PR
   checks and wait for main-push CI, Dependency security, Gitleaks and CodeQL at
   the exact source commit. Main candidate attestations must also succeed.
2. Review [verification.md](verification.md), platform gates below, and the
   changelog. Tag that tested commit, beginning with `v0.1.0`:

   ```sh
   git fetch origin main --tags
   git tag -a v0.1.0 TESTED_MAIN_COMMIT_SHA -m 'Bionic Reader 0.1.0'
   git push origin v0.1.0
   ```

3. `release.yml` validates the Cargo/tag version and main ancestry, selects
   successful main-push CI at that exact commit, checks security workflows,
   verifies complete checksums and every candidate's main CI provenance, and
   prepares a draft GitHub Release. Native binaries are never recompiled here.
4. Inspect the draft, add a human summary/migrations/platform validation to its
   generated notes, and perform clean-machine install/use/upgrade/uninstall
   checks. Publish the draft only when these gates are satisfied. Creating a
   draft does not publish registries or notify them to upload packages.
5. A human-published release triggers `publish-packages.yml`. It rechecks the
   public release/tag/source identity, all checksums and original CI provenance
   before staging separate Homebrew, PyPI, npm and GitHub Packages inputs. Only
   explicitly enabled jobs publish. Job summaries distinguish disabled/skipped
   channels from successful publication.
6. Replace the README's pending notice only after the release and listed
   registry channels have actually been published and installed successfully.

The release contains six native archives, six wheels, six standalone Node
packages, six Linux system packages plus architecture-named aliases, seven npm
registry tarballs, a generated Homebrew formula, six SBOMs, RELEASE-SOURCE.json
and SHA256SUMS. Stable Linux asset names support native commands such as
`dpkg --print-architecture`; SemVer remains in package metadata and release tags.
The seven npm registry files are derived from the verified standalone files:
one launcher with six optional OS/CPU/libc packages, with no lifecycle scripts.

If a tag arrives before CI finishes, preparation fails without publishing. Retry
using the existing tag and optionally an exact successful main-push run ID:

```sh
gh workflow run release.yml --ref main -f tag=v0.1.0 -f ci_run=MAIN_PUSH_RUN_ID
```

Artifacts expire after 14 days. Rerun the original main-push CI at that commit
if they expire; manual CI dispatches have no main-push attestations. Old bundles
missing package files fail rather than producing incomplete releases. Preparation
does not overwrite a release. Inspect/delete only an incomplete unpublished draft
before retrying; published versions require a new tag/version.

## Publishing package managers

The publishing workflow is ready locally, but external ownership and credentials
must be configured by the maintainer. The public PyPI `bionic-reader` and npm
`@arturoaviles/bionic-reader` endpoints returned 404 during this review; that does
not reserve either name or prove account ownership. The public
`arturoaviles/homebrew-tap` repository now exists; its `bionic-reader` formula
still needs to be published.
Do not enable a channel before creating/owning its destination.

| Channel | Repository variable | External setup |
| --- | --- | --- |
| Homebrew | `PUBLISH_HOMEBREW=true` | Use the existing `arturoaviles/homebrew-tap` repository and its `main` branch; add `HOMEBREW_TAP_TOKEN` scoped to Contents write on that tap only. Override `HOMEBREW_TAP_REPOSITORY` if needed and update the documented tap name accordingly. |
| PyPI | `PUBLISH_PYPI=true` | Register `bionic-reader` and a trusted publisher for owner `arturoaviles`, repository `bionic-reader`, workflow `publish-packages.yml`, environment `pypi`. No PyPI password/token is needed. |
| npm | `PUBLISH_NPM=true` | Own the `@arturoaviles` namespace and all seven package names; configure trusted publishers for `publish-packages.yml`, environment `npm`. Initial bootstrap may use a scoped `NPM_TOKEN` secret until OIDC is configured for each package. |
| GitHub Packages | `PUBLISH_GITHUB_PACKAGES=true` | Enable package creation by this repository's GITHUB_TOKEN and configure the `github-packages` environment. The job alone has Packages write. Set package visibility/public access as appropriate; installation requires GitHub registry authentication. |

See the official [PyPI trusted publishing](https://docs.pypi.org/trusted-publishers/using-a-publisher/)
and [npm trusted publisher](https://docs.npmjs.com/trusted-publishers/) setup guides.

Create the corresponding GitHub environments (`homebrew`, `pypi`, `npm`,
`github-packages`) and restrict deployment to trusted refs. Environments are
separate from default PR CI; no PR job receives publishing credentials.
Publishing caches are disabled. PyPI uses a SHA-pinned PyPA action and OIDC;
npm uses provenance; GitHub's npm registry uses its scoped GITHUB_TOKEN.
The Homebrew job updates only Formula/bionic-reader.rb in the configured tap,
refuses same-version content changes, and leaves a newer tap version in place
when retrying an older release.

The native package names are `@arturoaviles/bionic-reader-darwin-x64`,
`-darwin-arm64`, `-linux-x64-gnu`, `-linux-arm64-gnu`, `-linux-x64-musl`, and
`-linux-arm64-musl`; the launcher is `@arturoaviles/bionic-reader`. Optional
platform dependencies must be uploaded before the launcher. Partial npm/GitHub
uploads are resumable: an existing name/version is skipped only when its SHA-512
integrity equals the current tarball; a mismatch fails. Versions are never
silently replaced. PyPI keeps existing-file rejection enabled; after a partial
upload, verify uploaded digests and publish only the missing wheels using the
approved publishing identity, rather than weakening its immutability checks.

To retry an individual enabled channel without republishing the others:

```sh
gh workflow run publish-packages.yml --ref main -f tag=v0.1.0 -f channel=homebrew
```

The channel choices are all/homebrew/pypi/npm/github-packages. Manual publication
requires an already public normal release; drafts, prereleases and moved or
mismatched tags are rejected. After setup, test actual `brew install`, pip/uv
installation and npm/Yarn/pnpm native dependency selection before removing the
pending availability notice.

## Verify release provenance

Compare each downloaded asset's SHA-256 with its entry in the same release's
SHA256SUMS before using it. A complete download can be checked with
`sha256sum --check --strict SHA256SUMS` on Linux or
`shasum -a 256 --check SHA256SUMS` on macOS.

RELEASE-SOURCE.json records the original CI commit/run. Verify a native archive,
wheel, standalone Node tarball or Linux package against that commit:

```sh
gh attestation verify ASSET_FILENAME \
  --repo arturoaviles/bionic-reader \
  --signer-workflow arturoaviles/bionic-reader/.github/workflows/ci.yml \
  --source-digest RELEASE_COMMIT_SHA --deny-self-hosted-runners
```

Native archives/wheels also have CycloneDX attestations: add
`--predicate-type https://cyclonedx.org/bom`. Per-target SBOM filenames are
renamed at release assembly without changing their contents. Linux aliases
retain the original package bytes. Registry packages and Homebrew metadata are
derived from those verified inputs and covered by the aggregate release manifest;
publication rederives registry files and compares their complete bytes before
credentials are made available. Original main CI attestations do not cover the
new registry packaging metadata. See [CI integrity](ci/README.md#candidate-integrity).

## Platform and publication gates

- Native archives and wheels are exercised on matching macOS/Linux runners.
  GNU builds have symbol checks and offline glibc 2.28 execution on both CPUs.
  System packages build on both GNU CPUs; exact-file Debian/Fedora/Arch
  installation containers currently cover x86-64. ARM64 distribution installation
  remains a release validation gate.
- macOS deployment target 11.0 is not proof of macOS 11 runtime compatibility;
  CI uses macOS 15. Signing/notarization is not configured. Record tested OS
  versions and migration requirements in release notes.
- Direct RPM/Arch downloads are unsigned. Hosted APT/DNF/pacman repositories
  require publisher keys, hosting/TLS and trust distribution; their existing
  signed-repository builders/tests remain in CONTRIBUTING.md. They are not
  automatically deployed by the registry workflow.
- Snap remains a local classic/devel candidate. Snap Store approval and an
  approved publishing identity are external gates. Cargo uses Git/source until
  crates.io publication is separately configured. Windows/container images are
  outside the current supported product matrix.
- First publication requires fresh install/removal checks; later versions need
  upgrades too. CI success does not close the open 100% coverage target or every
  minimum-OS gate. Record exact commits/run links for new measurements.

See [INSTALL.md](INSTALL.md), [BUILD.md](BUILD.md), and the
[Glow distribution comparison](ci/distribution-review.md) for the related work.
