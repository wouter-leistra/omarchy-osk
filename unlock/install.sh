#!/bin/bash
# Install the touch passphrase prompt into the initramfs (needs root).
# Run build.sh and `make install-user` first. The existing prompt stays as the
# fallback.
set -euo pipefail

here="$(dirname "$(readlink -f "$0")")"
[[ -x $here/build/unl0kr ]] || { echo "run unlock/build.sh first" >&2; exit 1; }

if [[ $EUID -ne 0 ]]; then
  # as the user: follow theme changes and render the current look, then the
  # root part
  if [[ -x $HOME/.local/bin/omarchy-osk-boot-look ]]; then
    systemctl --user daemon-reload
    systemctl --user enable --now omarchy-osk-boot-look.path
    omarchy-osk-boot-look || true
  fi
  exec sudo "$0" "$@"
fi

user="${SUDO_USER:-$(logname 2>/dev/null || true)}"
home=""
[[ -n $user && $user != root ]] && home=$(getent passwd "$user" | cut -d: -f6)

install -Dm755 "$here/build/unl0kr" /usr/local/bin/unl0kr
install -Dm644 "$here/unl0kr.conf" /etc/unl0kr.conf
install -Dm755 "$here/omarchy-look" /usr/local/lib/omarchy-osk/omarchy-look
install -Dm755 "$here/boot-look-sync" /usr/local/lib/omarchy-osk/boot-look-sync
# whose Omarchy theme the boot screen follows
[[ -n $home ]] && printf 'OSK_USER=%s\n' "$user" > /etc/omarchy-osk-unlock.conf
install -Dm644 "$here/install" /etc/initcpio/install/omarchy-osk-unlock
install -Dm644 "$here/hook" /etc/initcpio/hooks/omarchy-osk-unlock
install -Dm644 "$here/mkinitcpio.conf" /etc/mkinitcpio.conf.d/omarchy_osk_unlock.conf

# theme changes reach the boot screen through the EFI partition: copy what
# omarchy-osk-boot-look renders whenever it changes
if [[ -n $home && $(findmnt -no FSTYPE /boot 2>/dev/null) == vfat ]]; then
  look="$home/.local/state/omarchy-osk/boot-look"
  mkdir -p /boot/omarchy-osk
  cat > /etc/systemd/system/omarchy-osk-boot-look.path <<UNIT
[Unit]
Description=Copy the boot passphrase screen's look to the EFI partition

[Path]
PathChanged=$look/omarchy.conf

[Install]
WantedBy=multi-user.target
UNIT
  cat > /etc/systemd/system/omarchy-osk-boot-look.service <<UNIT
[Unit]
Description=Copy the boot passphrase screen's look to the EFI partition
RequiresMountsFor=/boot

[Service]
Type=oneshot
ExecStart=/usr/local/lib/omarchy-osk/boot-look-sync $look /boot/omarchy-osk
ProtectSystem=strict
ReadWritePaths=/boot/omarchy-osk
ProtectHome=read-only
PrivateTmp=yes
PrivateNetwork=yes
NoNewPrivileges=yes
UNIT
  systemctl daemon-reload
  systemctl enable --now omarchy-osk-boot-look.path
  /usr/local/lib/omarchy-osk/boot-look-sync "$look" /boot/omarchy-osk || true
fi

if command -v limine-mkinitcpio >/dev/null; then
  limine-mkinitcpio
else
  mkinitcpio -P
fi
