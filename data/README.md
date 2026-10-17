# Layout data

What the letter keys show and offer on long press, per language. Installed to
`~/.local/share/omarchy-osk/`; files of the same name under
`~/.config/omarchy-osk/` take precedence.

- `languages.txt`: `tag layout name` per line, the default layout of each
  language and the name shown on the space bar. `OSK_LANGS=de:qwerty`
  overrides the layout. A layout name ending in `+` gets the `[extra_keys]`
  of the language added to its rows.
- `layouts/<name>.txt`: one key per line, its long-press keys after it, and
  a blank line between rows. `label|text` types `text`; `!shift!X` sets the
  shifted key when it isn't simply the upper case (e.g. `'` and `?` on
  azerty). Escape a leading `!`, `%` or `\` with `\`.
- `locales/<tag>.txt`: per language, sections `[popup_keys]` (a key, then
  its long-press keys; `punctuation` is the `.` key), `[extra_keys]`
  (`row: key long-press…`) and `[number_row]` (digits offered on the top
  row). Other sections are ignored. `more_popups_main.txt` is used for
  every language after its own keys.
- `popups.txt`: long-press keys for every language, in the same format
  (punctuation, symbols, accents), after the language's own.

## Source and license

`layouts/`, `locales/` and `languages.txt` are imported from
[HeliBoard](https://github.com/HeliBorg/HeliBoard) by
`tools/import-heliboard.py` (the commit is noted in `languages.txt`).
HeliBoard is GPL-3.0, like omarchy-osk; some of its layouts were taken
from [FlorisBoard](https://github.com/florisboard/florisboard) (Apache-2.0).
Changes made by the import: JSON layouts are flattened to the text format
(only the normal keys, the alphabet variant of selectors, and the shifted
key as `!shift!`), and the language list is reduced to the first (default)
layout of each language. The display names in `languages.txt` and
`popups.txt` are omarchy-osk's own.

To update: check out HeliBoard (see the script's help) and run
`tools/import-heliboard.py <checkout>`, then `make check`.
