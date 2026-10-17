#!/usr/bin/python3 -I
"""Differential test of the gesture decoder and completions: the C
swipe.c (tests/ref/decode_ref.c) against the Rust port (osk-decode), on
many sloppy swipes and prefixes. Both must give identical top-3 lists.

usage: tests/ref/decode_compare.py [--words N] [--c /tmp/decode-ref] [--rust PATH]
"""

import argparse
import os
import random
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)
from compare import Geo, load_rows  # noqa: E402


def words_of(lang, n, rnd):
    words = [l.split()[0] for l in open(os.path.join(ROOT, "dict", f"{lang}.txt"), encoding="utf-8")][:6000]
    return rnd.sample(words, n)


def path_for(word, geo, rnd, sloppy):
    pts = []
    for c in word:
        try:
            x, y = geo.pos(c)
        except ValueError:
            continue
        x += rnd.uniform(-sloppy, sloppy) * geo.u
        y += rnd.uniform(-sloppy, sloppy) * geo.row_h
        if pts:
            px, py = pts[-1]
            steps = rnd.randint(3, 16)
            bend = rnd.uniform(-0.3, 0.3) * geo.u
            for s in range(1, steps + 1):
                t = s / steps
                pts.append((px + (x - px) * t + bend * t * (1 - t) * 4, py + (y - py) * t))
        else:
            pts.append((x, y))
    return " ".join(f"{x:.3f},{y:.3f}" for x, y in pts)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--words", type=int, default=300)
    ap.add_argument("--c", default="/tmp/decode-ref")
    ap.add_argument("--rust", default=os.path.join(ROOT, "rust", "target", "release", "osk-decode"))
    a = ap.parse_args()
    a.c, a.rust = os.path.abspath(a.c), os.path.abspath(a.rust)
    rnd = random.Random(20261017)
    dicts = ",".join(os.path.join(ROOT, "dict", f"{l}.txt") for l in ("en", "fi", "sv"))
    total, failures = 0, 0
    for lang, layout in (("en", ("en", "qwerty")), ("fi", ("fi", "qwerty+")), ("sv", ("sv", "qwerty+"))):
        geo = Geo(load_rows(layout))
        with tempfile.NamedTemporaryFile("w", suffix=".geo", delete=False) as g:
            g.write(f"{geo.u} {geo.row_h}\n")
            for row in geo.rows:
                for c in row:
                    x, y = geo.pos(c)
                    g.write(f"{ord(c)} {x} {y}\n")
        lines = []
        for mode in ("ALL", lang):
            if mode != "ALL":
                lines.append(f"LANG {mode}")
            for w in words_of(lang, a.words, rnd):
                lines.append(path_for(w, geo, rnd, rnd.choice((0.0, 0.2, 0.35, 0.5))))
            for w in words_of(lang, a.words // 3, rnd):
                lines.append("?" + w[: rnd.randint(1, 3)])
        text = "\n".join(lines) + "\n"
        outs = []
        for binary in (a.c, a.rust):
            p = subprocess.run([binary, dicts, g.name], input=text, capture_output=True, text=True, timeout=600)
            outs.append(p.stdout.splitlines())
        os.unlink(g.name)
        diffs = [(i, x, y) for i, (x, y) in enumerate(zip(*outs)) if x != y]
        if len(outs[0]) != len(outs[1]):
            diffs.append((-1, len(outs[0]), len(outs[1])))
        found = sum(1 for l in outs[0] if not l.startswith(("lang", "0")))
        total += len(lines)
        if diffs:
            failures += 1
            print(f"FAIL  {lang}: {len(diffs)} of {len(lines)} differ, e.g. line {diffs[0][0]}:"
                  f"\n    C:    {diffs[0][1]}\n    Rust: {diffs[0][2]}")
        else:
            print(f"PASS  {lang}: {len(lines)} swipes and prefixes, {found} with results, identical")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
