#!/bin/bash
# Build unl0kr (postmarketOS buffybox) for the boot prompt, as a normal user.
# The result is unlock/build/unl0kr; install.sh puts it in place.
set -euo pipefail

TAG=3.6.0
COMMIT=d0f52495f7f3afa8839eef4b19f43f4ca1243107

here="$(dirname "$(readlink -f "$0")")"
out="$here/build"
src="$out/buffybox"
mkdir -p "$out"

if [[ ! -d $src ]]; then
  git clone -q --depth 1 --branch "$TAG" --recurse-submodules --shallow-submodules \
    https://gitlab.postmarketos.org/postmarketOS/buffybox.git "$src"
fi
[[ $(git -C "$src" rev-parse HEAD) == "$COMMIT" ]] || { echo "buffybox $TAG is not $COMMIT" >&2; exit 1; }

# meson and ninja from a private venv, no system packages needed
if ! command -v meson >/dev/null || ! command -v ninja >/dev/null; then
  [[ -x $out/venv/bin/meson ]] || { python3 -m venv "$out/venv" && "$out/venv/bin/pip" -q install meson ninja; }
  export PATH="$out/venv/bin:$PATH"
fi

cd "$src"
[[ -d build ]] || meson setup build -Dlvgl_backends=drm,framebuffer -Dman=false --buildtype=release
meson compile -C build unl0kr/unl0kr
cp build/unl0kr/unl0kr "$out/unl0kr"
"$out/unl0kr" -V
