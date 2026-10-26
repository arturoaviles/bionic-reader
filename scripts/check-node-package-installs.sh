#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ $# -ne 1 ]]; then
  printf 'Usage: %s <built-host-native-binary>\n' "$0" >&2
  exit 64
fi

binary=$1
[[ -x $binary ]] || {
  printf 'Build a host-native executable before checking package installs: %s\n' "$binary" >&2
  exit 66
}
if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
  printf 'Node.js and npm are required for package-manager install checks\n' >&2
  exit 69
fi

corepack_bin=${COREPACK_BIN:-corepack}
command -v "$corepack_bin" >/dev/null 2>&1 || {
  printf 'Pinned Corepack is required; install corepack 0.36.0 into a temporary tool directory first\n' >&2
  exit 69
}
corepack_version=$("$corepack_bin" --version)
[[ $corepack_version == 0.36.0 ]] || {
  printf 'Package-manager install checks require Corepack 0.36.0; found %s\n' "$corepack_version" >&2
  exit 69
}
command -v rustc >/dev/null 2>&1 || {
  printf 'rustc is required to identify the built host archive target\n' >&2
  exit 69
}

version=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
matches = [package for package in metadata["packages"] if package["name"] == "bionic-reader"]
if len(matches) != 1:
    raise SystemExit("expected exactly one bionic-reader Cargo package")
print(matches[0]["version"])
')
rust_host=$(rustc -vV | sed -n 's/^host: //p')
node_platform=$(node -p 'process.platform')
node_arch=$(node -p 'process.arch')
node_libc=$(node -p "process.report.getReport().header.glibcVersionRuntime ? 'gnu' : 'musl'")
case "$node_platform/$node_arch/$node_libc" in
  darwin/x64/*) native_suffix=darwin-x64; expected_rust_host=x86_64-apple-darwin ;;
  darwin/arm64/*) native_suffix=darwin-arm64; expected_rust_host=aarch64-apple-darwin ;;
  linux/x64/gnu) native_suffix=linux-x64-gnu; expected_rust_host=x86_64-unknown-linux-gnu ;;
  linux/x64/musl) native_suffix=linux-x64-musl; expected_rust_host=x86_64-unknown-linux-musl ;;
  linux/arm64/gnu) native_suffix=linux-arm64-gnu; expected_rust_host=aarch64-unknown-linux-gnu ;;
  linux/arm64/musl) native_suffix=linux-arm64-musl; expected_rust_host=aarch64-unknown-linux-musl ;;
  *) printf 'Unsupported Node host: %s/%s (%s)\n' "$node_platform" "$node_arch" "$node_libc" >&2; exit 69 ;;
esac
[[ $rust_host == "$expected_rust_host" ]] || {
  printf 'The native binary must match the Node host; rustc host is %s, Node host requires %s\n' \
    "$rust_host" "$expected_rust_host" >&2
  exit 69
}
[[ $("$binary" --version) == "bionic-reader $version" ]] || {
  printf 'The built executable version does not match Cargo metadata version %s\n' "$version" >&2
  exit 1
}

temporary=$(mktemp -d)
registry_pid=
cleanup_packages() {
  if [[ -n $registry_pid ]]; then
    kill "$registry_pid" 2>/dev/null || true
    wait "$registry_pid" 2>/dev/null || true
  fi
  rm -rf "$temporary"
}
trap cleanup_packages EXIT
# Guard empty-array expansion below for macOS Bash 3.2 with nounset.
offline_args=(--offline)
export npm_config_cache="$temporary/npm-cache"
export COREPACK_HOME="$temporary/corepack-cache"
export XDG_CACHE_HOME="$temporary/tool-cache"
export XDG_DATA_HOME="$temporary/tool-data"
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
mkdir -p "$temporary/shims" "$temporary/artifacts" "$temporary/packages"
"$corepack_bin" enable --install-directory "$temporary/shims"
export PATH="$temporary/shims:$PATH"

source_epoch=${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct HEAD)}
[[ $source_epoch =~ ^[0-9]+$ ]] || {
  printf 'SOURCE_DATE_EPOCH must be a non-negative integer\n' >&2
  exit 64
}
archive="$temporary/artifacts/bionic-reader-v$version-$rust_host.tar.gz"
python3 scripts/create_archive.py \
  --binary "$binary" \
  --version "$version" \
  --target "$rust_host" \
  --epoch "$source_epoch" \
  --output "$archive" \
  --root "$repo_root"

if [[ ${NODE_PACKAGE_MODE:-local} == standalone ]]; then
  python3 scripts/package-release-node.py "$archive" --output "$temporary/packages"
  base_tarball="$temporary/packages/bionic-reader-v$version-$rust_host.npm.tgz"
elif [[ ${NODE_PACKAGE_MODE:-local} == registry ]]; then
  # Host binaries in foreign-target fixtures are never executed: metadata must
  # select only the actual host dependency. Real target builds are tested separately.
  python3 - "$archive" "$temporary" "$version" <<'PYREGISTRY'
import json, runpy, sys
from pathlib import Path
root = Path.cwd()
archive, temporary, version = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
standalone = runpy.run_path(str(root / "scripts/package-release-node.py"))
registry = runpy.run_path(str(root / "scripts/package-registries.py"))
host = registry["read_package"](standalone["package"](archive, temporary / "standalone"))
for target, (system, cpu, libc) in registry["TARGETS"].items():
    files = dict(host)
    manifest = json.loads(files["package.json"][0])
    manifest["os"], manifest["cpu"] = [system], [cpu]
    if libc:
        manifest["libc"] = [libc]
    else:
        manifest.pop("libc", None)
    files["package.json"] = (json.dumps(manifest).encode(), 0o644)
    registry["write_package"](temporary / "standalone" / f"bionic-reader-v{version}-{target}.npm.tgz", files)
output = temporary / "registry"
registry["registry_packages"](temporary / "standalone", output, version)
base_path = output / f"bionic-reader-npm-base-v{version}.tgz"
base = registry["read_package"](base_path)
manifest = json.loads(base["package.json"][0])
PYREGISTRY
  base_tarball="$temporary/registry/bionic-reader-npm-base-v$version.tgz"
  python3 scripts/tests/registry_server.py "$temporary/registry" "$temporary/registry-port" &
  registry_pid=$!
  for _ in {1..50}; do
    [[ ! -s $temporary/registry-port ]] || break
    kill -0 "$registry_pid" 2>/dev/null || exit 1
    sleep 0.1
  done
  [[ -s $temporary/registry-port ]]
  registry_port=$(cat "$temporary/registry-port")
  # Bootstrap the pinned package-manager binary before restricting package
  # resolution to the loopback fixture. pnpm 12 fetches a native tool binary.
  mkdir -p "$temporary/pnpm-bootstrap"
  printf '%s\n' '{"packageManager":"pnpm@12.14.0"}' > "$temporary/pnpm-bootstrap/package.json"
  (cd "$temporary/pnpm-bootstrap" && npm_config_registry=https://registry.npmjs.org pnpm --version)
  export npm_config_registry="http://127.0.0.1:$registry_port"
  offline_args=()
else
# Exercise the same public local-pack path documented for repository builds.
scripts/package-npm.sh --local "$temporary/artifacts" "$temporary/packages"
base_tarball="$temporary/packages/bionic-reader-base-v$version.tgz"
native_tarball="$temporary/packages/bionic-reader-$native_suffix-v$version.tgz"
[[ -f $base_tarball && -f $native_tarball ]] || {
  printf 'The local npm pack mode did not create the expected host tarballs\n' >&2
  exit 1
}

python3 - "$base_tarball" "$native_tarball" "$repo_root" \
  "@arturoaviles/bionic-reader-$native_suffix" "$version" <<'PY'
from pathlib import Path
import json
import sys
import tarfile
from urllib.parse import unquote, urlparse

base_path, native_path, root_path, native_name, version = sys.argv[1:]
base_path = Path(base_path).resolve()
native_path = Path(native_path).resolve()
root = Path(root_path)

with tarfile.open(base_path, "r:gz") as archive:
    members = {member.name: member for member in archive.getmembers()}
    manifest = json.load(archive.extractfile("package/package.json"))
    expected = {
        "package/bin/bionic-reader.js",
        "package/share/bionic-reader/config.example.toml",
        "package/LICENSE",
    }
    for directory in ("completions", "integrations"):
        expected.update(
            f"package/share/bionic-reader/{directory}/{path.relative_to(root / directory).as_posix()}"
            for path in (root / directory).rglob("*")
            if path.is_file()
        )
    missing = expected - members.keys()
    if missing:
        raise SystemExit("local npm base tarball is missing package assets: " + ", ".join(sorted(missing)))
    if manifest.get("name") != "@arturoaviles/bionic-reader" or manifest.get("version") != version:
        raise SystemExit("local npm base tarball has the wrong package identity")
    optional = manifest.get("optionalDependencies", {})
    if set(optional) != {native_name}:
        raise SystemExit("local npm base tarball must depend only on the host native package")
    parsed = urlparse(optional[native_name])
    if parsed.scheme != "file" or Path(unquote(parsed.path)).resolve() != native_path:
        raise SystemExit("local npm base tarball does not point to its generated native tarball")

with tarfile.open(native_path, "r:gz") as archive:
    members = {member.name: member for member in archive.getmembers()}
    manifest = json.load(archive.extractfile("package/package.json"))
    executable = members.get("package/bin/bionic-reader")
    if executable is None or not executable.isfile() or executable.mode & 0o111 == 0:
        raise SystemExit("local npm native tarball is missing an executable binary")
    if manifest.get("name") != native_name or manifest.get("version") != version:
        raise SystemExit("local npm native tarball has the wrong package identity")
    expected_os = "darwin" if "darwin" in native_name else "linux"
    if manifest.get("os") != [expected_os]:
        raise SystemExit("local npm native tarball has the wrong operating-system restriction")
print("Local npm tarballs contain the wrapper, host executable, and all completion/integration assets")
PY

fi

make_fixture() {
  local directory=$1
  local manager=$2
  mkdir -p "$directory"
  cp "$base_tarball" "$directory/base.tgz"
  python3 - "$directory/package.json" "$manager" <<'PY'
from pathlib import Path
import json
import sys

path, manager = sys.argv[1:]
manifest = {
    "name": f"bionic-reader-{manager.split('@', 1)[0]}-install-smoke",
    "private": True,
    "version": "1.0.0",
    "packageManager": manager,
}
Path(path).write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
}

verify_launcher() (
  local directory=$1
  local manager=$2
  local expected_version=$3
  local -a launcher
  case "$manager" in
    yarn) launcher=(yarn run bionic-reader) ;;
    pnpm) launcher=(pnpm exec bionic-reader) ;;
    npm-global)
      launcher=(bionic-reader)
      export PATH="$npm_global_prefix/bin:$PATH"
      ;;
    *) launcher=(./node_modules/.bin/bionic-reader) ;;
  esac
  cd "$directory"

  [[ $("${launcher[@]}" --version) == "bionic-reader $expected_version" ]] || {
    printf '%s install returned the wrong bionic-reader version\n' "$manager" >&2
    exit 1
  }

  "${launcher[@]}" -t 'Bionic Reader' --style always > "$temporary/text-ansi"
  python3 - "$temporary/text-ansi" <<'PY'
from pathlib import Path
import re
import sys

output = Path(sys.argv[1]).read_bytes()
plain = re.sub(rb"\x1b\[[0-9;]*m", b"", output)
if b"\x1b[" not in output or plain != b"Bionic Reader":
    raise SystemExit("installed bionic-reader did not preserve text while emitting ANSI emphasis")
PY

  printf 'Bionic Reader\n' | "${launcher[@]}" --style always > "$temporary/stdin-ansi"
  python3 - "$temporary/stdin-ansi" <<'PY'
from pathlib import Path
import re
import sys

output = Path(sys.argv[1]).read_bytes()
plain = re.sub(rb"\x1b\[[0-9;]*m", b"", output)
if b"\x1b[" not in output or plain != b"Bionic Reader\n":
    raise SystemExit("installed bionic-reader did not emphasize stdin while preserving its text")
PY

  printf 'plain stdin pass-through\n' | "${launcher[@]}" --style never > "$temporary/stdin-plain"
  printf 'plain stdin pass-through\n' > "$temporary/stdin-expected"
  cmp "$temporary/stdin-expected" "$temporary/stdin-plain"
)

write_cli_wrapper() {
  local output=$1
  local node_directory=$2
  local manager_directory=$3
  local manager=$4
  local project_directory=$5
  local installed_command=$6
  mkdir -p "$(dirname "$output")"
  python3 - "$output" "$node_directory" "$manager_directory" "$manager" \
    "$project_directory" "$installed_command" <<'PY'
from pathlib import Path
import shlex
import sys

output, node_directory, manager_directory, manager, project_directory, installed_command = sys.argv[1:]
path = Path(output)
lines = [
    "#!/bin/sh",
    "set -eu",
    f"PATH={shlex.quote(node_directory)}:{shlex.quote(manager_directory)}:\"$PATH\"",
    "export PATH",
]
if manager == "npm":
    lines.append(f"exec {shlex.quote(installed_command)} \"$@\"")
else:
    lines.append(f"cd {shlex.quote(project_directory)}")
    if manager == "yarn":
        lines.append(f"exec {shlex.quote(installed_command)} run bionic-reader \"$@\"")
    elif manager == "pnpm":
        lines.append(f"exec {shlex.quote(installed_command)} exec bionic-reader \"$@\"")
    else:
        raise SystemExit(f"unsupported CLI wrapper manager: {manager}")
path.write_text("\n".join(lines) + "\n", encoding="utf-8")
path.chmod(0o755)
PY
}

check_installed_cli() {
  local label=$1
  local wrapper=$2
  printf 'Running full installed CLI and Bash PTY smoke through %s...\n' "$label"
  python3 scripts/check-installed-cli.py "$wrapper"
}

node_directory=$(dirname "$(command -v node)")
npm_global_prefix="$temporary/npm-user-prefix"
npm_global_work="$temporary/npm-global-work"
npm_global_root=$(npm root --global --prefix "$npm_global_prefix")
mkdir -p "$npm_global_prefix" "$npm_global_work"
[[ -w $npm_global_prefix ]] || {
  printf 'Temporary npm global prefix must be user-writable: %s\n' "$npm_global_prefix" >&2
  exit 1
}
npm install --global --prefix "$npm_global_prefix" ${offline_args[@]+"${offline_args[@]}"} --ignore-scripts \
  --no-audit --no-fund "$base_tarball"
[[ -x $npm_global_prefix/bin/bionic-reader ]] || {
  printf 'npm did not install the global bionic-reader command into its user prefix\n' >&2
  exit 1
}
verify_launcher "$npm_global_work" npm-global "$version"
write_cli_wrapper "$temporary/npm-cli-bin/bionic-reader" "$node_directory" \
  "$temporary/shims" npm "$npm_global_work" "$npm_global_prefix/bin/bionic-reader"
check_installed_cli 'the user-writable global npm prefix' "$temporary/npm-cli-bin/bionic-reader"
npm uninstall --global --prefix "$npm_global_prefix" ${offline_args[@]+"${offline_args[@]}"} --ignore-scripts \
  --no-audit --no-fund @arturoaviles/bionic-reader
[[ ! -e $npm_global_prefix/bin/bionic-reader ]] || {
  printf 'npm left the global bionic-reader command installed after removal\n' >&2
  exit 1
}
[[ ! -e $npm_global_root/@arturoaviles/bionic-reader && \
   ! -e $npm_global_root/@arturoaviles/bionic-reader-$native_suffix ]] || {
  printf 'npm left bionic-reader packages installed after removal\n' >&2
  exit 1
}

verify_removed() {
  local directory=$1
  local manager=$2
  python3 - "$directory/package.json" <<'PY'
from pathlib import Path
import json
import sys

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
dependencies = manifest.get("dependencies", {})
if "@arturoaviles/bionic-reader" in dependencies:
    raise SystemExit("package manager left bionic-reader in project dependencies after removal")
PY
  if [[ $manager == yarn ]]; then
    if (cd "$directory" && yarn run bionic-reader --version > "$temporary/removed-output" 2>&1); then
      printf 'Yarn still exposes bionic-reader after removal\n' >&2
      exit 1
    fi
  elif [[ -e $directory/node_modules/.bin/bionic-reader ]]; then
    printf '%s left the bionic-reader command installed after removal\n' "$manager" >&2
    exit 1
  fi
}

npm_fixture="$temporary/npm"
make_fixture "$npm_fixture" "npm@$(npm --version)"
(cd "$npm_fixture" && npm install ${offline_args[@]+"${offline_args[@]}"} --ignore-scripts --no-audit --no-fund --save-exact ./base.tgz)
verify_launcher "$npm_fixture" npm "$version"
(cd "$npm_fixture" && npm uninstall ${offline_args[@]+"${offline_args[@]}"} --ignore-scripts --no-audit --no-fund @arturoaviles/bionic-reader)
verify_removed "$npm_fixture" npm

yarn_fixture="$temporary/yarn"
make_fixture "$yarn_fixture" 'yarn@4.18.1'
cat > "$yarn_fixture/.yarnrc.yml" <<'YAML'
nodeLinker: pnp
enableGlobalCache: false
enableNetwork: false
enableScripts: false
enableTelemetry: false
YAML
if [[ ${NODE_PACKAGE_MODE:-local} == registry ]]; then
  cat >> "$yarn_fixture/.yarnrc.yml" <<YAML
npmRegistryServer: "http://127.0.0.1:$registry_port"
globalFolder: "$temporary/yarn-global"
unsafeHttpWhitelist: [127.0.0.1]
YAML
  # Match real registry resolution over loopback, without public package access.
  sed 's/enableNetwork: false/enableNetwork: true/' "$yarn_fixture/.yarnrc.yml" > "$yarn_fixture/.yarnrc.new"
  mv "$yarn_fixture/.yarnrc.new" "$yarn_fixture/.yarnrc.yml"
fi
yarn_version=$(cd "$yarn_fixture" && yarn --version)
[[ $yarn_version == 4.18.1 ]] || {
  printf 'Expected Yarn 4.18.1 from Corepack; found %s\n' "$yarn_version" >&2
  exit 1
}
(cd "$yarn_fixture" && YARN_ENABLE_IMMUTABLE_INSTALLS=false \
  yarn add "@arturoaviles/bionic-reader@file:./base.tgz")
[[ -f $yarn_fixture/.pnp.cjs ]] || {
  printf 'Yarn did not create the expected Plug\x27n\x27Play loader\n' >&2
  exit 1
}
verify_launcher "$yarn_fixture" yarn "$version"
write_cli_wrapper "$temporary/yarn-cli-bin/bionic-reader" "$node_directory" \
  "$temporary/shims" yarn "$yarn_fixture" "$temporary/shims/yarn"
check_installed_cli 'the Yarn project launcher' "$temporary/yarn-cli-bin/bionic-reader"
(cd "$yarn_fixture" && yarn remove @arturoaviles/bionic-reader)
verify_removed "$yarn_fixture" yarn

pnpm_fixture="$temporary/pnpm"
make_fixture "$pnpm_fixture" 'pnpm@12.14.0'
if [[ ${NODE_PACKAGE_MODE:-local} == registry ]]; then
  printf 'registry=%s\n@arturoaviles:registry=%s\n' \
    "$npm_config_registry" "$npm_config_registry" > "$pnpm_fixture/.npmrc"
fi
pnpm_version=$(cd "$pnpm_fixture" && pnpm --version)
[[ $pnpm_version == 12.14.0 ]] || {
  printf 'Expected pnpm 12.14.0 from Corepack; found %s\n' "$pnpm_version" >&2
  exit 1
}
(cd "$pnpm_fixture" && pnpm add --reporter "${NODE_PACKAGE_REPORTER:-append-only}" ${offline_args[@]+"${offline_args[@]}"} --ignore-scripts --store-dir "$temporary/pnpm-store" ./base.tgz)
verify_launcher "$pnpm_fixture" pnpm "$version"
write_cli_wrapper "$temporary/pnpm-cli-bin/bionic-reader" "$node_directory" \
  "$temporary/shims" pnpm "$pnpm_fixture" "$temporary/shims/pnpm"
check_installed_cli 'the pnpm project launcher' "$temporary/pnpm-cli-bin/bionic-reader"
(cd "$pnpm_fixture" && pnpm remove ${offline_args[@]+"${offline_args[@]}"} --store-dir "$temporary/pnpm-store" @arturoaviles/bionic-reader)
verify_removed "$pnpm_fixture" pnpm

printf 'Local npm, Yarn Plug\x27n\x27Play, and pnpm install/use/remove checks passed for bionic-reader %s\n' \
  "$version"
