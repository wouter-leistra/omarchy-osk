#!/usr/bin/python3 -I
"""Live comparison of two keyboard daemons on screen: the same states are
shown with each binary (installed in turn as ~/.local/bin/omarchy-oskd) and
the screenshots of the keyboard area must be identical.

usage: tests/ref/live_compare.py C_BINARY RUST_BINARY
Like tests/ui_test.py nobody may use the machine meanwhile; input goes to a
throwaway foot window. The binary installed at the start is put back.
"""

import os
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
import keys  # noqa: E402
import ui_test as ui  # noqa: E402

BIN = os.path.expanduser("~/.local/bin/omarchy-oskd")
OUT = "/tmp/osk-live"
REGION = "0,780 2160x660"  # keyboard plus the popup row above it


def put(src, dst):
    """replace a binary that may be running"""
    tmp = dst + ".new"
    shutil.copy2(src, tmp)
    os.replace(tmp, dst)


def shot(name):
    subprocess.run(["grim", "-g", REGION, f"{OUT}/{name}.png"], check=True)


def hold(c, then_ms=1500, slide=0):
    x, y = keys.pos(c)
    lines = [f"move {x:.0f} {y:.0f}", "down", "sleep 700"]
    if slide:
        lines += [f"to {x + slide:.0f} {y:.0f}", "sleep 100"]
    lines += [f"sleep {then_ms}", "up", "sleep 150"]
    return subprocess.Popen([ui.VPTR], stdin=subprocess.PIPE, text=True), "\n".join(lines) + "\n"


def states(tag):
    """drive one daemon through the states, screenshot each"""
    f = ui.Field(f"osk-live-{tag}")
    try:
        f.focus()
        ui.osk("USR2")
        ui.wait(ui.keyboard_visible)
        time.sleep(1.0)
        shot(f"{tag}-idle")
        f.send("tap:hel")
        time.sleep(0.5)
        shot(f"{tag}-typed")
        p, script = hold("e", slide=keys.key_width())
        p.stdin.write(script)
        p.stdin.flush()
        time.sleep(1.2)
        shot(f"{tag}-longpress")
        p.stdin.close()
        p.wait()
        time.sleep(0.4)
        f.send("tap:^")
        time.sleep(0.4)
        shot(f"{tag}-shift")
        f.send("tap:a", ui.QWERTY123)
        time.sleep(0.5)
        shot(f"{tag}-symbols")
        f.send(ui.QWERTY123, ui.SPACE_SWIPE, "lang:fi")
        time.sleep(0.6)
        shot(f"{tag}-finnish")
        f.send(ui.SPACE_SWIPE, "lang:en")
        os.kill(int(subprocess.run(["pgrep", "-x", "omarchy-oskd"], capture_output=True, text=True).stdout.split()[0]),
                ui.signal.SIGRTMIN + 2)
        time.sleep(0.5)
        shot(f"{tag}-private")
        os.kill(int(subprocess.run(["pgrep", "-x", "omarchy-oskd"], capture_output=True, text=True).stdout.split()[0]),
                ui.signal.SIGRTMIN + 3)
        time.sleep(0.3)
        return f.result()
    finally:
        f.close()
        time.sleep(0.3)


def main():
    c_bin, rust_bin = sys.argv[1], sys.argv[2]
    os.makedirs(OUT, exist_ok=True)
    keep = BIN + ".live-compare"
    shutil.copy2(BIN, keep)
    saved = open(ui.STATE).read() if os.path.exists(ui.STATE) else None
    texts = {}
    try:
        for tag, binary in (("c", c_bin), ("rust", rust_bin)):
            with open(ui.STATE, "w") as fh:
                fh.write("en\n")
            put(binary, BIN)
            subprocess.run([os.path.expanduser("~/.local/bin/omarchy-osk-restart")])
            time.sleep(2.0)
            texts[tag] = states(tag)
    finally:
        put(keep, BIN)
        os.remove(keep)
        if saved is not None:
            with open(ui.STATE, "w") as fh:
                fh.write(saved)
        subprocess.run([os.path.expanduser("~/.local/bin/omarchy-osk-restart")])
    fails = 0
    print(f"typed text: C {texts['c']!r}, Rust {texts['rust']!r}")
    if texts["c"] != texts["rust"]:
        fails += 1
    for name in ("idle", "typed", "longpress", "shift", "symbols", "finnish", "private"):
        a, b = f"{OUT}/c-{name}.png", f"{OUT}/rust-{name}.png"
        p = subprocess.run(["magick", "compare", "-metric", "AE", a, b, f"{OUT}/diff-{name}.png"],
                           capture_output=True, text=True)
        n = (p.stderr or p.stdout).split()[0]
        ok = n == "0"
        fails += not ok
        print(f"{'PASS' if ok else 'FAIL'}  {name}: {n} pixels differ")
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()
