#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  printf 'Usage: %s <debian|fedora|archlinux> <expected-version>\n' "$0" >&2
  exit 64
fi

manager=$1
expected_version=$2
repo_root=/workspace
temporary_root=/smoke
repo_directory="$temporary_root/$manager-repository"
identity='Bionic Reader CI install smoke <ci@bionic-reader.invalid>'
export GNUPGHOME="$temporary_root/$manager-gnupg"
install -d -m 700 "$GNUPGHOME"

create_smoke_key() {
  command -v gpg >/dev/null 2>&1 || {
    printf 'The %s smoke image is missing gpg\n' "$manager" >&2
    exit 69
  }
  gpg --batch --homedir "$GNUPGHOME" --pinentry-mode loopback --passphrase '' \
    --quick-generate-key "$identity" rsa2048 sign 1d >/dev/null
  key_fingerprint=$(gpg --batch --homedir "$GNUPGHOME" --with-colons \
    --list-secret-keys "$identity" | awk -F: '$1 == "fpr" { print $10; exit }')
  [[ -n $key_fingerprint ]] || {
    printf 'The isolated smoke signing key was not created\n' >&2
    exit 1
  }
}

verify_installed_package() {
  local actual_version
  actual_version=$(bionic-reader --version)
  [[ $actual_version == "bionic-reader $expected_version" ]] || {
    printf 'Installed package version did not match: %s\n' "$actual_version" >&2
    exit 1
  }
  bionic-reader init bash > "$temporary_root/init.bash"
  bionic-reader init zsh > "$temporary_root/init.zsh"
  bionic-reader init fish > "$temporary_root/init.fish"
  bash -n "$temporary_root/init.bash"
  zsh -n "$temporary_root/init.zsh"
  fish --no-execute "$temporary_root/init.fish"

  for shell in bash zsh fish; do
    [[ -s /usr/share/bionic-reader/integrations/$shell.sh || \
      -s /usr/share/bionic-reader/integrations/$shell.fish ]] || {
      printf 'Installed package omitted the %s integration asset\n' "$shell" >&2
      exit 1
    }
  done
  [[ -s /usr/share/bionic-reader/completions/bionic-reader.bash ]] || {
    printf 'Installed package omitted bash completion\n' >&2
    exit 1
  }
  [[ -s /usr/share/bionic-reader/completions/_bionic-reader ]] || {
    printf 'Installed package omitted zsh completion\n' >&2
    exit 1
  }
  [[ -s /usr/share/bionic-reader/completions/bionic-reader.fish ]] || {
    printf 'Installed package omitted fish completion\n' >&2
    exit 1
  }
}

case "$manager" in
  debian)
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install --yes --no-install-recommends \
      apt-utils dpkg-dev gnupg bash fish python3 zsh
    create_smoke_key
    "$repo_root/scripts/create-apt-repo.sh" \
      /packages "$repo_directory" "file://$repo_directory" "$key_fingerprint" stable
    install -D -m 0644 "$repo_directory/keys/bionic-reader-archive-keyring.gpg" \
      /usr/share/keyrings/bionic-reader-archive-keyring.gpg
    install -m 0644 "$repo_directory/bionic-reader.sources" \
      /etc/apt/sources.list.d/bionic-reader.sources
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install --yes --no-install-recommends bionic-reader
    ;;
  fedora)
    dnf install --assumeyes --setopt=install_weak_deps=False \
      createrepo_c fish gnupg2 python3 rpm-sign zsh
    create_smoke_key
    "$repo_root/scripts/create-dnf-repo.sh" \
      /packages "$repo_directory" "file://$repo_directory" "$key_fingerprint"
    rpm --import "$repo_directory/keys/bionic-reader-archive-keyring.gpg"
    install -m 0644 "$repo_directory/bionic-reader.repo" \
      /etc/yum.repos.d/bionic-reader.repo
    dnf --disablerepo='*' --enablerepo=bionic-reader install --assumeyes bionic-reader
    ;;
  archlinux)
    pacman_conf="$temporary_root/pacman.conf"
    pacman_gpgdir="$temporary_root/pacman-gnupg"
    awk -v gpg_directory="$pacman_gpgdir" '
      /^[[:space:]]*#?[[:space:]]*GPGDir[[:space:]]*=/ {
        print "GPGDir = " gpg_directory
        configured = 1
        next
      }
      /^\[options\]$/ && !configured {
        print
        print "GPGDir = " gpg_directory
        configured = 1
        next
      }
      { print }
    ' /etc/pacman.conf > "$pacman_conf"
    install -d -m 700 "$pacman_gpgdir"
    pacman-key --gpgdir "$pacman_gpgdir" --init
    pacman-key --gpgdir "$pacman_gpgdir" --populate archlinux
    pacman --config "$pacman_conf" --sync --refresh --sysupgrade --noconfirm archlinux-keyring
    pacman-key --gpgdir "$pacman_gpgdir" --populate archlinux
    pacman --config "$pacman_conf" --sync --needed --noconfirm \
      fish gnupg pacman-contrib python zsh
    create_smoke_key
    "$repo_root/scripts/create-pacman-repo.sh" \
      /packages "$repo_directory" "file://$repo_directory" "$key_fingerprint"
    pacman-key --gpgdir "$pacman_gpgdir" --add \
      "$repo_directory/keys/bionic-reader-archive-keyring.gpg"
    pacman-key --gpgdir "$pacman_gpgdir" --lsign-key "$key_fingerprint"
    cat "$repo_directory/bionic-reader.conf" >> "$pacman_conf"
    pacman --config "$pacman_conf" --sync --refresh --sysupgrade --noconfirm bionic-reader
    ;;
  *)
    printf 'Unknown package-manager smoke image: %s\n' "$manager" >&2
    exit 64
    ;;
esac

verify_installed_package
python3 "$repo_root/scripts/check-installed-cli.py" "$(command -v bionic-reader)"
printf '%s package installation smoke passed for bionic-reader %s\n' "$manager" "$expected_version"
