#!/bin/bash
# Install the touch passphrase prompt into the initramfs (needs root).
# Run build.sh first. The existing prompt stays as the fallback.
set -euo pipefail

here="$(dirname "$(readlink -f "$0")")"
[[ -x $here/build/unl0kr ]] || { echo "run unlock/build.sh first" >&2; exit 1; }
[[ $EUID -eq 0 ]] || exec sudo "$0" "$@"

install -Dm755 "$here/build/unl0kr" /usr/local/bin/unl0kr
install -Dm644 "$here/unl0kr.conf" /etc/unl0kr.conf
install -Dm644 "$here/install" /etc/initcpio/install/omarchy-osk-unlock
install -Dm644 "$here/hook" /etc/initcpio/hooks/omarchy-osk-unlock
install -Dm644 "$here/mkinitcpio.conf" /etc/mkinitcpio.conf.d/omarchy_osk_unlock.conf

if command -v limine-mkinitcpio >/dev/null; then
  limine-mkinitcpio
else
  mkinitcpio -P
fi
