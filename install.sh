#!/bin/sh
# Install wcl from the latest GitHub release.
#   curl -fsSL https://raw.githubusercontent.com/decheverri123/web-crawler-llm/main/install.sh | sh
set -eu

REPO="decheverri123/web-crawler-llm"
BIN="wcl"

os=$(uname -s)
arch=$(uname -m)

case "$os" in
  Linux) os_id="unknown-linux-gnu" ;;
  Darwin) os_id="apple-darwin" ;;
  *)
    echo "error: unsupported OS: $os (build from source with 'cargo install --path .')" >&2
    exit 1
    ;;
esac

case "$arch" in
  x86_64 | amd64) arch_id="x86_64" ;;
  arm64 | aarch64) arch_id="aarch64" ;;
  *)
    echo "error: unsupported architecture: $arch (build from source with 'cargo install --path .')" >&2
    exit 1
    ;;
esac

target="${arch_id}-${os_id}"
archive="${BIN}-${target}.tar.gz"
base_url="https://github.com/${REPO}/releases/latest/download"

install_dir="${WCL_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$install_dir"

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT INT TERM

echo "Downloading ${base_url}/${archive}" >&2
curl -fsSL "${base_url}/${archive}" -o "$tmpdir/$archive"
curl -fsSL "${base_url}/${archive}.sha256" -o "$tmpdir/${archive}.sha256"

if command -v sha256sum >/dev/null 2>&1; then
  (cd "$tmpdir" && sha256sum -c "${archive}.sha256")
else
  (cd "$tmpdir" && shasum -a 256 -c "${archive}.sha256")
fi

tar -xzf "$tmpdir/$archive" -C "$tmpdir"
mv "$tmpdir/$BIN" "$install_dir/$BIN"
chmod +x "$install_dir/$BIN"

echo "Installed ${BIN} to ${install_dir}/${BIN}" >&2

case ":$PATH:" in
  *":$install_dir:"*) ;;
  *)
    echo "" >&2
    echo "warning: ${install_dir} is not on your PATH. Add this to your shell profile:" >&2
    echo "  export PATH=\"${install_dir}:\$PATH\"" >&2
    ;;
esac
