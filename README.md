# omarchy-osk

A fast, Gboard-style on-screen keyboard for [Omarchy](https://omarchy.org)
(Hyprland), written in Rust, made for the Surface Pro 3 and its Type Cover:
swipe typing with suggestions, a layout per language, long-press accents,
and it docks below your windows instead of covering them.

Inspired by [wvkbd](https://github.com/jjsullivan5196/wvkbd): omarchy-osk
started as a fork of it in C and was then rewritten in Rust. No wvkbd code
runs in it anymore, but because the Rust code was ported from wvkbd's, it
keeps wvkbd's GPL-3.0 license and credits (see [Credits](#credits-and-thanks)).

> **Made for one device.** omarchy-osk is built and tested on a **Microsoft
> Surface Pro 3 with the Surface Type Cover**, on **Omarchy 4.0.4**. Other
> tablets, touch laptops and touchscreens may work, but nothing is
> guaranteed: touch, pen and the "show while the keyboard is detached"
> behaviour are only tested there, and newer Omarchy versions may break the
> parts that patch Omarchy's own menu, theme switcher and lock screen.
>
> **The boot passphrase prompt is a different keyboard.** The optional disk
> encryption prompt (`unlock/`) uses [unl0kr](https://gitlab.postmarketos.org/postmarketOS/buffybox)
> from postmarketOS, not omarchy-osk: no swipe typing, no suggestions and
> none of omarchy-osk's layouts. Its touch keyboard is US English (unl0kr
> also has German, Spanish and French layouts), so characters such as
> `å ä ö` can't be typed by touch there. The Type Cover does type your own
> layout (`fi` here), so a passphrase with such characters needs the cover.

## Features

- **Gesture (swipe) typing** with a suggestion strip: swipe across letters to
  type a word, tap a suggestion to swap it, backspace right after a swipe
  removes the whole word.
- **English and Finnish dictionaries** (Swedish included); swipe across the
  space bar to switch language, the choice is remembered.
- **A layout per language**, taken from
  [HeliBoard](https://github.com/HeliBorg/HeliBoard): about 80 layouts
  (qwerty, qwertz, azerty, Cyrillic, Greek, …) for 100+ languages. Finnish
  gets `å ö ä`, English plain qwerty, German qwertz. Keys type through a
  real xkb keymap (`fi` by default) so symbols like `@ € | £` come out right.
- **Long-press alternates** per language (numbers on the top row, ä å on
  `a` in English, é ü ñ ø …, extra punctuation); characters the keymap lacks
  are added to it.
- **Docks below windows**: Hyprland shrinks the windows instead of the
  keyboard covering what you type.
- **Omarchy theme and font**, re-applied on theme/font change.
- **Touch, mouse and Surface pen** (tablet-v2) input.
- Long-press space and slide to move the cursor; double space types `. `;
  punctuation attaches to the previous word; automatic capitals.
- **Auto-show** for focused text fields while the Type Cover is detached,
  driven by fcitx5's virtual keyboard D-Bus interface.

## Performance

Measured on the Surface Pro 3 it was made for (Intel Core i5-4300U, 4 GB,
Hyprland 0.56; keyboard 2160×576, English and Finnish word lists with
96,428 words), against the C version it was rewritten from. Both behave
identically: the tests compare every key event, keymap, rendered pixel and
swipe result with recordings of the C version.

| | C version | omarchy-osk | |
|---|---:|---:|---|
| **Daemon** (real Wayland session) | | | |
| shown after the show signal | 144 ms | 2.6 ms | 55× faster |
| ready after start | 323 ms | 31 ms | 10× faster |
| ready after start, first time (nothing cached) | 192 ms | 146 ms | |
| memory while shown, RSS / PSS | 39.7 / 27.4 MB | 26.0 / 13.6 MB | half the PSS |
| memory while hidden, RSS / PSS | 27.1 / 21.3 MB | 26.0 / 15.9 MB | |
| CPU and wakeups while idle | none | none | |
| binary | 0.33 MB | 1.2 MB | |
| **Gesture typing** (2,400 sloppy swipes and prefixes) | | | |
| decode a swipe, median | 7.1 ms | 0.49 ms | 15× faster |
| decode a swipe, 95th percentile | 14.8 ms | 1.9 ms | 8× faster |
| decode a swipe, worst | 81 ms | 17 ms | |
| complete a prefix, median / 95th percentile | 9 / 219 µs | 8 / 67 µs | |
| load the word lists | 206 ms | 0.06 ms (cached), 129 ms (first time) | |
| **Per input step** (headless, including drawing) | | | |
| mean of typing, swiping, long press, languages, shift | 0.53 ms | 0.44 ms | |
| peak memory | 45 MB | 33 MB | |

How:

- **Dictionary**: 12 bytes per word plus its text (the C version used 188);
  the sorted dictionary is cached in `~/.cache/omarchy-osk` and
  memory-mapped, so it costs no private memory.
- **Gesture decoder**: each word's key sequence is worked out once per
  layout, while idle, and words are grouped by first and last key, so a
  swipe only looks at words that start and end where it does. Whether a
  swipe passes every letter in order is answered from per-key lists of
  nearby points. Results are the same as checking every word.
- **Drawing**: straight into the one shared-memory buffer of each surface,
  without a second buffer or a copy; laid-out labels are cached.
- **Showing**: the buffers are kept while hidden and the next show is drawn
  ahead of time, so showing only attaches them. Fonts load while idle.
- **Signals** are blocked from the program's first code, so a show or hide
  sent while it starts waits instead of ending the process.

The first swipe on a newly chosen layout also builds its key sequences
(38 ms); the keyboard does that while idle right after showing, so it
doesn't delay typing. Reproduce with `make bench` (`tests/bench.py --live`;
point `--trace`, `--decode` and `--daemon` at other builds to compare).
Numbers vary with system load; it was about 2 during these runs.

## Install

```sh
curl -fsSL https://github.com/wouter-leistra/omarchy-osk/releases/latest/download/install.sh | bash
```

The installer downloads the latest release (a prebuilt keyboard, its scripts
and data; the checksum is verified), installs it for your user in
`~/.local` (no root needed) and runs `omarchy-osk-setup`, which asks:

- **languages** (in space-bar switching order). English, Finnish and
  Swedish come with word lists for swipe typing; for other languages it
  offers to download one from
  [FrequencyWords](https://github.com/hermitdave/FrequencyWords) and keep
  the words that can be swiped on that language's layout. Without a word
  list a language still gets its layout and long-press keys.
- **when to show the keyboard**: when a text field gets focus while a
  keyboard you pick (the Surface Type Cover) is detached, on every text
  field, or only with the bar button.
- **its height**, a **button in the bar**, and the **patched Omarchy menu,
  theme switcher and lock screen** (below).
- the **Hyprland layer rule** for the slide-in and the lock screen, added to
  `~/.config/hypr/looknfeel.lua` between marker comments.

It also adds theme and font hooks, so the keyboard follows your Omarchy
theme. To change the answers later, open **Setup › On-screen keyboard** in
the Omarchy menu, search "On-screen keyboard setup" in the app launcher, or
run `omarchy-osk-setup`. `omarchy-osk-uninstall` removes everything again
(Omarchy's own menu, theme switcher and lock screen come back; `--purge`
also removes your settings).

**Tested versions.** Every release is made on the Surface Pro 3 with all
tests passing (`tools/release.sh`), and records which Omarchy and Hyprland
it was tested with (`~/.local/share/omarchy-osk/TESTED`). The installer
warns when your Omarchy is newer, and after an `omarchy update` past the
tested version a notification says so: the keyboard itself should keep
working, the patched Omarchy plugins are the parts most likely to break.

Needs Omarchy (x86_64), which brings everything else: pango, cairo,
libxkbcommon, gum, jq and python-gobject (for auto-show).

### From source

```sh
make install-user        # needs Rust: builds and installs the files to ~/.local
omarchy-osk-setup        # the same questions as above
```

### Omarchy menu, theme switcher and lock screen

Out of the box the Omarchy menu (Super+Space) and the theme/background
switcher cover the whole screen and take exclusive keyboard focus. Hyprland
then sends every touch to them, so the on-screen keyboard can't type into
them and a tap on it closes the menu. The lock screen hides the keyboard
altogether. omarchy-osk keeps patched copies of these three Omarchy plugins
(`omarchy/patches/`):

```bash
omarchy-osk-plugins    # copies menu, image-picker and lock to ~/.config/omarchy/plugins/<you>.*, patched
```

- menu and image picker: `WlrKeyboardFocus.OnDemand` instead of `Exclusive`
  and `ExclusionMode.Normal` with `exclusiveZone: 0`, so they open above the
  keyboard and take its typing; the menu also gets its own app library
  (a copied menu gets none from the shell, which leaves Apps empty).
- lock screen: shows the keyboard on lock (see below).

When you choose them in setup, an Omarchy post-update hook runs it after
every `omarchy update`: when Omarchy changed one of these plugins, it takes
the new version and patches it again. If a patch no longer fits, your
previous copy stays and a notification says so. `omarchy-osk-plugins --force`
redoes all three.

`above_lock = 2` in the layer rule lets the keyboard show and take touches
on the lock screen. On lock the patched lock screen runs `omarchy-osk-lock
lock`: the keyboard switches to private mode (no suggestions, gestures,
automatic capitals or smart punctuation, so the password never appears on
screen) and shows itself while the Type Cover is detached. A button under the password field shows or hides it. Unlocking
switches private mode off again.

### Disk encryption passphrase at boot (optional, separate)

This is **not omarchy-osk** and not part of the installer. The LUKS
passphrase is asked in the initramfs, long before Wayland, where
omarchy-osk can't run. `unlock/` adds an mkinitcpio hook that asks it with
[unl0kr](https://gitlab.postmarketos.org/postmarketOS/buffybox) from
postmarketOS: a full-screen touch keyboard without swipe typing,
suggestions or omarchy-osk's layouts. Its touch layout is US English
(unl0kr also has German, Spanish and French), so `å ä ö` and other
characters outside that layout can't be typed by touch; the Type Cover
types your own layout (`fi` here), so such a passphrase needs the cover.
It needs sudo and changes your initramfs, so only install it if you know
how to recover from a boot problem (the hook falls back to Omarchy's own
prompt, see below):

```bash
unlock/build.sh      # builds unl0kr 3.6.0 as your user (meson/ninja in a venv)
unlock/install.sh    # needs sudo: installs it and rebuilds the initramfs
```

The hook runs right before `encrypt` and only adds a way in: when unl0kr
can't start, is closed, or gets a wrong passphrase three times, `encrypt`
asks the usual way (Plymouth prompt). Add `oskunlock=0` to the kernel command
line in the boot menu to skip it once; `unlock/uninstall.sh` removes it. What
happened is in the kernel log (`journalctl -b -k | grep omarchy-osk-unlock`).
Settings such as the touch layout (`us`, `de`, `fr`, `es`) are in
`/etc/unl0kr.conf`. `tests/unlock_vm.py` tests it in QEMU with a throwaway
LUKS disk.

It looks like the Omarchy lock screen: `unlock/unl0kr-omarchy.patch` adds an
`[omarchy]` section to unl0kr (theme colours, the lock screen's password
field, no header bar) and JetBrains Mono fonts. The on-screen keyboard
hides while the Type Cover is attached and slides up when it is detached
(`OSK_COVER_NAME`, as for auto-show). The colours and the blurred
wallpaper follow your Omarchy theme: on every theme or background change
`omarchy-osk-boot-look` (a user path unit) renders them, a root path unit
copies them to the EFI partition (`/boot/omarchy-osk`, checked by
`unlock/boot-look-sync`), and the hook reads them from there at boot. The
look built into the initramfs is the fallback.

## Commands

| Command | Does |
|---|---|
| `omarchy-osk-setup` | choose languages, auto-show, height, bar button, Omarchy patches (also in the Omarchy menu under Setup) |
| `omarchy-osk-toggle` | show/hide, starting it if needed |
| `omarchy-osk` | start the keyboard with theme colours (extra args passed on, e.g. `--hidden`) |
| `omarchy-osk-restart` | restart it (used by the theme/font hooks) |
| `omarchy-osk-tested` | which Omarchy version this release was tested with, against yours |
| `omarchy-osk-plugins` | update the patched Omarchy plugins (run after `omarchy update`) |
| `omarchy-osk-uninstall` | remove everything (`--purge`: settings too) |
| `omarchy-osk-autoshow` | auto-show daemon (run via the user service) |
| `omarchy-osk-lock` | lock screen: `lock`, `show`, `hide`, `unlock` |

## Settings

`~/.config/omarchy-osk/config`:

```sh
OSK_LANGS="en,fi"         # languages, in space-bar switching order
OSK_HEIGHT_RATIO="0.40"   # share of the screen height
OSK_XKB="fi"              # xkb layout[:variant] to type with (default: Hyprland's)
OSK_AUTOSHOW="cover"      # cover: while OSK_COVER_NAME is detached; always; never
OSK_COVER_NAME="Microsoft Surface Type Cover Keyboard"
```

`omarchy-osk-setup` writes these; edit them by hand and run
`omarchy-osk-restart` (and `systemctl --user restart omarchy-osk-autoshow`
for the last two) if you prefer.

Each language is a tag (`fi`, `de`, `de-CH`, `ru`, `fr-CA` …) with an
optional layout: `OSK_LANGS=en,fi,de:qwerty` gives German on qwerty instead
of its default qwertz. A layout name ending in `+` also gets the language's
extra keys (`fi` uses `qwerty+`: qwerty plus `å ö ä`). The available tags
and their default layouts are in `data/languages.txt`, the layouts in
`data/layouts/`.

## Adding a language

`omarchy-osk-setup` adds languages and fetches their word lists. To make a
word list yourself: swipe typing and suggestions use plain word-frequency
lists in `~/.local/share/omarchy-osk/dict/<tag>.txt` (the repo copies are in
`dict/`), `word count` per line, most frequent first. The keyboard turns
any such list into one with only the words that can be swiped on a
language's layout (lower case, letters that have a key, plus `'`):

```sh
curl -fsSL https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/de/de_50k.txt |
  omarchy-oskd --filter-dict de-DE > ~/.local/share/omarchy-osk/dict/de-DE.txt
```

Letters that are only on long press can't be swiped, so words with them are
left out (plain `de` is qwertz with `ü ö ä` on long press; `de-DE` has them
as keys). Then add the tag to `OSK_LANGS` and `omarchy-osk-restart`. To add
a list to the repo itself, put it in `dict/`.

## Keymap and layout

Two separate things decide what you get:

- **`OSK_XKB`** is the xkb layout used to *type* (default `fi`, same as the
  Type Cover). Keys look up their character in this keymap, so `@`, `€` etc.
  always arrive correctly, and characters the keymap lacks (é, ñ, — …) are
  added to spare keycodes automatically. Use the layout your physical
  keyboard uses, e.g. `OSK_XKB=us`, `OSK_XKB=se`, `OSK_XKB=de:nodeadkeys`.
- **The language's layout** decides what the letter keys *show* and type.
  Layouts are plain text files (`data/layouts/`, imported from HeliBoard by
  `tools/import-heliboard.py`): one key per line, its long-press keys after
  it, and a blank line between rows:

  ```
  q
  w
  e é è

  a
  ```

  Rows are centred; Shift and Backspace share what the last row leaves
  free. The `?123` symbol pages and the bottom row are built in
  (`builtin_sets()` in `src/keyboard.rs`).

Long-press keys come, in this order, from: the digit on the top row (or the
language's `[number_row]`), the layout file, the language's `[popup_keys]`
in `data/locales/<tag>.txt` (HeliBoard format), `data/popups.txt` (the same
for every language: punctuation, symbols, common accents) and HeliBoard's
common accents (`locales/more_popups_main.txt`). On a letter key the
characters that are keys of the layout already are left out, so Finnish `a`
doesn't offer `ä`, English `a` does.

To change any of it, copy the file to `~/.config/omarchy-osk/` under the same
name (e.g. `~/.config/omarchy-osk/layouts/qwerty.txt` or
`~/.config/omarchy-osk/locales/en.txt`) and edit it there: those files win
over the installed ones. Then `omarchy-osk-restart`. `make check` checks the
data in the repo.

## Development

The daemon is `omarchy-oskd` (`src/main.rs`: Wayland, `src/keyboard.rs`:
keys, gestures, suggestions and drawing, `src/swipe.rs`: the gesture
decoder, `src/langs.rs`: layout data, `src/keymap.rs`: the xkb keymap).

```sh
make check              # unit tests and the golden tests
make bench              # speed and resource use
tests/install_test.py   # installer, setup and uninstaller in a throwaway home
tests/ui_test.py        # end to end on the real desktop (needs the machine to itself)
make dist               # the release tarball
tools/release.sh vX.Y.Z [--publish]   # all of the above on this machine, TESTED, release
```

The golden tests (`tests/golden.py`) run touch scripts through a headless
build (`osk-trace`) and compare every key event, the keymap and the pixels of
both surfaces with recordings, and the gesture decoder's results on 2400
sloppy swipes. The recordings were made with the original C version, so the
Rust version is checked to behave exactly as it did. The last C version is
tagged `c-version` in this repository.

## Credits and thanks

- [wvkbd](https://github.com/jjsullivan5196/wvkbd) by John Sullivan and
  contributors (GPL-3.0), the inspiration and starting point. omarchy-osk
  began as a fork of it in C and was then rewritten in Rust; as a port of
  wvkbd's code it remains a derivative work under the same license. The
  wvkbd history is kept in this repository (everything since upstream
  commit `bbecd99` is omarchy-osk), and its readme is in
  [README.wvkbd.md](README.wvkbd.md).
- The Rust crates it builds on, listed in `Cargo.lock`, are under the MIT
  and/or Apache-2.0 licenses (some also Zlib, Unlicense or Unicode-3.0), all
  compatible with GPL-3.0: notably
  [wayland-rs](https://github.com/Smithay/wayland-rs),
  [gtk-rs](https://gtk-rs.org) (cairo, pango),
  [xkbcommon-rs](https://github.com/rust-x-bindings/xkbcommon-rs) and
  [libc](https://github.com/rust-lang/libc).
- Word lists in `dict/` are adapted from
  [FrequencyWords](https://github.com/hermitdave/FrequencyWords) by Hermit Dave,
  built from [OpenSubtitles](https://www.opensubtitles.org) data, and licensed
  [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). See
  [dict/README.md](dict/README.md) for what was changed.
- The swipe decoder follows the idea of SHARK² by Per Ola Kristensson and
  Shumin Zhai ("SHARK²: A Large Vocabulary Shorthand Writing System for
  Pen-based Computers", UIST 2004). No code was taken from any implementation.
- The boot passphrase prompt uses
  [unl0kr](https://gitlab.postmarketos.org/postmarketOS/buffybox) from
  postmarketOS (GPL-3.0-or-later), built from its source by `unlock/build.sh`
  with `unlock/unl0kr-omarchy.patch`. Its fonts in `unlock/fonts/` are
  generated with lv_font_conv from
  [JetBrains Mono](https://www.jetbrains.com/lp/mono/) (Nerd Fonts build,
  SIL OFL 1.1) and Font Awesome 5 Free (SIL OFL 1.1), as upstream's are.
- The layouts and per-language key data in `data/layouts/`, `data/locales/`
  and `data/languages.txt` come from
  [HeliBoard](https://github.com/HeliBorg/HeliBoard) (GPL-3.0), some of
  whose layouts come from [FlorisBoard](https://github.com/florisboard/florisboard)
  (Apache-2.0). See [data/README.md](data/README.md).
- The Wayland protocol file in `tests/proto/` carries its own copyright
  notice.
- Thanks to [Omarchy](https://omarchy.org) for the theme, bar plugin and hook
  system this plugs into, to [fcitx5](https://github.com/fcitx/fcitx5) for
  the virtual keyboard DBus interface used for auto-show, and to
  [Nerd Fonts](https://www.nerdfonts.com) for the icons.

## License

GPL-3.0, as wvkbd: see [LICENSE](LICENSE) and [COPYING](COPYING). The word
lists in `dict/` are CC BY-SA 4.0; the layout data in `data/` comes from
HeliBoard (GPL-3.0, partly FlorisBoard's Apache-2.0).
