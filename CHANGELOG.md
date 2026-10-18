# Changelog

What changed in each release, newest first. `tools/release.sh` puts a
version's section in its GitHub release notes and won't release a version
that has none here.

## 0.1.3 (2026-10-18)

- Made for Omarchy 4.0.5. Its lock screen now writes `Commons.Color.lock`,
  which the lock screen patch didn't fit, so `omarchy update` kept your old
  lock screen copy. The patch is updated for the new lock screen.
- The installer installs the release made for your Omarchy version instead of
  the latest: the newest release tested with your Omarchy or an older one.
  On Omarchy 4.0.4 that stays 0.1.2. `install.sh --which` says what it would
  install and why.
- When an Omarchy update doesn't fit the menu, theme switcher or lock screen
  patch, the notification says to run the install line again.

## 0.1.2 (2026-10-18)

- Setup can be opened from the Omarchy menu (**Setup › On-screen keyboard**)
  and from the app launcher ("On-screen keyboard setup"), in Omarchy's
  floating terminal. The uninstaller removes both entries again.
- The language picker said space picks a language; it is `x`.
- `omarchy-osk-setup --help` and `omarchy-osk-uninstall --help` no longer
  print a stray line.

## 0.1.1 (2026-10-18)

- The bar button now turns on during a first install. Setup enabled it
  before Omarchy had noticed the new plugin, so it failed every time.

## 0.1.0 (2026-10-18)

The first release.

- The keyboard, rewritten in Rust from the earlier C version (based on
  wvkbd): swipe typing, per-language layouts and long-press keys (from
  HeliBoard), word lists for English, Finnish and Swedish, and colours and
  font that follow your Omarchy theme.
- Much faster and lighter than the C version: the keyboard appears in about
  3 ms instead of 144 ms, decodes a swipe in about 0.5 ms instead of 7 ms,
  and uses half the memory.
- Shows itself when a text field gets focus while the Type Cover is
  detached; works on the lock screen, with a private mode for passwords.
- One-line installer with a setup menu (languages, when to show the
  keyboard, height, bar button, Omarchy patches, Hyprland layer rule) and
  an uninstaller.
- Says which Omarchy version it was tested with, and warns when yours is
  newer.
