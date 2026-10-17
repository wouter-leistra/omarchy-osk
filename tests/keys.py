# emit vptr commands for taps/swipes on the omarchy-osk layouts
import sys
Y0, H, W = 864, 576, 2160
SUG = int(H / 4.7 * 0.7)
ROW = (H - SUG) // 4

# letter rows per language, as built from data/ (rows are centred, Shift
# and Backspace share what the last row leaves free)
LAYOUTS = {
    "en": ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
    "fi": ["qwertyuiopå", "asdfghjklöä", "zxcvbnm"],
}
LANG = "en"


def use(lang):
    """geometry of the language the keyboard shows now"""
    global LANG
    LANG = lang


def pos(c):
    rows = LAYOUTS[LANG]
    units = max(max(len(r) for r in rows[:-1]), len(rows[-1]) + 2)
    u = W / units
    side = (units - len(rows[-1])) / 2
    b = W / 11  # the bottom row is the same for every language
    special = {" ": (6.5 * b, 3), "\n": (9.25 * b, 3), ".": (8 * b, 3), ",": (2 * b, 3),
               "<": (W - side * u / 2, 2), "^": (side * u / 2, 2)}
    if c in special:
        x, r = special[c]
    else:
        for r, s in enumerate(rows):
            if c in s:
                x = ((units - len(s)) / 2 + s.index(c) + 0.5) * u
                break
        else:
            raise ValueError(f"{c!r} is not on the {LANG} layout")
    return x, Y0 + SUG + r * ROW + ROW / 2


def key_width():
    rows = LAYOUTS[LANG]
    return W / max(max(len(r) for r in rows[:-1]), len(rows[-1]) + 2)


def commands(tokens):
  out = []
  for tok in tokens:
      if tok.startswith("lang:"):
          use(tok[5:])
      elif tok.startswith("tap:"):
          for c in tok[4:].replace("_", " ").replace("|", "\n"):
              x, y = pos(c)
              out += [f"move {x:.0f} {y:.0f}", "down", "sleep 40", "up", "sleep 60"]
      elif tok.startswith("swipe:"):
          w = tok[6:]
          x, y = pos(w[0])
          out += [f"move {x:.0f} {y:.0f}", "down"]
          for c in w[1:]:
              x, y = pos(c)
              out.append(f"to {x:.0f} {y:.0f}")
          out += ["up", "sleep 150"]
      elif tok.startswith("sugg:"):
          slot = int(tok[5:])
          out += [f"move {(slot + 0.5) * W / 3:.0f} {Y0 + SUG / 2:.0f}", "down", "sleep 40", "up", "sleep 100"]
  return out


if __name__ == "__main__":
    print("\n".join(commands(sys.argv[1:])))
