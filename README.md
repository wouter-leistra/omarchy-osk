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

## Credits

- [wvkbd](https://github.com/jjsullivan5196/wvkbd) by John Sullivan and
  contributors (GPL-3.0), which this is built on.
- Word lists in `dict/` are derived from
  [FrequencyWords](https://github.com/hermitdave/FrequencyWords) by Hermit Dave,
  based on OpenSubtitles (content licensed CC BY-SA 4.0).

## License

GPL-3.0, like wvkbd.
