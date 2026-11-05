#!/usr/bin/env bash
# Fills the winget manifest templates for a published release.
# Usage: winget-manifest.sh <version>   (e.g. 0.1.0)
# Output: dist/winget/manifests/a/ayokiru/winrice/<version>/ ready for a winget-pkgs PR.
set -euo pipefail
cd "$(dirname "$0")/.."
version="${1:?usage: winget-manifest.sh <version>}"
base="https://github.com/ayokiru/winrice/releases/download/v$version"
sha="$(curl -fsSL "$base/winrice.exe.sha256" | awk '{print toupper($1)}')"
[[ ${#sha} -eq 64 ]] || { echo "winget-manifest: no checksum for v$version" >&2; exit 1; }
date="$(curl -fsSL "https://api.github.com/repos/ayokiru/winrice/releases/tags/v$version" \
  | python3 -c 'import sys,json; print(json.load(sys.stdin)["published_at"][:10])')"
out="dist/winget/manifests/a/ayokiru/winrice/$version"
mkdir -p "$out"
for f in packaging/winget/*.yaml; do
  sed -e "s/{{VERSION}}/$version/g" -e "s/{{SHA256}}/$sha/g" -e "s/{{DATE}}/$date/g" "$f" > "$out/$(basename "$f")"
done
echo "$out"
