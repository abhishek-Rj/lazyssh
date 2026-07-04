#!/usr/bin/env sh
set -eu

OWNER="abhishek-Rj"
REPO="russhx"
BIN_NAME="russhx"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${1:-latest}"

fail() {
  printf 'russhx installer: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

detect_os() {
  os="$(uname -s 2>/dev/null || true)"
  case "$os" in
    Linux) printf 'linux' ;;
    Darwin) printf 'macos' ;;
    MINGW*|MSYS*|CYGWIN*) printf 'windows' ;;
    *) fail "unsupported OS: ${os:-unknown}" ;;
  esac
}

detect_arch() {
  arch="$(uname -m 2>/dev/null || true)"
  case "$arch" in
    x86_64|amd64) printf 'x86_64' ;;
    aarch64|arm64) printf 'aarch64' ;;
    *) fail "unsupported CPU architecture: ${arch:-unknown}" ;;
  esac
}

latest_version() {
  url="https://api.github.com/repos/$OWNER/$REPO/releases/latest"
  response="$(curl -fsSL "$url")" || fail "failed to fetch latest release from GitHub"
  tag="$(printf '%s\n' "$response" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
  [ -n "$tag" ] || fail "latest GitHub release is missing or has no tag"
  printf '%s' "$tag"
}

need_cmd curl
need_cmd uname

os="$(detect_os)"
arch="$(detect_arch)"

case "$os-$arch" in
  linux-x86_64|macos-x86_64|macos-aarch64|windows-x86_64) ;;
  linux-aarch64) fail "unsupported platform: linux-aarch64" ;;
  windows-aarch64) fail "unsupported platform: windows-aarch64" ;;
  *) fail "unsupported platform: $os-$arch" ;;
esac

if [ "$VERSION" = "latest" ]; then
  VERSION="$(latest_version)"
fi

case "$os" in
  windows)
    archive="$BIN_NAME-$VERSION-$os-$arch.zip"
    binary="$BIN_NAME.exe"
    ;;
  *)
    archive="$BIN_NAME-$VERSION-$os-$arch.tar.gz"
    binary="$BIN_NAME"
    ;;
esac

download_url="https://github.com/$OWNER/$REPO/releases/download/$VERSION/$archive"
tmp_dir="$(mktemp -d 2>/dev/null || mktemp -d -t russhx)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

printf 'Downloading %s...\n' "$download_url"
curl -fL "$download_url" -o "$tmp_dir/$archive" || fail "failed to download release asset: $archive"

case "$archive" in
  *.tar.gz)
    need_cmd tar
    tar -xzf "$tmp_dir/$archive" -C "$tmp_dir" || fail "failed to extract $archive"
    ;;
  *.zip)
    if command -v unzip >/dev/null 2>&1; then
      unzip -q "$tmp_dir/$archive" -d "$tmp_dir" || fail "failed to extract $archive"
    elif command -v powershell.exe >/dev/null 2>&1; then
      powershell.exe -NoProfile -Command "Expand-Archive -Force '$tmp_dir/$archive' '$tmp_dir'" \
        || fail "failed to extract $archive"
    else
      fail "cannot extract zip archive; install unzip or use PowerShell"
    fi
    ;;
esac

found="$(find "$tmp_dir" -type f -name "$binary" | head -n 1)"
[ -n "$found" ] || fail "release archive did not contain $binary"

mkdir -p "$INSTALL_DIR" || fail "failed to create install directory: $INSTALL_DIR"
cp "$found" "$INSTALL_DIR/$binary" || fail "failed to install $binary into $INSTALL_DIR"
chmod +x "$INSTALL_DIR/$binary" 2>/dev/null || true

printf 'Installed %s to %s\n' "$BIN_NAME" "$INSTALL_DIR/$binary"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    cat <<EOF

Note: $INSTALL_DIR is not in your PATH.
Add it with one of the following lines:

  echo 'export PATH="\$HOME/.local/bin:\$PATH"' >> ~/.shrc
  echo 'export PATH="\$HOME/.local/bin:\$PATH"' >> ~/.bashrc
  echo 'export PATH="\$HOME/.local/bin:\$PATH"' >> ~/.zshrc

Then restart your shell or run:

  export PATH="\$HOME/.local/bin:\$PATH"
EOF
    ;;
esac

if ! command -v ssh >/dev/null 2>&1; then
  cat <<EOF

Warning: OpenSSH was not found in PATH.
russhx uses the system ssh command to connect to servers.
EOF
fi
