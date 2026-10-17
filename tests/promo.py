#!/usr/bin/python3 -I
"""Record the promo video: auto-show, typing, swiping, symbols, Super+Ctrl+Shift+Space
into the Omarchy theme switcher, theme following and the bar toggle.

Runs on workspace 3 with throwaway windows. Nobody may use the machine while
this runs: text goes in only while the demo window (or an Omarchy menu opened
by the demo) has focus. Writes OUT.mp4 and OUT.json (scene start times, in
seconds from the start of the recording). The theme is restored afterwards.

usage: tests/promo.py OUT.mp4
"""

import json
import os
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import keys  # noqa: E402
from demo import hypr, focus_workspace, osk, spline, jitter, VPTR  # noqa: E402

TITLE = "osk-demo"
U = keys.W / 11
STATE = os.path.expanduser("~/.local/state/omarchy-osk/language")
BAR_ICON = (1945, 23)
NEXT_CARD = (1504, 600)       # first slice right of the selected card
SELECTED_CARD = (1080, 600)


class Abort(Exception):
    pass


def row_y(r):
    return keys.Y0 + keys.SUG + r * keys.ROW + keys.ROW / 2


def layer_open(ns):
    return any(l.get("namespace") == ns for mon in hypr("layers").values()
               for level in mon["levels"].values() for l in level)


def keyboard_visible():
    return layer_open("omarchy-osk")


def wait(pred, timeout=4.0):
    end = time.time() + timeout
    while time.time() < end:
        if pred():
            return True
        time.sleep(0.05)
    return pred()


# where typed text may go: the demo window, or a menu the demo opened
target = {"layer": None}


def guard():
    if hypr("activeworkspace")["id"] != 3:
        raise Abort("left workspace 3")
    if target["layer"]:
        if not layer_open(target["layer"]):
            raise Abort(f"{target['layer']} is not open")
    elif hypr("activewindow").get("title") != TITLE:
        raise Abort("demo window lost focus")


def vptr(lines, check=True):
    if check:
        guard()
        if not keyboard_visible():
            osk("USR2")
            wait(keyboard_visible, 1.5)
            time.sleep(0.3)
    subprocess.run([VPTR], input="\n".join(lines) + "\n", text=True)


def tap_at(x, y, after=120):
    return [f"move {x:.0f} {y:.0f}", "down", "sleep 50", "up", f"sleep {after}"]


def tap(text, gap=100):
    lines = []
    for c in text:
        x, y = jitter(keys.pos(c), 6)
        lines += tap_at(x, y, gap)
    vptr(lines)


def swipe(word, pause=0.15, speed=1700):
    path = spline([jitter(keys.pos(c)) for c in word], speed=speed)
    vptr([f"move {path[0][0]:.0f} {path[0][1]:.0f}", "down"]
         + [f"move {x:.0f} {y:.0f}" for x, y in path[1:]] + ["sleep 20", "up"])
    time.sleep(pause)


def hold_slide(c, dx, ms=550):
    x, y = keys.pos(c)
    vptr([f"move {x:.0f} {y:.0f}", "down", f"sleep {ms}", f"to {x + dx:.0f} {y:.0f}",
          "sleep 200", "up", "sleep 150"])


def click_window(title):
    c = next(c for c in hypr("clients") if c["title"] == title)
    # top-left corner: floating windows (imv) sit in the middle
    x, y = c["at"][0] + 120, c["at"][1] + 120
    vptr([f"move {x} {y}", "down", "sleep 40", "up", "sleep 100"], check=False)


SYM = tap_at(0.75 * U, row_y(3))     # ?123 / ABC
SYM2 = tap_at(1.0 * U, row_y(2))     # =\< from ?123
CTRL = tap_at(0.5 * U, row_y(1))
SUPER = tap_at(2.5 * U, row_y(1))
SPACE = tap_at(5.0 * U, row_y(3))
ENTER = tap_at(9.25 * U, row_y(3))
PAREN = tap_at(9.5 * U, row_y(1))    # ) on ?123


def main():
    out = os.path.abspath(sys.argv[1])
    marks, t0 = {}, None

    def mark(name):
        marks[name] = round(time.time() - t0, 2)
        print(f"{marks[name]:6.2f} {name}", flush=True)

    theme = subprocess.run(["omarchy", "theme", "current"], capture_output=True, text=True).stdout.strip()
    bg = os.path.realpath(os.path.expanduser("~/.local/state/omarchy/current/background"))
    focus_on_activate = json.loads(subprocess.run(["hyprctl", "getoption", "misc:focus_on_activate", "-j"],
                                                  capture_output=True, text=True).stdout)["bool"]
    subprocess.run(["hyprctl", "eval", "hl.config({ misc = { focus_on_activate = false } })"],
                   capture_output=True)
    saved_lang = open(STATE).read() if os.path.exists(STATE) else None
    with open(STATE, "w") as fh:
        fh.write("en\n")

    # auto-show service that believes the Type Cover is detached
    subprocess.run(["systemctl", "--user", "stop", "omarchy-osk-autoshow"])
    env = dict(os.environ, OSK_COVER_NAME="no-such-cover-for-demo", OSK_DEBUG="1")
    daemon = subprocess.Popen([os.path.expanduser("~/.local/bin/omarchy-osk-autoshow")], env=env,
                              stdout=open("/tmp/osk-promo-autoshow.log", "w"), stderr=subprocess.STDOUT)
    subprocess.run(["pkill", "-x", "omarchy-oskd"])
    time.sleep(0.3)
    subprocess.Popen(["omarchy-osk"], start_new_session=True,
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(3.0)

    prev_ws = hypr("activeworkspace")["id"]
    focus_workspace(3)
    if hypr("activeworkspace")["id"] != 3:
        sys.exit("could not switch to workspace 3")
    procs, rec = [], None
    try:
        pics = "/usr/share/omarchy/themes/catppuccin/backgrounds"
        pic = os.path.join(pics, sorted(os.listdir(pics))[-1])
        procs.append(subprocess.Popen(["imv", pic], start_new_session=True,
                                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        wait(lambda: hypr("activewindow").get("class") == "imv", 5)
        osk("USR1")
        wait(lambda: not keyboard_visible(), 2)
        time.sleep(0.8)

        t0 = time.time()
        rec = subprocess.Popen(["gpu-screen-recorder", "-w", "eDP-1", "-f", "60", "-fm", "cfr",
                                "-cursor", "no", "-k", "auto", "-fallback-cpu-encoding", "yes",
                                "-q", "very_high", "-o", out],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(1.5)

        # 1. a text field gets focus: the keyboard appears by itself
        mark("autoshow")
        font = subprocess.run(["omarchy-font-current"], capture_output=True, text=True).stdout.strip()
        procs.append(subprocess.Popen(
            ["foot", "-T", TITLE, "-o", f"font={font}:size=30", "nvim", "--clean",
             "-c", "set wrap linebreak nonu noru noshowmode laststatus=0 shortmess+=I mouse=",
             "-c", "startinsert"],
            start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        wait(lambda: hypr("activewindow").get("title") == TITLE, 5)
        if not wait(keyboard_visible, 3):
            print("auto-show did not trigger", flush=True)
        time.sleep(1.2)
        mark("autohide")
        click_window(next(c["title"] for c in hypr("clients")
                          if c["class"] == "imv" and c["workspace"]["id"] == 3))
        if not wait(lambda: not keyboard_visible(), 2):
            print("auto-hide did not trigger", flush=True)
        time.sleep(0.9)
        click_window(TITLE)
        wait(keyboard_visible, 2)
        time.sleep(0.7)

        # 2. tap and swipe
        mark("typing")
        tap("^")
        for w in ["hello", "from", "my", "^surface", "^pro"]:
            if w[0] == "^":
                tap("^")
                w = w[1:]
            swipe(w)
        hold_slide("e", 0, ms=500)    # long-press e: 3
        hold_slide(".", U)            # long-press .: !
        tap(" ")
        mark("symbols")
        hold_slide(",", 0, ms=500)    # long-press ,: colon
        vptr(SYM + PAREN + SYM)
        tap(" ")
        time.sleep(0.4)

        # 3. Super+Ctrl+Shift+Space from the keyboard opens the theme switcher
        mark("super")
        # Shift, then Ctrl and Super on the extra keys page: Super+Ctrl+Shift+Space
        vptr(tap_at(*keys.pos("^"), 200) + SYM + SYM2 + CTRL + SUPER)
        time.sleep(0.5)
        vptr(SPACE)
        if not wait(lambda: layer_open("omarchy-image-selector"), 4):
            raise Abort("theme switcher did not open")
        # it takes all pointer input while open, so only tap the cards
        target["layer"] = "omarchy-image-selector"
        mark("switcher")
        time.sleep(1.0)
        for _ in range(2):
            vptr(tap_at(*NEXT_CARD))
            time.sleep(0.9)
        vptr(tap_at(*SELECTED_CARD))
        mark("theme")
        target["layer"] = None
        wait(lambda: subprocess.run(["omarchy", "theme", "current"], capture_output=True,
                                    text=True).stdout.strip() != theme, 15)
        wait(lambda: not layer_open("omarchy-image-selector"), 5)
        time.sleep(3.0)               # hooks restyle the keyboard
        wait(lambda: hypr("activewindow").get("title") == TITLE, 3)
        if hypr("activewindow").get("title") != TITLE:
            click_window(TITLE)
            time.sleep(0.5)

        # 4. keep typing in the new colours
        mark("follows")
        for w in ["and", "it", "follows", "my", "theme"]:
            swipe(w)
        tap(".")
        time.sleep(0.8)

        # 5. toggle from the bar
        mark("toggle")
        vptr(tap_at(*BAR_ICON, 100), check=False)
        if not wait(lambda: not keyboard_visible(), 2):
            print("bar toggle did not hide", flush=True)
        time.sleep(1.0)
        vptr(tap_at(*BAR_ICON, 100), check=False)
        wait(keyboard_visible, 2)
        time.sleep(1.2)
        mark("end")
        time.sleep(2.5)
    except Abort as e:
        print(f"ABORTED: {e}", flush=True)
    finally:
        if rec:
            rec.send_signal(signal.SIGINT)
            rec.wait(timeout=30)
        for p in procs:
            try:
                os.killpg(p.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        if subprocess.run(["omarchy", "theme", "current"], capture_output=True,
                          text=True).stdout.strip() != theme:
            subprocess.run(["omarchy", "theme", "set", theme.lower().replace(" ", "-")], capture_output=True)
        if os.path.realpath(os.path.expanduser("~/.local/state/omarchy/current/background")) != bg \
                and os.path.exists(bg):
            subprocess.run(["omarchy", "theme", "bg", "set", bg], capture_output=True)
        focus_workspace(prev_ws)
        subprocess.run(["hyprctl", "eval", "hl.config({ misc = { focus_on_activate = %s } })"
                        % ("true" if focus_on_activate else "false")], capture_output=True)
        daemon.terminate()
        subprocess.run(["systemctl", "--user", "start", "omarchy-osk-autoshow"])
        if saved_lang is None:
            os.remove(STATE)
        else:
            with open(STATE, "w") as fh:
                fh.write(saved_lang)
        subprocess.run([os.path.expanduser("~/.local/bin/omarchy-osk-restart")])
        with open(os.path.splitext(out)[0] + ".json", "w") as fh:
            json.dump(marks, fh)
    print(out)


if __name__ == "__main__":
    main()
