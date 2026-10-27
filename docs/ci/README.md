# CI tools and rollout

The complete PR test and installation matrix remains enabled. `CI gate` requires
every PR matrix job to succeed, including native glibc 2.28 execution. Failure,
cancellation, missing jobs and unexpected skips block it. Attestation is explicitly
excluded because it runs only on trusted main pushes. `make ci-policy-check`
prevents new PR jobs from silently escaping aggregation.

## Local tools

Install Rust through rustup; the project toolchain file selects 1.85.1. Install
Python checkers in a virtual environment:

```sh
python3 -m venv .venv
.venv/bin/python -m pip install -r packaging/python/requirements-checks.txt
export PATH="$PWD/.venv/bin:$PATH"
rustup toolchain install stable --profile minimal
cargo +stable install --locked typos-cli --version 1.51.1
cargo +stable install --locked zizmor --version 1.31.1
cargo +stable install --locked lychee --version 0.24.2
cargo +stable install --locked cargo-deny --version 0.20.2
cargo +stable install --locked cargo-cyclonedx --version 0.5.9
cargo +stable install --locked cargo-mutants --version 27.1.0
cargo +stable install --locked hyperfine --version 2.0.0
cargo +stable install --locked cargo-audit --version 0.22.2
```

Install [actionlint 1.7.12](https://github.com/rhysd/actionlint/releases/tag/v1.7.12)
from its checksum-verified release. On Linux x86-64,
`scripts/install-workflow-tools.sh DIRECTORY` installs actionlint and lychee with
fixed release digests. CI uses SHA-pinned taiki-e/install-action with fixed tool
versions, checksum verification and no fallback for supported tools. cargo-fuzz
0.13.2 retains a locked source installation because it has no upstream prebuilts.
ShellCheck, Bash, Zsh, fish, Node.js and Ruby remain required for existing checks.

`make verify` includes fast offline workflow, spelling and local link checks.
Spelling covers tracked files; generated completions and lockfiles are excluded.
The small dictionary records protocol names and intentional partial-word test
fixtures. External links are checked weekly with bounded retries and private,
loopback and link-local addresses excluded. Reserved `.invalid`/`.test` domains
and example.com/example.org examples are excluded explicitly.

Workflow lint uses actionlint, yamllint and zizmor's annotation format, preserving
zizmor's failure status. The single inline zizmor exception covers the existing
pinned, script-disabled Corepack bootstrap. It does not suppress other findings.

`make ci-negative-check` checks that real tools reject broken anchors, duplicate
YAML keys, workflow expression injection, an unapproved license and a Git source.
It also proves that the NCSA exception does not extend to libfuzzer-sys 0.4.10,
0.4.11 or 0.4.14, or to an unrelated crate at the approved version.
Unit tests exercise altered/incomplete checksum manifests and failed/skipped CI.

## Dependency policy and caches

`make deny` applies the same license/source/version policy to both locked graphs,
including dev and build dependencies, across all six supported targets. The allow
list is MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2-Clause, BSD-3-Clause,
ISC, Unicode-3.0 and Zlib. Unlicensed crates and unsatisfied expressions fail.
The user approved one exception on 2026-10-26: NCSA for exactly libfuzzer-sys
0.4.13 in the fuzz toolchain, replacing the earlier 0.4.10 exception. Registry
wildcards, Git sources and unexpected registries fail; first-party path
requirements are permitted. Duplicate versions
remain warnings. Dependency Review checks all scopes from low severity upward;
its independent license check is disabled. The repository dependency graph was
confirmed available through its SBOM API before rollout.

Rust caches separate OS, architecture, native target, compiler and instrumentation
lanes. rust-cache adds compiler, Cargo flags/environment and workspace lockfile
hashes, including both application and fuzz lockfiles. Only main push runs save
caches. Scheduled reports restore where possible without saving. Linux x86-64
MSRV source/Git/package installation intentionally has no restored cache.

## Candidate integrity

Each native target's workflow artifact contains a flat bundle: archive, wheel, standalone Node tarball,
`sbom.cdx.json` and relative-path `SHA256SUMS`. Binary-mode CycloneDX 1.5 SBOMs use
the matching target and default feature configuration, excluding dev dependencies.
Checksums are generated after staging every file, then verified before extraction
or installation. Local host-only wheels retain their existing installation checks.

After downloading a candidate artifact into `bundle`, verify before using it:

```sh
python3 scripts/candidate-checksums.py bundle --verify
```

Checksums detect corruption; main-push attestations bind content to its source.
Use the exact main commit recorded on the workflow run:

```sh
gh attestation verify bundle/ARCHIVE.tar.gz \
  --repo arturoaviles/bionic-reader \
  --signer-workflow arturoaviles/bionic-reader/.github/workflows/ci.yml \
  --source-digest COMMIT_SHA --deny-self-hosted-runners
gh attestation verify bundle/ARCHIVE.tar.gz \
  --repo arturoaviles/bionic-reader \
  --signer-workflow arturoaviles/bionic-reader/.github/workflows/ci.yml \
  --source-digest COMMIT_SHA --deny-self-hosted-runners \
  --predicate-type https://cyclonedx.org/bom
```

Repeat for the wheel. PR and manual-dispatch artifacts have no attestation.
The privileged attestation job downloads only same-run artifacts, verifies
digests and never executes candidate code. Only it receives id-token,
attestations and artifact-metadata write permissions. CodeQL uses the supported
`pull_request` flow, including forks/Dependabot, with no privileged PR checkout.

GNU runtime checks use a digest-pinned Debian Buster image with glibc 2.28 on
matching x86-64/ARM64 runners, without network access or package installation.
The existing symbol-version check remains. macOS 11 runtime validation remains
a release prerequisite; modern macOS runners do not establish that floor.

## Merge enforcement and first scheduled runs

[The original ruleset snapshot](main-ruleset-before.json) preserves the active
ruleset returned by GitHub. Its ref include list was empty. [The proposed
update](main-ruleset-proposed.json) explicitly targets `refs/heads/main`, preserves
deletion/force-push protection and bypass actors, requires PRs with zero reviewer
approvals and up-to-date branches, and blocks high/critical CodeQL security alerts.
Required GitHub Actions contexts are:

- CI gate
- Dependency advisory audit
- Dependency policy
- Dependency Review
- Gitleaks secret scan
- CodeQL (rust)
- CodeQL (python)
- CodeQL (javascript-typescript)

Do not activate these requirements until the contexts have registered and passed.
First merge the validated workflow changes so main receives the initial CodeQL
baseline and candidate attestations. Then use the exact tested PR commit:

```sh
python3 scripts/apply-main-ruleset.py TESTED_PR_SHA
python3 scripts/apply-main-ruleset.py TESTED_PR_SHA --apply
gh workflow run scheduled-assurance.yml --ref main -f suite=all
```

The helper checks exact successful GitHub Actions contexts on the PR and main,
requires the reviewed workflow files on main and a successful main CI run
(including attestations), rejects conflicting ruleset changes, and takes a fresh
snapshot under `target/ci-rollout` before applying. It leaves the repository
unchanged when prerequisites are missing. Record the resulting rule URL, main
attestation verification and initial scheduled run URLs in the verification log.
Validate fork/Dependabot PR execution after the workflows land on main.

Scheduled mutation reports use two workers and a 45-minute budget per module.
Baseline/tool failures remain fatal; survivors and mutant timeouts are advisory.
Weekly Divan/hyperfine reports include throughput, allocations, startup, binary
size and machine/compiler details. Timing has no regression threshold. Nightly
fuzzing uses ASan and does not replace thread-sanitizer testing. Diagnostics retain
14 days. Coverage floors, scan-work bounds and memory gates remain mandatory;
the requested 100% coverage target remains unmet.

## GitHub Release promotion

GNU target bundles also include direct-install `.deb`, `.rpm`, and Arch packages
before checksums and main-push provenance are generated. The Debian/Fedora/Arch
smokes install the exact x86-64 bundle packages rather than rebuilding them. The separate
`release.yml` workflow verifies a version tag and a successful main-push CI run at
that exact commit, then verifies and promotes all six attested target bundles to
a draft GitHub Release. It requires successful security workflow runs at the same
commit. The write-capable draft job only downloads the verified staging artifact,
checks its manifest, and uploads assets; it never checks out or runs candidate
code. See [RELEASE.md](../RELEASE.md) for retry, validation, and publication steps.

The target matrix also packages one self-contained npm/Yarn/pnpm tarball per
native archive and checks its exact npm installation. Native GNU Linux and
macOS jobs additionally exercise Yarn Plug'n'Play and pnpm in standalone mode.
Release preparation generates a Homebrew formula from the promoted archive
digests, derives seven native npm registry packages and adds architecture-named
Linux package aliases before aggregate hashing. The separate
`publish-packages.yml` workflow revalidates the public release and prepares
independent Homebrew, PyPI, npm and GitHub Packages publishing jobs. Each channel
is opt-in and receives only its own credentials. The user installation path
uses native package-manager commands.
