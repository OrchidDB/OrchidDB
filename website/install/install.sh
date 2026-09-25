#!/usr/bin/env bash
# OrchidDB CLI installer. Downloads only published GitHub release assets.
set -euo pipefail
fail() { printf 'OrchidDB: %s\n' "$*" >&2; exit 1; }
for tool in curl tar mktemp; do command -v "$tool" >/dev/null || fail "Required command: $tool"; done
case "$(uname -s)/$(uname -m)" in
  Darwin/arm64|Darwin/aarch64) target=aarch64-apple-darwin ;;
  Darwin/x86_64) target=x86_64-apple-darwin ;;
  Linux/x86_64|Linux/amd64) target=x86_64-unknown-linux-gnu ;;
  *) fail "Unsupported platform: $(uname -s)/$(uname -m). See https://docs.orchiddb.com/installation.html" ;;
esac
if [[ "$target" == *linux* ]]; then
  glibc_info=$(ldd --version 2>&1) || fail 'Linux releases require glibc 2.35+. For musl/Alpine, build from source.'
  printf '%s\n' "$glibc_info" | grep -qiE 'glibc|GNU libc' || fail 'Linux releases require glibc 2.35+. For musl/Alpine, build from source.'
  glibc_info=${glibc_info%%$'\n'*}
  [[ "$glibc_info" =~ ([0-9]+)\.([0-9]+) ]] || fail 'Cannot determine the glibc version. Build from source instead.'
  (( BASH_REMATCH[1] > 2 || (BASH_REMATCH[1] == 2 && BASH_REMATCH[2] >= 35) )) || fail 'Linux releases require glibc 2.35 or newer. Build from source on this system.'
fi
version=${ORCHIDDB_VERSION:-latest}
fetch() { curl --fail --silent --show-error --location --retry 3 --proto '=https' --tlsv1.2 "$@"; }
if [[ "$version" == latest ]]; then
  releases=$(fetch 'https://api.github.com/repos/OrchidDB/OrchidDB/releases?per_page=1') || fail 'Cannot discover published releases. Try again or set ORCHIDDB_VERSION=vX.Y.Z.'
  version=$(printf '%s\n' "$releases" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)
  [[ -n "$version" ]] || fail 'No published release yet. Build from source: cargo install --locked --git https://github.com/OrchidDB/OrchidDB --bin orchiddb'
fi
[[ "$version" == v* ]] || version="v$version"
[[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || fail 'ORCHIDDB_VERSION must be a version such as v0.1.0.'
asset="orchiddb-$version-$target.tar.gz"
url="https://github.com/OrchidDB/OrchidDB/releases/download/$version"
workspace=$(mktemp -d)
trap 'rm -rf "$workspace"' EXIT
printf 'Downloading OrchidDB %s for %s…\n' "$version" "$target"
fetch "$url/$asset" -o "$workspace/$asset" || fail "Release asset unavailable: $url/$asset"
fetch "$url/SHA256SUMS" -o "$workspace/SHA256SUMS" || fail 'Release checksums unavailable.'
expected=$(awk -v asset="$asset" '$2 == asset { print $1 }' "$workspace/SHA256SUMS")
[[ "$expected" =~ ^[0-9a-f]{64}$ ]] || fail 'Missing or invalid checksum for this archive.'
if command -v sha256sum >/dev/null; then
  actual=$(sha256sum "$workspace/$asset"); actual=${actual%% *}
elif command -v shasum >/dev/null; then
  actual=$(shasum -a 256 "$workspace/$asset"); actual=${actual%% *}
else
  fail 'Install sha256sum or shasum to verify downloads.'
fi
[[ "$actual" == "$expected" ]] || fail 'Checksum mismatch; nothing was installed.'
# Extract only the expected executable, never arbitrary archive paths.
stem="orchiddb-$version-$target"
tar -xzf "$workspace/$asset" -C "$workspace" "$stem/orchiddb"
[[ -f "$workspace/$stem/orchiddb" && ! -L "$workspace/$stem/orchiddb" ]] || fail 'Archive does not contain a regular executable.'
destination=${ORCHIDDB_INSTALL_DIR:-"$HOME/.local/bin"}
mkdir -p "$destination"
# Stage on the destination filesystem, so replacement is atomic.
staged=$(mktemp "$destination/.orchiddb.XXXXXX")
trap 'rm -rf "$workspace"; rm -f "${staged:-}"' EXIT
cp "$workspace/$stem/orchiddb" "$staged"
chmod 755 "$staged"
mv -f "$staged" "$destination/orchiddb"
printf 'Installed %s/orchiddb\n' "$destination"
case ":$PATH:" in *":$destination:"*) ;; *) printf 'Add %s to your PATH.\n' "$destination" ;; esac
printf "Try: %s/orchiddb --query 'RETURN 1 AS value'\n" "$destination"
