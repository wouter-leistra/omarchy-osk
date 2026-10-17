#!/usr/bin/python3 -I
"""Record a ~20 s demo of omarchy-osk for sharing.

Opens a foot window with nvim, records the screen with gpu-screen-recorder
(no cursor) and plays human-paced taps and swipes through the virtual
pointer. Nobody may use the machine while this runs; every step checks that
the demo window still has focus and aborts otherwise.

usage: tests/demo.py OUT.mp4
"""

import json
import math
import os
import random
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import keys  # noqa: E402

VPTR = "/tmp/vptr"
TITLE = "osk-demo"
U = keys.W / 11
SPACE_Y = keys.Y0 + keys.SUG + 3 * keys.ROW + keys.ROW / 2
STATE = os.path.expanduser("~/.local/state/omarchy-osk/language")
random.seed(7)


class Abort(Exception):
    pass


def hypr(what):
    return json.loads(subprocess.run(["hyprctl", what, "-j"], capture_output=True, text=True).stdout)


def guard():
    if hypr("activewindow").get("title") != TITLE:
        raise Abort("demo window lost focus")


def vptr(lines):
    guard()
    subprocess.run([VPTR], input="\n".join(lines) + "\n", text=True)


def focus_workspace(n):
    subprocess.run(["hyprctl", "dispatch", f'hl.dsp.focus({{ workspace = "{n}" }})'],
                   capture_output=True)


def osk(sig):
    subprocess.run(["pkill", f"-{sig}", "-x", "omarchy-oskd"])


def spline(pts, speed=1400.0, hz=45.0):
    """Catmull-Rom path through pts, sampled at a constant finger speed."""
    pts = [pts[0]] + pts + [pts[-1]]
    dense = []
    for i in range(1, len(pts) - 2):
        p0, p1, p2, p3 = pts[i - 1], pts[i], pts[i + 1], pts[i + 2]
        for k in range(20):
            t = k / 20
            dense.append(tuple(0.5 * (2 * p1[j] + (-p0[j] + p2[j]) * t
                                      + (2 * p0[j] - 5 * p1[j] + 4 * p2[j] - p3[j]) * t * t
                                      + (-p0[j] + 3 * p1[j] - 3 * p2[j] + p3[j]) * t ** 3)
                               for j in range(2)))
    dense.append(pts[-2])
    step = speed / hz
    out, acc = [dense[0]], 0.0
    for a, b in zip(dense, dense[1:]):
        acc += math.dist(a, b)
        if acc >= step:
            out.append(b)
            acc = 0.0
    if out[-1] != dense[-1]:
        out.append(dense[-1])
    return out


def jitter(p, r=10):
    return p[0] + random.uniform(-r, r), p[1] + random.uniform(-r, r)


def swipe(word, pause=0.25):
    pts = [jitter(keys.pos(c)) for c in word]
    path = spline(pts)
    lines = [f"move {path[0][0]:.0f} {path[0][1]:.0f}", "down"]
    lines += [f"move {x:.0f} {y:.0f}" for x, y in path[1:]]
    lines += ["sleep 30", "up"]
    vptr(lines)
    time.sleep(pause)


def tap(text, gap=110):
    lines = []
    for c in text:
        x, y = jitter(keys.pos(c), 6)
        lines += [f"move {x:.0f} {y:.0f}", "down", "sleep 60", "up", f"sleep {gap + random.randint(-20, 40)}"]
    vptr(lines)


def hold_slide(c, dx, ms=650):
    x, y = keys.pos(c)
    vptr([f"move {x:.0f} {y:.0f}", "down", f"sleep {ms}", f"to {x + dx:.0f} {y:.0f}",
          "sleep 250", "up", "sleep 200"])


def space_swipe():
    path = spline([(4.2 * U, SPACE_Y), (6.8 * U, SPACE_Y + 4)], speed=1000)
    vptr([f"move {path[0][0]:.0f} {path[0][1]:.0f}", "down"]
         + [f"move {x:.0f} {y:.0f}" for x, y in path[1:]] + ["up"])
    time.sleep(0.6)


def main():
    out = os.path.abspath(sys.argv[1])
    saved_lang = open(STATE).read() if os.path.exists(STATE) else None
    with open(STATE, "w") as fh:
        fh.write("en\n")
    subprocess.run([os.path.expanduser("~/.local/bin/omarchy-osk-restart")])
    time.sleep(1.5)
    osk("USR1")

    # a fresh workspace with nothing but the demo window
    prev_ws = hypr("activeworkspace")["id"]
    focus_workspace(3)
    if hypr("activeworkspace")["id"] != 3:
        sys.exit("could not switch to workspace 3")
    time.sleep(0.4)
    font = subprocess.run(["omarchy-font-current"], capture_output=True, text=True).stdout.strip()
    term = subprocess.Popen(["foot", "-T", TITLE, "-o", f"font={font}:size=20",
                             "nvim", "--clean", "-c", "set wrap linebreak nonu noru noshowmode laststatus=0 shortmess+=I",
                             "-c", "startinsert"],
                            start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    rec = None
    try:
        for _ in range(50):
            if hypr("activewindow").get("title") == TITLE:
                break
            time.sleep(0.1)
        time.sleep(1.0)
        guard()
        rec = subprocess.Popen(["gpu-screen-recorder", "-w", "eDP-1", "-f", "60", "-fm", "cfr",
                                "-cursor", "no", "-k", "auto", "-fallback-cpu-encoding", "yes",
                                "-q", "very_high", "-o", out],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(1.2)

        osk("USR2")                       # slides in, the window shrinks
        time.sleep(1.0)
        tap("^")
        for w in ["hello", "world"]:
            swipe(w)
        tap(".")
        time.sleep(0.2)
        for w in ["this", "is", "my", "new", "keyboard", "for"]:
            swipe(w)
        tap("omarchy")
        time.sleep(0.2)
        hold_slide(".", U)                # long-press . for !
        tap(" ")
        time.sleep(0.3)
        space_swipe()                     # English -> Finnish
        for w in ["moi", "kaikki"]:
            swipe(w)
        time.sleep(0.4)
        space_swipe()                     # back to English
        time.sleep(0.4)
        x, y = 10.5 * U, SPACE_Y          # hide key
        vptr([f"move {x:.0f} {y:.0f}", "down", "sleep 60", "up"])
        time.sleep(1.5)
    except Abort as e:
        print(f"ABORTED: {e}", flush=True)
    finally:
        if rec:
            rec.send_signal(signal.SIGINT)
            rec.wait(timeout=15)
        os.killpg(term.pid, signal.SIGTERM)
        focus_workspace(prev_ws)
        if saved_lang is None:
            os.remove(STATE)
        else:
            with open(STATE, "w") as fh:
                fh.write(saved_lang)
        subprocess.run([os.path.expanduser("~/.local/bin/omarchy-osk-restart")])
    print(out)


if __name__ == "__main__":
    main()
