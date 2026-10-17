# Benchmarks

The Rust keyboard against the C version it was ported from (tag
`c-version`), with the same inputs, on the machine it is made for.

- Surface Pro 3: Intel Core i5-4300U, 4 GB, Omarchy, Hyprland 0.56,
  kernel 7.2, cairo 1.18.4, pango 1.58; keyboard 2160×576, English and
  Finnish word lists (96,428 words), the user's theme and font.
- `make bench` (`tests/bench.py --live`), medians of several runs. The C
  numbers come from the same script pointed at the C programs (built from
  `c-version`: the daemon, `tests/ref/harness.c` and a timing build of
  `tests/ref/decode_ref.c`).
- Both versions behave identically: `make check` compares every key event,
  keymap, rendered pixel and swipe result with recordings of the C version,
  and live screenshots of both daemons match.

| | C | Rust | |
|---|---:|---:|---|
| **Daemon** (real Wayland session) | | | |
| ready after start | 323 ms | 31 ms | 10× |
| ready after start, first time (no cache) | 192 ms | 146 ms | |
| shown after the show signal | 144 ms | 2.6 ms | 55× |
| memory hidden, RSS / PSS | 27.1 / 21.3 MB | 26.0 / 15.9 MB | |
| memory shown, RSS / PSS | 39.7 / 27.4 MB | 26.0 / 13.6 MB | PSS −50% |
| CPU and wakeups while idle | 0 / 0 | 0 / 0 | |
| binary | 0.33 MB | 1.2 MB | |
| **Gesture typing** (2,400 sloppy swipes and prefixes) | | | |
| decode a swipe, median | 7.1 ms | 0.49 ms | 15× |
| decode a swipe, 95th percentile | 14.8 ms | 1.9 ms | 8× |
| decode a swipe, worst | 81 ms | 17 ms | |
| complete a prefix, median / 95th percentile | 9 / 219 µs | 8 / 67 µs | |
| load the word lists | 206 ms | 0.06 ms (cached), 129 ms (first time) | |
| **Keyboard work per input step** (headless, incl. rendering) | | | |
| startup CPU | 177 ms | 15 ms (cached), 90 ms (first time) | |
| per step, mean of typing, swiping, long press, languages, shift | 0.53 ms | 0.44 ms | |
| peak memory | 45 MB | 33 MB | |

Where it comes from:

- **Dictionary**: 12 bytes per word plus its text instead of 188 bytes;
  the sorted dictionary is cached in `~/.cache/omarchy-osk` and
  memory-mapped (clean pages, shared with the page cache).
- **Decoder**: each word's key sequence is worked out once per layout
  (while idle) and words are grouped by first and last key, so a gesture
  only looks at plausible words; "passes every letter in order" is answered
  from per-key lists of nearby path points. Results stay identical: words
  are visited in the same order with the same arithmetic.
- **Drawing**: operations go straight into the one shared-memory buffer of
  each surface when the compositor has released it; no second buffer and
  no copy. Laid-out labels are cached.
- **Showing**: buffers outlive the surfaces, and the next show is drawn
  while hidden (or right after a hidden start), so showing only attaches
  them. Fonts are loaded while idle after startup.
- **Signals** are blocked from the program's first code, so a show or hide
  sent while it is starting waits instead of ending the process (in a
  stress test of random timings from spawn on: 48/60 starts survive, C
  29/60; the rest arrive before any code runs).

The first gesture on a layout also builds its key sequences (38 ms, C:
11 ms, not in the table); the daemon does that while idle after showing, so
in use the first gesture is as fast as the others. The worst ordinary
decode (17 ms) is a long, wandering path.

Numbers vary with load; the machine had a load average of about 2 during
these runs.
