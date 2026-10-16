# omarchy-osk

A Gboard-style on-screen keyboard for [Omarchy](https://omarchy.org) (Hyprland),
built for a Surface Pro 3 but usable on any touchscreen. It is a fork of
[wvkbd](https://github.com/jjsullivan5196/wvkbd); see `README.wvkbd.md` for the
original project.

## Features

- **Gesture (swipe) typing** with a suggestion strip: swipe across letters to
  type a word, tap a suggestion to swap it, backspace right after a swipe
  removes the whole word.
- **English and Finnish dictionaries** (Swedish included); swipe across the
  space bar to switch language, the choice is remembered.
- **Finnish/Swedish layout** (å ä ö), typed through a real `fi` xkb keymap so
  symbols like `@ € | £` come out right.
- **Long-press alternates** (numbers on the top row, é ü ñ ø …, extra
  punctuation); characters the layout lacks are added to the keymap.
- **Docks below windows**: Hyprland shrinks the windows instead of the
  keyboard covering what you type.
- **Omarchy theme and font**, re-applied on theme/font change.
- **Touch, mouse and Surface pen** (tablet-v2) input.
- Long-press space and slide to move the cursor; double space types `. `;
  punctuation attaches to the previous word; automatic capitals.
- **Auto-show** for focused text fields while the Type Cover is detached,
  driven by fcitx5's virtual keyboard D-Bus interface.

## Install

Needs `wayland`, `libxkbcommon`, `pango`, `cairo`, `wayland-protocols`,
`scdoc`, `python-gobject` (for auto-show) and Omarchy.

```sh
make install-user        # installs to ~/.local, no root needed
systemctl --user enable --now omarchy-osk-autoshow
```

Bar toggle: copy `omarchy/plugin/` to `~/.config/omarchy/plugins/omarchy-osk/`
and run `omarchy plugin enable user.osk right`. Theme hooks:
`omarchy hook install theme-set omarchy/plugin/theme-hook.sh` (and `font-set`).

Optional Hyprland slide-in animation (`~/.config/hypr/looknfeel.lua`):

```lua
hl.layer_rule({ match = { namespace = "^omarchy-osk$" }, animation = "slide bottom" })
```

## Commands

| Command | Does |
|---|---|
| `omarchy-osk` | start the keyboard with theme colours (extra args passed on, e.g. `--hidden`) |
| `omarchy-osk-toggle` | show/hide, starting it if needed |
| `omarchy-osk-restart` | restart hidden (used by the theme/font hooks) |
| `omarchy-osk-autoshow` | auto-show daemon (run via the user service) |

## Settings

`~/.config/omarchy-osk/config`:

```sh
OSK_LANGS=en,fi          # swipe dictionaries, in switching order
OSK_HEIGHT_RATIO=0.40    # share of the screen height
OSK_XKB=fi               # xkb layout[:variant] to type with
```

## Adding a language

Swipe typing and suggestions use plain word-frequency lists, one per
language, in `~/.local/share/omarchy-osk/dict/<code>.txt` (the repo copies
are in `dict/`). Each line is `word count`, most frequent first:

```
hello 1234567
world 456789
```

1. Get a frequency list, for example from
   [FrequencyWords](https://github.com/hermitdave/FrequencyWords)
   (`content/2018/<code>/<code>_50k.txt`).
2. Keep only words that can be swiped on the layout: lowercase letters that
   have a key, plus `'`. For the Finnish/Swedish layout:

   ```sh
   code=de
   curl -fsSL -o /tmp/$code.txt \
     https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/$code/${code}_50k.txt
   LC_ALL=C.UTF-8 awk '{w=tolower($1); if (w ~ /^[a-zåäö'"'"']+$/ && $2 >= 5) print w" "$2}' \
     /tmp/$code.txt > ~/.local/share/omarchy-osk/dict/$code.txt
   ```

   Words containing letters the layout lacks (`ü`, `ß`, `é` …) are skipped
   for gestures, so for such languages add those letters to the layout as
   well (see below) and to the awk pattern.
3. Add the code to `OSK_LANGS` in `~/.config/omarchy-osk/config`, in the
   order the space-bar swipe should cycle:

   ```sh
   OSK_LANGS=en,fi,de
   ```
4. `omarchy-osk-restart`. The space bar shows the language name for common
   codes (`lang_display_name()` in `keyboard.c`), otherwise the code.

To add it to the repo itself, put the file in `dict/` and run
`make install-user`.

## Keymap and layout

Two separate things decide what you get:

- **`OSK_XKB`** is the xkb layout used to *type* (default `fi`, same as the
  Type Cover). Keys look up their character in this keymap, so `@`, `€` etc.
  always arrive correctly, and characters the keymap lacks (é, ñ, — …) are
  added to spare keycodes automatically. Use the layout your physical
  keyboard uses, e.g. `OSK_XKB=us`, `OSK_XKB=se`, `OSK_XKB=de:nodeadkeys`.
- **`layout.omarchy.h`** is what the keys *show*. It currently has the
  Finnish/Swedish letter rows (`å ö ä`), a `?123` symbol page and a `=\<`
  page with Esc/Tab/Ctrl/Alt/Super/arrows. Each `CH("x", "X")` key types its
  label (lowercase / shifted), so changing a label is all it takes, e.g.
  replace `CH("å", "Å")` with `CH("ü", "Ü")` for German. Rows are separated
  by `EndRow`; key widths are relative within a row.

Long-press alternates live in the `alternates[]` table at the top of
`keyboard.c` (`{"e", "3éèêë€"}`): the key label, then the characters offered,
first one pre-selected. Letter keys whose first alternate is a digit show it
as a hint.

After editing either file: `make install-user && omarchy-osk-restart`.

## Credits

- [wvkbd](https://github.com/jjsullivan5196/wvkbd) by John Sullivan and
  contributors (GPL-3.0), which this is built on.
- Word lists in `dict/` are derived from
  [FrequencyWords](https://github.com/hermitdave/FrequencyWords) by Hermit Dave,
  based on OpenSubtitles (content licensed CC BY-SA 4.0).

## License

GPL-3.0, like wvkbd.
