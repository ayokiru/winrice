#!/usr/bin/env bash
# Cross-builds winrice.exe from macOS/Linux without local paths in the binary.
set -euo pipefail
cd "$(dirname "$0")/.."
export XWIN_ACCEPT_LICENSE=1
export RUSTFLAGS="--remap-path-prefix=$HOME/.cargo=cargo --remap-path-prefix=$HOME/.rustup=rustup --remap-path-prefix=$PWD=winrice -C link-arg=/PDBALTPATH:%_PDB% -C link-arg=/Brepro"
cargo xwin build --release --target x86_64-pc-windows-msvc
mkdir -p dist
cp target/x86_64-pc-windows-msvc/release/winrice.exe dist/
if strings -a dist/winrice.exe | grep -Fq "$HOME"; then
  echo "build-win: local paths found in the binary" >&2
  exit 1
fi
(cd dist && shasum -a 256 winrice.exe > winrice.exe.sha256)
echo "dist/winrice.exe ready"
