#!/usr/bin/python3 -I
"""Differential test: the C keyboard (tests/ref/harness.c) against the Rust
port (osk-trace), on generated touch scripts.

Both get the same options and script; their logs (every key, modifier and
keymap request plus the keyboard state after each step), uploaded keymaps
and rendered surfaces (PNG, compared pixel by pixel) must be identical.

usage: tests/ref/compare.py [--c /tmp/osk-ref] [--rust PATH] [scenario...]
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))

W, H = 2160, 576
FONT = "JetBrainsMono Nerd Font 29"
THEME = ["--bg", "161622", "--fg", "313244", "--fg-sp", "45475a", "--press", "585b70",
         "--press-sp", "585b70", "--text", "cdd6f4", "--text-sp", "cdd6f4",
         "--text-press", "cdd6f4", "--text-press-sp", "cdd6f4", "--fg-accent", "89b4fa",
         "--press-accent", "585b70", "--text-accent", "1e1e2e", "--trail", "89b4facc"]


def dict_paths(*langs):
    return ",".join(os.path.join(ROOT, "dict", f"{l}.txt") for l in langs)


def load_rows(lang_layout):
    """letter rows of a layout file (+ extra keys of the language)"""
    lang, layout = lang_layout
    path = os.path.join(ROOT, "data", "layouts", layout.rstrip("+") + ".txt")
    rows, cur = [], []
    for line in open(path, encoding="utf-8").read().splitlines():
        if line.strip():
            tok = line.split()
            lab = tok[0]
            if lab.startswith("\\") and len(lab) > 1:
                lab = lab[1:]
            cur.append(lab.split("|")[-1] if "|" in lab and not lab.startswith("|") else lab)
        elif cur:
            rows.append(cur)
            cur = []
    if cur:
        rows.append(cur)
    if layout.endswith("+"):
        loc = os.path.join(ROOT, "data", "locales", f"{lang}.txt")
        sec = ""
        for line in open(loc, encoding="utf-8").read().splitlines():
            if line.startswith("["):
                sec = line.strip("[]\n")
                continue
            tok = line.split()
            if sec == "extra_keys" and len(tok) >= 2:
                rows[int(tok[0].rstrip(":")) - 1].append(tok[1])
    return rows


class Geo:
    """key centres in surface coordinates, as kbd_build_lang_keys lays out"""

    def __init__(self, rows, w=W, h=H, swipe=True):
        self.rows, self.w, self.h = rows, w, h
        nrows = len(rows) + 1
        self.sug = int(h / (nrows + 0.7) * 0.7) if swipe else 0
        self.row_h = (h - self.sug) // nrows
        self.units = max(max([len(r) for r in rows[:-1]] or [0]), len(rows[-1]) + 2)
        self.u = w / self.units
        self.side = (self.units - len(rows[-1])) / 2
        self.b = w / 11  # bottom row: 1.5 1 5 1 1.5 1

    def y(self, r):
        return int(self.sug + r * self.row_h + self.row_h / 2)

    def pos(self, c):
        special = {" ": (6.5 * self.b, len(self.rows)), "\n": (9.25 * self.b, len(self.rows)),
                   ".": (8 * self.b, len(self.rows)), ",": (2 * self.b, len(self.rows)),
                   "<": (self.w - self.side * self.u / 2, len(self.rows) - 1),
                   "^": (self.side * self.u / 2, len(self.rows) - 1),
                   "?123": (0.75 * self.b, len(self.rows)), "hide": (10.5 * self.b, len(self.rows))}
        if c in special:
            x, r = special[c]
            return int(x), self.y(r)
        for r, row in enumerate(self.rows):
            if c in row:
                x = ((self.units - len(row)) / 2 + row.index(c) + 0.5) * self.u
                return int(x), self.y(r)
        raise ValueError(f"{c!r} not on layout")

    def sym(self, r, col):
        """a key of the 11 wide ?123 / =\\< pages"""
        return int((col + 0.5) * self.w / 11), self.y(r)


class Script:
    def __init__(self, geo):
        self.g, self.lines = geo, []

    def raw(self, *lines):
        self.lines += lines
        return self

    def tap_at(self, x, y):
        return self.raw(f"down {x} {y}", "up")

    def tap(self, text):
        for c in text:
            self.tap_at(*self.g.pos(c))
        return self

    def key(self, name):
        return self.tap_at(*self.g.pos(name))

    def swipe(self, word, steps=10):
        pts = [self.g.pos(c) for c in word]
        self.raw(f"down {pts[0][0]} {pts[0][1]}")
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            for s in range(1, steps + 1):
                self.raw(f"move {int(x0 + (x1 - x0) * s / steps)} {int(y0 + (y1 - y0) * s / steps)}")
        return self.raw("up")

    def longpress(self, c, cells=0, png=None):
        x, y = self.g.pos(c)
        self.raw(f"down {x} {y}", "longpress")
        if png:
            self.raw(f"png {png}")
        if cells:
            self.raw(f"move {int(x + cells * self.g.u)} {y}")
        return self.raw("up")

    def space_swipe(self, d=1):
        x, y = self.g.pos(" ")
        return self.raw(f"down {x} {y}", f"move {x + d * 120} {y}", f"move {x + d * 260} {y}", "up")

    def sugg(self, slot):
        return self.tap_at(int((slot + 0.5) * self.g.w / 3), self.g.sug // 2)

    def png(self, name):
        return self.raw(f"png {name}")


EN = ("en", "qwerty")
FI = ("fi", "qwerty+")


def scenarios():
    s = {}

    def sc(name, langs, dicts, build, extra=(), w=W, h=H, rows_of=EN):
        s[name] = (langs, dicts, build, list(extra), w, h, rows_of)

    def typing(t):
        t.tap("hello").png("typing1").tap(" ").tap("wor").png("typing2").sugg(1).tap("<").tap("x")
        t.tap("  ").tap("ok.").png("typing3").tap("i ").tap(",").tap("yes")
    sc("typing", "en,fi", ("en", "fi"), typing)

    def swiping(t):
        t.swipe("hello").png("swipe1").swipe("world").tap(".").swipe("hello").tap("<")
        t.swipe("good").sugg(0).png("swipe2").swipe("time").sugg(2).raw("png swipe3")
    sc("swiping", "en,fi", ("en", "fi"), swiping)

    def trail(t):
        x, y = t.g.pos("h")
        x2, y2 = t.g.pos("o")
        t.raw(f"down {x} {y}", f"move {x + 60} {y}", f"move {x + 200} {y - 20}", f"move {x2} {y2}", "png trail", "up")
    sc("trail", "en,fi", ("en", "fi"), trail)

    def shift(t):
        t.key("^").png("shift1").tap("a").tap("b").key("^").key("^").png("caps").tap("cd").key("^")
        t.tap("e").key("^").swipe("hello").key("^").key("^").swipe("world").key("^")
    sc("shift", "en,fi", ("en", "fi"), shift)

    def longpress(t):
        t.longpress("q").longpress("e", 1).longpress("a", 5, png="lp_a").longpress("n")
        t.longpress(".", 0, png="lp_dot").longpress("p", 0).key("^").longpress("e", 1, png="lp_E")
        t.longpress("m").longpress("z", 1)
    sc("longpress", "en,fi", ("en", "fi"), longpress)

    def cursor(t):
        x, y = t.g.pos(" ")
        t.tap("abc").raw(f"down {x} {y}", "longpress", f"move {x - 50} {y}", f"move {x - 200} {y}",
                         f"move {x - 420} {y}", f"move {x + 100} {y}", "up").tap("x")
    sc("cursor", "en,fi", ("en", "fi"), cursor)

    def languages(t):
        t.space_swipe(1).png("lang_fi").raw("# now Finnish")
        t.g.rows = load_rows(FI)
        t.g.__init__(t.g.rows)
        t.swipe("kiitos").tap("äö å").longpress("ö", 1).space_swipe(-1)
        t.g.__init__(load_rows(EN))
        t.tap("en").space_swipe(-1).png("lang_back")
    sc("languages", "en,fi", ("en", "fi"), languages)

    def symbols(t):
        g = t.g
        t.key("?123").png("sym").tap_at(*g.sym(1, 0)).tap_at(*g.sym(1, 2)).tap_at(*g.sym(0, 3))
        t.longpress  # noqa
        x, y = g.sym(0, 0)
        t.raw(f"down {x} {y}", "longpress", f"move {x + g.w // 11} {y}", "up")
        t.tap_at(int(g.w / 11), g.y(2)).png("sym2")  # =\<
        t.tap_at(*g.sym(1, 0))  # Ctrl latch
        t.tap_at(int(0.75 * g.b), g.y(3))  # ABC
        t.tap("c")
        t.key("?123").tap_at(int(g.w / 11), g.y(2)).tap_at(*g.sym(1, 2))  # Super
        t.tap_at(int(0.75 * g.b), g.y(3)).tap(" ")
        t.key("?123").tap_at(int(g.w / 11), g.y(2))
        t.key("^") if False else None
        for col in range(3, 11):
            t.tap_at(*g.sym(1, col))  # arrows, Home/End/PgUp/PgDn
        t.tap_at(*g.sym(0, 0)).tap_at(*g.sym(0, 1)).tap_at(*g.sym(0, 5))  # Esc Tab \
        t.tap_at(int(g.w / 11), g.y(2)).key("hide")
    sc("symbols", "en,fi", ("en", "fi"), symbols)

    def private(t):
        t.raw("private on").png("private").tap("ab ").swipe("hello").tap(".").key("^").tap("x")
        t.raw("private off").tap("hel").raw("png private_off")
    sc("private", "en,fi", ("en", "fi"), private)

    def hide_show(t):
        x, y = t.g.pos("a")
        t.tap("he").raw(f"down {x} {y}", "hide", "show", "up").tap("y").key("^").raw("hide", "show").tap("z")
        t.raw("reset").png("hidden")
    sc("hide_show", "en,fi", ("en", "fi"), hide_show)

    def cancel(t):
        x, y = t.g.pos("h")
        x2, y2 = t.g.pos("l")
        t.raw(f"down {x} {y}", f"move {x2} {y2}", "cancel", "png cancel").tap("o")
        t.raw(f"down {x} {y}", "longpress", "cancel").tap("p")
        t.raw("down 30 30", "cancel")
    sc("cancel", "en,fi", ("en", "fi"), cancel)

    def gaps(t):
        y1 = t.g.y(1)
        t.tap_at(20, y1).tap_at(W - 20, y1).tap_at(5, 5).tap_at(W - 5, t.g.sug // 2)
    sc("gaps", "en,fi", ("en", "fi"), gaps)

    def backspace(t):
        t.tap("abc<<").tap("d").swipe("hello").tap("<").tap("hel").sugg(1).tap("<<<").tap("<")
        x, y = t.g.pos("<")
        t.raw(f"down {x} {y}", f"move {x - 300} {y}", "up")
    sc("backspace", "en,fi", ("en", "fi"), backspace)

    def nodict(t):
        t.tap("abc").swipe("hello").tap(" .").longpress("e", 2).png("nodict")
    sc("nodict", "en,fi", (), nodict)

    def builtin(t):
        t.g.__init__([list("qwertyuiopå"), list("asdfghjklöä"), list("zxcvbnm")])
        t.tap("aö").swipe("hello").png("builtin")
    sc("builtin", None, ("en", "fi"), builtin, extra=["--data", "/nonexistent"])

    def many(t):
        t.png("many_en")
        for name, rows in [("fi", FI), ("ru", ("ru", "russian")), ("el", ("el", "greek")),
                           ("de", ("de-DE", "qwertz+")), ("fr", ("fr", "azerty"))]:
            t.space_swipe(1)
            t.g.__init__(load_rows(rows), swipe=True)
            t.png(f"many_{name}")
            row0 = t.g.rows[0]
            t.tap_at(*t.g.pos(row0[1])).tap_at(*t.g.pos(row0[2]))
            t.key("^").tap_at(*t.g.pos(t.g.rows[1][0]))
            t.longpress(row0[1], 1).longpress(t.g.rows[1][1], 1)
            t.tap(" ")
        x, y = t.g.pos(t.g.rows[2][-1])
        t.raw(f"down {x} {y}", "up")
        t.keymap = True
    sc("many", "en,fi,ru,el,de-DE,fr", ("en", "fi"), many)

    def small(t):
        t.tap("hi").longpress("o", 1, png="small_lp").key("^").png("small")
    sc("small", "en,fi", ("en", "fi"), small, w=1366, h=420)

    def portrait(t):
        t.tap("hi").png("portrait")
    sc("portrait", "en,fi", ("en", "fi"), portrait, w=1440, h=700)

    def plainfont(t):
        t.tap("a").png("plain")
    sc("plainfont", "en,fi", ("en", "fi"), plainfont, extra=["--fn", "Sans 20", "-R", "10"])

    return s


def run(binary, args, script, outdir, env):
    os.makedirs(outdir, exist_ok=True)
    text = "\n".join(script) + "\nkeymap keymap.xkb\n"
    p = subprocess.run([binary] + args, input=text, capture_output=True, text=True, env=env,
                       timeout=120, cwd=outdir)
    return p.stdout, p.stderr


def pixels_differ(a, b):
    p = subprocess.run(["magick", "compare", "-metric", "AE", a, b, "null:"], capture_output=True, text=True)
    out = (p.stderr or p.stdout).strip().split()[0]
    try:
        return int(float(out))
    except ValueError:
        return -1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--c", default="/tmp/osk-ref")
    ap.add_argument("--rust", default=os.path.join(ROOT, "rust", "target", "release", "osk-trace"))
    ap.add_argument("--keep", action="store_true")
    ap.add_argument("only", nargs="*")
    a = ap.parse_args()

    base = tempfile.mkdtemp(prefix="osk-compare-")
    failures = 0
    for name, (langs, dicts, build, extra, w, h, rows_of) in scenarios().items():
        if a.only and name not in a.only:
            continue
        t = Script(Geo(load_rows(rows_of), w, h, swipe=bool(dicts)))
        build(t)
        args = ["-W", str(w), "-H", str(h), "--fn", FONT, "-R", "14", *THEME,
                "--data", os.path.join(ROOT, "data")]
        if langs:
            args += ["--langs", langs]
        if dicts:
            args += ["--dict", dict_paths(*dicts)]
        args += extra
        logs = {}
        for side, binary in (("c", a.c), ("rust", a.rust)):
            d = os.path.join(base, name, side)
            env = dict(os.environ, XDG_RUNTIME_DIR=os.path.join(d, "run"),
                       XDG_STATE_HOME=os.path.join(d, "state"))
            os.makedirs(env["XDG_RUNTIME_DIR"], exist_ok=True)
            logs[side] = run(binary, args, t.lines, d, env)
        problems = []
        cl, rl = logs["c"][0].splitlines(), logs["rust"][0].splitlines()
        if cl != rl:
            for i, (x, y) in enumerate(zip(cl + [""] * len(rl), rl + [""] * len(cl))):
                if x != y:
                    problems.append(f"log line {i + 1}:\n      C:    {x}\n      Rust: {y}")
                    break
        cd, rd = os.path.join(base, name, "c"), os.path.join(base, name, "rust")
        ck, rk = (open(os.path.join(d, "keymap.xkb")).read() for d in (cd, rd))
        if ck != rk:
            problems.append("keymaps differ")
        pngs = sorted(f for f in os.listdir(cd) if f.endswith(".png"))
        for f in pngs:
            if not os.path.exists(os.path.join(rd, f)):
                problems.append(f"{f} missing")
                continue
            n = pixels_differ(os.path.join(cd, f), os.path.join(rd, f))
            if n != 0:
                problems.append(f"{f}: {n} pixels differ")
        steps = sum(1 for l in cl if l.startswith("> "))
        events = sum(1 for l in cl if l.startswith(("key ", "mods ")))
        if problems:
            failures += 1
            print(f"FAIL  {name} ({steps} steps, {events} events, {len(pngs)} images)")
            for p in problems[:8]:
                print("    " + p)
        else:
            print(f"PASS  {name} ({steps} steps, {events} events, {len(pngs)} images)")
    if a.keep or failures:
        print(f"output in {base}")
    else:
        shutil.rmtree(base)
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
