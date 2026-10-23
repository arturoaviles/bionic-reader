#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ $# -ne 1 ]]; then
  printf 'Usage: %s <built-native-binary>\n' "$0" >&2
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

node_platform=$(node -p 'process.platform')
node_arch=$(node -p 'process.arch')
node_libc=$(node -p "process.report.getReport().header.glibcVersionRuntime ? 'gnu' : 'musl'")
case "$node_platform/$node_arch" in
  darwin/x64|darwin/arm64) native_name="@arturoaviles/bionic-reader-darwin-$node_arch" ;;
  linux/x64|linux/arm64) native_name="@arturoaviles/bionic-reader-linux-$node_arch-$node_libc" ;;
  *) printf 'Unsupported Node host: %s/%s\n' "$node_platform" "$node_arch" >&2; exit 69 ;;
esac

corepack_bin=${COREPACK_BIN:-corepack}
command -v "$corepack_bin" >/dev/null 2>&1 || {
  printf 'Pinned Corepack is required; install corepack 0.36.0 into a temporary tool directory first\n' >&2
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
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
export npm_config_cache="$temporary/npm-cache"
export COREPACK_HOME="$temporary/corepack-cache"
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
mkdir -p \
  "$temporary/shims" \
  "$temporary/packages/native/bin" \
  "$temporary/packages/base/bin" \
  "$temporary/packages/base/share/bionic-reader" \
  "$temporary/tarballs"
"$corepack_bin" enable --install-directory "$temporary/shims"
export PATH="$temporary/shims:$PATH"

cp "$binary" "$temporary/packages/native/bin/bionic-reader"
chmod 755 "$temporary/packages/native/bin/bionic-reader"
cp LICENSE "$temporary/packages/native/LICENSE"
cp packaging/npm/bin/bionic-reader.js "$temporary/packages/base/bin/bionic-reader.js"
chmod 755 "$temporary/packages/base/bin/bionic-reader.js"
cp LICENSE "$temporary/packages/base/LICENSE"
cp packaging/config.example.toml "$temporary/packages/base/share/bionic-reader/config.example.toml"
cp -R integrations "$temporary/packages/base/share/bionic-reader/integrations"
cp -R completions "$temporary/packages/base/share/bionic-reader/completions"

python3 - "$temporary/packages/native/package.json" "$native_name" "$version" "$node_platform" "$node_arch" "$node_libc" <<'PY'
from pathlib import Path
import json
import sys

path, name, version, platform, arch, libc = sys.argv[1:]
manifest = {
    "name": name,
    "version": version,
    "description": f"Native bionic-reader executable for {platform}/{arch}",
    "license": "MIT",
    "files": ["bin/bionic-reader", "LICENSE"],
    "os": [platform],
    "cpu": [arch],
    "preferUnplugged": True,
}
if platform == "linux":
    manifest["libc"] = ["glibc" if libc == "gnu" else "musl"]
Path(path).write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY

native_tarball=$(cd "$temporary/packages/native" && \
  npm pack --ignore-scripts --silent --pack-destination "$temporary/tarballs")
native_tarball="$temporary/tarballs/$native_tarball"
python3 - "$repo_root/packaging/npm/package.json.in" "$temporary/packages/base/package.json" \
  "$version" "$native_name" "$native_tarball" <<'PY'
from pathlib import Path
import json
import sys

template, output, version, native_name, native_tarball = sys.argv[1:]
manifest = json.loads(Path(template).read_text(encoding="utf-8").replace("@VERSION@", version))
manifest["optionalDependencies"] = {native_name: f"file:{native_tarball}"}
Path(output).write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
base_tarball=$(cd "$temporary/packages/base" && \
  npm pack --ignore-scripts --silent --pack-destination "$temporary/tarballs")
base_tarball="$temporary/tarballs/$base_tarball"

make_fixture() {
  local directory=$1
  local manager=$2
  mkdir -p "$directory"
  python3 - "$directory/package.json" "$manager" "$base_tarball" <<'PY'
from pathlib import Path
import json
import sys

path, manager, base_tarball = sys.argv[1:]
manifest = {
    "name": f"bionic-reader-{manager.split('@', 1)[0]}-install-smoke",
    "private": True,
    "version": "1.0.0",
    "packageManager": manager,
    "dependencies": {"@arturoaviles/bionic-reader": f"file:{base_tarball}"},
}
Path(path).write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
}

verify_launcher() (
  local directory=$1
  local expected_version=$2
  shift 2
  cd "$directory"

  [[ $("$@" --version) == "bionic-reader $expected_version" ]] || {
    printf 'Installed launcher returned the wrong version (%s)\n' "$*" >&2
    exit 1
  }
  [[ $("$@" -t 'Bionic Reader' --style never) == 'Bionic Reader' ]] || {
    printf 'Installed launcher did not transform a text argument (%s)\n' "$*" >&2
    exit 1
  }
  printf 'offline stdin input\n' | "$@" --style never | cmp - <(printf 'offline stdin input\n')
)

npm_fixture="$temporary/npm"
make_fixture "$npm_fixture" "npm@$(npm --version)"
(cd "$npm_fixture" && npm install --offline --ignore-scripts --no-audit --no-fund)
verify_launcher "$npm_fixture" "$version" ./node_modules/.bin/bionic-reader

yarn_fixture="$temporary/yarn"
make_fixture "$yarn_fixture" 'yarn@4.18.1'
cat > "$yarn_fixture/.yarnrc.yml" <<'YAML'
nodeLinker: pnp
enableGlobalCache: false
enableNetwork: false
enableScripts: false
enableTelemetry: false
YAML
yarn_version=$(cd "$yarn_fixture" && yarn --version)
[[ $yarn_version == 4.18.1 ]] || {
  printf 'Expected Yarn 4.18.1 from Corepack; found %s\n' "$yarn_version" >&2
  exit 1
}
(cd "$yarn_fixture" && yarn install --mode=skip-build)
[[ -f $yarn_fixture/.pnp.cjs ]] || {
  printf 'Yarn did not create the expected Plug\x27n\x27Play loader\n' >&2
  exit 1
}
verify_launcher "$yarn_fixture" "$version" yarn run bionic-reader

pnpm_fixture="$temporary/pnpm"
make_fixture "$pnpm_fixture" 'pnpm@12.14.0'
pnpm_version=$(cd "$pnpm_fixture" && pnpm --version)
[[ $pnpm_version == 12.14.0 ]] || {
  printf 'Expected pnpm 12.14.0 from Corepack; found %s\n' "$pnpm_version" >&2
  exit 1
}
(cd "$pnpm_fixture" && pnpm install --offline --ignore-scripts --store-dir "$temporary/pnpm-store")
verify_launcher "$pnpm_fixture" "$version" ./node_modules/.bin/bionic-reader

printf 'Offline npm, Yarn Plug\x27n\x27Play, and pnpm candidate installs passed with bionic-reader %s\n' \
  "$version"
