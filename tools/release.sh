#!/bin/bash
# Make a release on the device it is made for, so "tested with" is true:
#
#   tools/release.sh v0.1.0 [--skip-ui] [--publish]
#
# 1. the tree must be clean, Cargo.toml's version must match and
#    CHANGELOG.md must have a dated section for it
# 2. installs this build for your user and runs every test: unit and golden
#    tests (make check), the installer test, the end-to-end tests/ui_test.py
#    (needs the machine to itself; --skip-ui to leave them out, which TESTED
#    then says)
# 3. writes target/TESTED: Omarchy and Hyprland versions, device, commit,
#    date and what passed
# 4. makes the tarball and SHA256SUMS (make dist), with TESTED inside
# 5. --publish: tags the commit, pushes the tag and creates the GitHub
#    release with install.sh, the tarball and SHA256SUMS; the notes are the
#    CHANGELOG.md section and what it was tested on
set -euo pipefail
cd "$(dirname "$0")/.."

tag=${1:?usage: tools/release.sh vX.Y.Z [--skip-ui] [--publish]}
shift
skip_ui=0 publish=0
for a in "$@"; do
  case $a in
    --skip-ui) skip_ui=1 ;;
    --publish) publish=1 ;;
    *) echo "unknown option $a" >&2; exit 2 ;;
  esac
done

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[[ $tag == "v$version" ]] || { echo "Cargo.toml says $version, not ${tag#v}" >&2; exit 1; }
[[ -z $(git status --porcelain) ]] || { echo "the tree is not clean" >&2; exit 1; }
# this version's section of CHANGELOG.md, without its heading
changes=$(awk -v v="$version" '
  /^## / { on = ($2 == v); next }
  on' CHANGELOG.md | sed -e '/./,$!d' | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}' |
  # one line per paragraph and list item: GitHub shows every line break
  awk 'NF == 0 { if (buf != "") print buf; buf = ""; print ""; next }
       /^- / { if (buf != "") print buf; buf = $0; next }
       { sub(/^ +/, ""); buf = (buf == "" ? $0 : buf " " $0) }
       END { if (buf != "") print buf }')
[[ -n $changes ]] || { echo "CHANGELOG.md has nothing for $version" >&2; exit 1; }
grep -q "^## $version (" CHANGELOG.md ||
  { echo "CHANGELOG.md: date the heading, like ## $version ($(date +%Y-%m-%d))" >&2; exit 1; }
commit=$(git rev-parse --short HEAD)

make install-user
omarchy-osk-restart || true
make check | tee target/check.log
golden=$(grep -c '^PASS' target/check.log)
units=$(grep -o 'test result: ok. [0-9]* passed' target/check.log | awk '{s += $4} END {print s}')
python3 -I tests/install_test.py | tee target/install.log
installer="installer $(tail -1 target/install.log)"
if ((skip_ui)); then
  ui="end-to-end tests not run"
else
  python3 -I tests/ui_test.py | tee target/ui.log
  ui="end-to-end $(tail -1 target/ui.log)"
fi

device=$(cat /sys/class/dmi/id/product_name 2>/dev/null || echo unknown)
grep -q 'Type Cover' /proc/bus/input/devices 2>/dev/null && device="$device with the Type Cover"
cat >target/TESTED <<EOF
version=$version
commit=$commit
date=$(date -u +%Y-%m-%d)
omarchy=$(omarchy version | head -1 | cut -d- -f1)
hyprland=$(hyprctl version -j | jq -r .version)
device=$device
tests=$units unit tests and $golden golden tests passed; $installer; $ui
EOF
cat target/TESTED

make dist
ls -la target/omarchy-osk-x86_64.tar.gz target/SHA256SUMS

if ((publish)); then
  git tag -a "$tag" -m "omarchy-osk $version"
  git push origin "$tag"
  {
    echo "$changes"
    echo
    # install.sh picks the release for the user's Omarchy from "with Omarchy X.Y.Z"
    echo "Tested on $(sed -n 's/^device=//p' target/TESTED) with Omarchy $(sed -n 's/^omarchy=//p' target/TESTED) and Hyprland $(sed -n 's/^hyprland=//p' target/TESTED): $(sed -n 's/^tests=//p' target/TESTED)."
    echo
    echo "Install or update: \`curl -fsSL https://github.com/wouter-leistra/omarchy-osk/releases/latest/download/install.sh | bash\`"
    echo
    echo "Made for the Surface Pro 3 with the Type Cover; other devices may work but are not tested."
  } >target/notes.md
  gh release create "$tag" -R wouter-leistra/omarchy-osk --title "omarchy-osk $version" --notes-file target/notes.md \
    install.sh target/omarchy-osk-x86_64.tar.gz target/SHA256SUMS
fi
