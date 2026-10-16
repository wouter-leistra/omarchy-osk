# emit vptr commands for taps/swipes on the omarchy-osk fi layout
import sys
Y0, H, W = 864, 576, 2160
SUG = int(H / 4.7 * 0.7)
ROW = (H - SUG) // 4
U = W / 11
rows = ["qwertyuiopå", "asdfghjklöä", "zxcvbnm"]
off = [0, 0, 2]
def pos(c):
    special = {" ": (6.5 * U, 3), "\n": (9.25 * U, 3), "<": (10 * U, 2), "^": (1 * U, 2), ".": (8.5 * U, 3), ",": (2 * U, 3)}
    if c in special:
        x, r = special[c]
    else:
        for r, s in enumerate(rows):
            if c in s:
                x = (off[r] + s.index(c) + 0.5) * U
                break
    return x, Y0 + SUG + r * ROW + ROW / 2
def commands(tokens):
  out = []
  for tok in tokens:
      if tok.startswith("tap:"):
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
