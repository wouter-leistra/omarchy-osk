#!/bin/bash
# Remove the touch passphrase prompt and rebuild the initramfs (needs root).
set -euo pipefail

[[ $EUID -eq 0 ]] || exec sudo "$0" "$@"

rm -f /etc/mkinitcpio.conf.d/omarchy_osk_unlock.conf \
  /etc/initcpio/install/omarchy-osk-unlock /etc/initcpio/hooks/omarchy-osk-unlock \
  /etc/unl0kr.conf /usr/local/bin/unl0kr \
  /usr/local/lib/omarchy-osk/omarchy-look /etc/omarchy-osk-unlock.conf

if command -v limine-mkinitcpio >/dev/null; then
  limine-mkinitcpio
else
  mkinitcpio -P
fi
