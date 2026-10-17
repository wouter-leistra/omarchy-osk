#!/bin/bash
# Lay out a release in DIR (default target/stage/omarchy-osk): the built
# keyboard, the scripts, the data and install.sh, as the release tarball
# holds them. Run `cargo build --release` first.
set -euo pipefail
cd "$(dirname "$0")/.."
out=${1:-target/stage/omarchy-osk}
rm -rf "$out"
share="$out/share/omarchy-osk"
mkdir -p "$out/bin" "$out/lib/omarchy-osk" "$share"

install -m 755 target/release/omarchy-oskd "$out/bin/"
for f in bin/omarchy-osk*; do
  [[ -f $f ]] && install -m 755 "$f" "$out/bin/"
done
install -m 755 install.sh "$out/"
install -m 755 unlock/omarchy-look "$out/lib/omarchy-osk/"

install -Dm 644 -t "$share/dict" dict/*.txt dict/README.md
install -Dm 644 -t "$share/layouts" data/layouts/*.txt
install -Dm 644 -t "$share/locales" data/locales/*.txt
install -m 644 data/languages.txt data/popups.txt "$share/"
install -m 644 data/README.md "$share/README.data.md"
for p in menu image-picker lock; do
  install -Dm 644 -t "$share/patches/$p" omarchy/patches/$p/*.patch
done
install -Dm 644 -t "$share/plugin" omarchy/plugin/BarWidget.qml omarchy/plugin/manifest.json
install -Dm 755 -t "$share/plugin" omarchy/plugin/theme-hook.sh
install -Dm 755 -t "$share/hooks" omarchy/hooks/post-update
install -Dm 644 -t "$share/systemd" omarchy/systemd/*
install -Dm 644 -t "$share/doc" README.md LICENSE COPYING
sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1 >"$share/VERSION"
[[ -f target/TESTED ]] && install -m 644 target/TESTED "$share/TESTED"
echo "$out"
