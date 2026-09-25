#!/usr/bin/env bash
# Install the same mdBook release locally and in the publishing workflow.
set -euo pipefail
version=0.5.4
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) platform=aarch64-apple-darwin ;;
  Darwin-x86_64) platform=x86_64-apple-darwin ;;
  Linux-x86_64) platform=x86_64-unknown-linux-musl ;;
  Linux-aarch64) platform=aarch64-unknown-linux-musl ;;
  *) echo 'Unsupported platform; install mdbook 0.5.4 with cargo install mdbook --version 0.5.4 --locked.' >&2; exit 1 ;;
esac
install_dir="${MDBOOK_INSTALL_DIR:-$HOME/.local/bin}"
archive_dir="$(mktemp -d)"
trap 'rm -rf "$archive_dir"' EXIT
curl --fail --location --silent --show-error \
  "https://github.com/rust-lang/mdBook/releases/download/v$version/mdbook-v$version-$platform.tar.gz" \
  -o "$archive_dir/mdbook.tar.gz"
mkdir -p "$install_dir"
tar -xzf "$archive_dir/mdbook.tar.gz" -C "$archive_dir"
install -m 755 "$archive_dir/mdbook" "$install_dir/mdbook"
"$install_dir/mdbook" --version
printf 'Installed in %s. Add this directory to PATH.\n' "$install_dir"
