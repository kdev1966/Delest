#!/bin/bash
# Builds Delest's installable packages for the system it runs on, with the delest
# command inside, next to the window's program, where the window looks for it to
# install the agents' hooks:
#   macOS    Delest.app and its DMG, universal: Intel and Apple Silicon (signed ad hoc:
#            macOS asks to confirm the first opening). Needs both Rust targets:
#            rustup target add x86_64-apple-darwin aarch64-apple-darwin
#   Linux    a .deb and an AppImage
#   Windows  an NSIS installer, for the current user (run from Git Bash)
# Needs the Tauri CLI: cargo install tauri-cli --version "^2" --locked, or set
# TAURI="npx --yes @tauri-apps/cli@2.12.3".
set -euo pipefail
cd "$(dirname "$0")/.."
triple=$(rustc -vV | sed -n 's/^host: //p')
gui=crates/delest-gui
tauri=${TAURI:-cargo tauri}

case "$(uname -s)" in
  Darwin) os=macos; bundles=app,dmg; exe= ;;
  Linux) os=linux; bundles=deb,appimage; exe= ;;
  MINGW* | MSYS* | CYGWIN*) os=windows; bundles=nsis; exe=.exe ;;
  *) echo "package.sh: unknown system $(uname -s)" >&2; exit 1 ;;
esac

mkdir -p "$gui/binaries"
target=()
if [ "$os" = macos ]; then
  # One app for both kinds of Mac: each architecture built, then joined by lipo.
  # Tauri builds each architecture in turn and looks for the command under its name.
  for arch in x86_64 aarch64; do
    cargo build --release --locked -p delest --target "$arch-apple-darwin"
    cp "target/$arch-apple-darwin/release/delest" "$gui/binaries/delest-$arch-apple-darwin"
  done
  lipo -create -output "$gui/binaries/delest-universal-apple-darwin" \
    target/x86_64-apple-darwin/release/delest target/aarch64-apple-darwin/release/delest
  target=(--target universal-apple-darwin)
else
  cargo build --release --locked -p delest
  cp "target/release/delest$exe" "$gui/binaries/delest-$triple$exe"
fi

if [ "$os" = macos ]; then
  # The macOS icon, made from the PNG (1024 px, drawn from icons/icon.svg).
  set_dir=$(mktemp -d)/icon.iconset
  mkdir -p "$set_dir"
  for size in 16 32 128 256 512; do
    sips -z $size $size "$gui/icons/icon.png" --out "$set_dir/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    [ $double -le 1024 ] && sips -z $double $double "$gui/icons/icon.png" --out "$set_dir/icon_${size}x${size}@2x.png" >/dev/null
  done
  iconutil -c icns "$set_dir" -o "$gui/icons/icon.icns"
fi

build() { (cd "$gui" && $tauri build --config "tauri.bundle.$os.json" --bundles "$bundles" "${target[@]}"); }
# The DMG's first attempt can stop at the Finder's layout, leaving its working image
# mounted: that image is detached and the build tried once more.
if ! build; then
  [ "$os" = macos ] || exit 1
  for volume in /Volumes/dmg.*; do [ -d "$volume" ] && hdiutil detach -force "$volume" >/dev/null; done
  rm -f target/*/release/bundle/macos/rw.*.dmg target/release/bundle/macos/rw.*.dmg
  build
fi
find target -path '*/release/bundle/*' -type f \( -name '*.dmg' -o -name '*.deb' -o -name '*.AppImage' -o -name '*-setup.exe' \)
