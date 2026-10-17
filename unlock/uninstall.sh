#!/bin/bash
# Remove the touch passphrase prompt and rebuild the initramfs (needs root).
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  systemctl --user disable --now omarchy-osk-boot-look.path 2>/dev/null || true
  exec sudo "$0" "$@"
fi

systemctl disable --now omarchy-osk-boot-look.path 2>/dev/null || true
rm -f /etc/systemd/system/omarchy-osk-boot-look.path /etc/systemd/system/omarchy-osk-boot-look.service
systemctl daemon-reload
rm -rf /boot/omarchy-osk

rm -f /etc/mkinitcpio.conf.d/omarchy_osk_unlock.conf \
  /etc/initcpio/install/omarchy-osk-unlock /etc/initcpio/hooks/omarchy-osk-unlock \
  /etc/unl0kr.conf /usr/local/bin/unl0kr \
  /usr/local/lib/omarchy-osk/omarchy-look /usr/local/lib/omarchy-osk/boot-look-sync \
  /etc/omarchy-osk-unlock.conf

if command -v limine-mkinitcpio >/dev/null; then
  limine-mkinitcpio
else
  mkinitcpio -P
fi
