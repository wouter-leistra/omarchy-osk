# C reference tests for the Rust port

The Rust port in `rust/` is checked against the C version, which serves as
the reference: same input, same output.

- `harness.c` runs the real C `keyboard.c`, `swipe.c`, `langs.c` and
  `drw.c` headless. libwayland is replaced by stubs that print every
  virtual-keyboard request (`key`, `mods`, `keymap`), and the surfaces can
  be dumped to PNG. `rust/src/bin/osk-trace.rs` is the same program around
  the Rust code. Both take the options of `omarchy-oskd` (plus `-W`, `-H`,
  `-S` for width, height and scale) and a script on stdin:

  ```
  down X Y | move X Y | up | cancel    touch events (surface coordinates)
  longpress                            the long-press timer fires
  sleep MS                             a real pause
  hide | show | reset | private on|off what the signals do
  png NAME                             NAME.kbd.png and NAME.popup.png
  keymap FILE                          the uploaded keymap
  ```

  After each step both print the keyboard state (layout, language,
  modifiers, current word, suggestions, open long-press keys).
- `compare.py` generates scripts for typing, gestures, suggestions,
  long press, shift and caps lock, the symbol pages and shortcuts, private
  mode, hide/show, cancel, several languages and sizes and scales, and
  requires identical logs, keymaps and pixels.
- `decode_ref.c` / `rust/src/bin/osk-decode.rs` and `decode_compare.py` do
  the same for the gesture decoder and completions on 2400 sloppy swipes and
  prefixes in English, Finnish and Swedish.
- `live_compare.py C_BIN RUST_BIN` installs each daemon in turn and compares
  screenshots of the real keyboard in several states (needs the machine to
  itself, like `tests/ui_test.py`).

`make check-rust` builds everything and runs the first three.
