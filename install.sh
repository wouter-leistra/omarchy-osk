#!/bin/bash
# omarchy-osk installer: a Gboard-style on-screen keyboard for Omarchy.
#
#   curl -fsSL https://github.com/wouter-leistra/omarchy-osk/releases/latest/download/install.sh | bash
#
# Downloads the release made for your Omarchy version (a prebuilt keyboard,
# its scripts and data), checks it, installs it for your user in ~/.local (no
# root needed) and runs omarchy-osk-setup, which asks for languages and the
# rest.
#
# Each release is tested with one Omarchy version, and its patches for
# Omarchy's menu, theme switcher and lock screen fit that version. So the
# newest release tested with your Omarchy or an older one is installed, not
# simply the latest: on Omarchy 4.0.4 that is the last release made for 4.0.4.
#
# Options:
#   --version vX.Y.Z   install that release instead
#   --which            only say which release would be installed, and why
#   --from DIR|FILE    install from an unpacked release or a release tarball
#   --yes              no questions: defaults (or your current settings)
#   --files-only       only copy the files, don't run the setup
#   --force            install even where it isn't meant to run
set -euo pipefail

REPO="wouter-leistra/omarchy-osk"
ASSET="omarchy-osk-x86_64.tar.gz"
PREFIX="${OSK_PREFIX:-$HOME/.local}"
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}"
STATE="${XDG_STATE_HOME:-$HOME/.local/state}/omarchy-osk"

version="" from="" yes=0 files_only=0 force=0 which=0
while (($#)); do
  case $1 in
    --version) version=$2; shift ;;
    --from) from=$2; shift ;;
    --yes|-y) yes=1 ;;
    --files-only) files_only=1 ;;
    --force) force=1 ;;
    --which) which=1 ;;
    -h|--help)
      echo "usage: install.sh [--version vX.Y.Z] [--which] [--from DIR|FILE] [--yes] [--files-only] [--force]"
      exit 0 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
  shift
done

say() { echo "omarchy-osk: $*"; }
die() { echo "omarchy-osk: $*" >&2; exit 1; }

# ---------------------------------------------------------------- checks
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || ((force)) ||
  die "this release is for x86_64 Linux (Omarchy) only"
if ! command -v omarchy >/dev/null; then
  ((force)) || die "Omarchy not found: omarchy-osk is made for Omarchy (use --force to install anyway)"
fi
for tool in jq hyprctl python3; do
  command -v "$tool" >/dev/null || ((force)) || die "$tool is missing"
done

# ---------------------------------------------------------------- which release
# The newest release tested with this Omarchy or an older one. Release notes
# say "... with Omarchy X.Y.Z and Hyprland ..." (tools/release.sh writes them).
pick_release() {
  local omarchy releases tested tag older="" newest=""
  omarchy=$(omarchy version 2>/dev/null | head -1 | cut -d- -f1)
  if [[ ! $omarchy =~ ^[0-9]+(\.[0-9]+)*$ ]]; then
    reason="can't tell your Omarchy version"
    return
  fi
  releases=$(curl -fsSL "${OSK_RELEASES_API:-https://api.github.com/repos/$REPO/releases?per_page=100}" 2>/dev/null |
    jq -r '.[] | select(.draft | not) | select(.prerelease | not)
           | [.tag_name, (.body // "" | capture("with Omarchy (?<v>[0-9][0-9.]*[0-9])").v // "")] | @tsv' 2>/dev/null) || releases=""
  if [[ -z $releases ]]; then
    reason="can't list the releases on GitHub"
    return
  fi
  # newest first, as GitHub lists them
  while IFS=$'\t' read -r tag tested; do
    [[ -n $tested ]] || continue
    [[ -n $newest ]] || newest="$tag $tested"
    if [[ $(printf '%s\n%s\n' "$tested" "$omarchy" | sort -V | head -1) == "$tested" ]]; then
      version=$tag
      if [[ $tag == "${newest%% *}" ]]; then
        reason="the latest release, tested with Omarchy $tested; you have $omarchy"
      else
        reason="tested with Omarchy $tested; you have $omarchy. Newer releases are made for Omarchy ${newest#* }: update Omarchy (omarchy update) to get them"
      fi
      return
    fi
    # the newest release for each older Omarchy comes first
    [[ -n $older && $tested == "${older#* }" ]] || older="$tag $tested"
  done <<<"$releases"
  if [[ -n $older ]]; then
    # every release is newer than this Omarchy: the one made for the oldest
    version=${older%% *}
    reason="made for Omarchy ${older#* }, newer than your $omarchy: update Omarchy (omarchy update) first, or Omarchy's menu, theme switcher and lock screen may not get the keyboard patches"
  fi
}

reason=""
if [[ -z $version && -z $from && -z ${OSK_RELEASE_URL:-} ]]; then
  pick_release
fi
if ((which)); then
  echo "${version:-latest}${reason:+: $reason}"
  exit 0
fi
[[ -z $reason ]] || say "installing ${version:-the latest release}: $reason"

# ---------------------------------------------------------------- the files
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

if [[ -z $from ]]; then
  if [[ -n ${OSK_RELEASE_URL:-} ]]; then
    url=$OSK_RELEASE_URL  # tests
  elif [[ -n $version ]]; then
    url="https://github.com/$REPO/releases/download/$version"
  else
    url="https://github.com/$REPO/releases/latest/download"
  fi
  say "downloading ${version:-the latest release}"
  curl -fsSL -o "$tmp/$ASSET" "$url/$ASSET" || die "download failed: $url/$ASSET"
  curl -fsSL -o "$tmp/SHA256SUMS" "$url/SHA256SUMS" || die "download failed: $url/SHA256SUMS"
  (cd "$tmp" && grep " $ASSET\$" SHA256SUMS | sha256sum -c --quiet) || die "checksum mismatch: the download is damaged"
  from="$tmp/$ASSET"
fi
if [[ -f $from ]]; then
  mkdir -p "$tmp/src"
  tar -xzf "$from" -C "$tmp/src"
  src=$(find "$tmp/src" -mindepth 1 -maxdepth 1 -type d | head -1)
else
  src=$from
fi
[[ -x $src/bin/omarchy-oskd && -f $src/share/omarchy-osk/languages.txt ]] || die "$from is not an omarchy-osk release"

missing=$(ldd "$src/bin/omarchy-oskd" 2>/dev/null | grep 'not found' || true)
[[ -z $missing ]] || ((force)) || die "libraries missing (install pango, cairo, libxkbcommon): $missing"
python3 -c 'import gi' 2>/dev/null || say "python-gobject is missing: auto-show won't work until it is installed"

# ---------------------------------------------------------------- install
mkdir -p "$STATE"
manifest="$STATE/files"
touch "$manifest"
record() { grep -qxF "$1" "$manifest" || echo "$1" >>"$manifest"; }

put() { # put SRC DEST MODE
  install -Dm "$3" "$1" "$2"
  record "$2"
}

for f in "$src"/bin/*; do
  put "$f" "$PREFIX/bin/$(basename "$f")" 755
done
while IFS= read -r -d '' f; do
  rel=${f#"$src/share/omarchy-osk/"}
  put "$f" "$PREFIX/share/omarchy-osk/$rel" 644
done < <(find "$src/share/omarchy-osk" -type f -print0)
for f in "$src"/lib/omarchy-osk/*; do
  put "$f" "$PREFIX/lib/omarchy-osk/$(basename "$f")" 755
done
for f in "$src"/share/omarchy-osk/systemd/*; do
  put "$f" "$CONFIG/systemd/user/$(basename "$f")" 644
done
command -v systemctl >/dev/null && systemctl --user daemon-reload 2>/dev/null || true

version_installed=$(cat "$src/share/omarchy-osk/VERSION" 2>/dev/null || echo "?")
say "installed omarchy-osk $version_installed in $PREFIX"
case ":$PATH:" in
  *":$PREFIX/bin:"*) ;;
  *) say "note: $PREFIX/bin is not on your PATH" ;;
esac

((files_only)) && exit 0

# ---------------------------------------------------------------- set up
setup=("$PREFIX/bin/omarchy-osk-setup")
((yes)) && setup+=(--yes)
if ((yes)) || ! { : </dev/tty; } 2>/dev/null; then
  "${setup[@]}"
else
  # piped through curl: the questions come from the terminal
  "${setup[@]}" </dev/tty
fi
