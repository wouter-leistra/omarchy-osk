#!/usr/bin/python3 -I
"""Record the promo video: the one-line install and its setup (adding
Swedish), setup from the Omarchy menu, auto-show, typing, swiping, symbols,
switching language, Super+Ctrl+Shift+Space into the Omarchy theme switcher,
theme following and the bar toggle.

Runs on an empty workspace (4) with throwaway windows. Nobody may use the
machine while this runs: keys and text go in only while the demo window (or
an Omarchy menu opened by the demo) has focus. Writes OUT.mp4 and OUT.json
(scene start times, in seconds from the start of the recording). Your
omarchy-osk settings, theme, background and do-not-disturb are restored
afterwards; the install itself is the real one (the latest release).

The recorder encodes on the CPU here (no VAAPI encoder for this
resolution), so it records 30 fps with real timestamps (MKV): at 60 fps it
drops frames and the scene times drift from the video.

usage: tests/promo.py OUT.mkv
"""

import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import keys  # noqa: E402
from demo import hypr, focus_workspace, osk, spline, jitter, VPTR  # noqa: E402

TITLE = "osk-demo"
WS = 4
INSTALL_TITLE = "osk-install"
INSTALL = "curl -fsSL https://github.com/wouter-leistra/omarchy-osk/releases/latest/download/install.sh | bash"
TERM_LOG = "/tmp/osk-promo-term.log"
CONF = os.path.expanduser("~/.config/omarchy-osk/config")
SETUP_STATE = os.path.expanduser("~/.local/state/omarchy-osk/setup")
SWEDISH_DOWNS = 87       # sv Svenska is 88th in setup's language list
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
    if hypr("activeworkspace")["id"] != WS:
        raise Abort(f"left workspace {WS}")
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
SPACE_SWIPE = [f"move {4.5 * U:.0f} {row_y(3):.0f}", "down", f"to {6.5 * U:.0f} {row_y(3):.0f}",
               "up", "sleep 200"]


def active_title():
    return hypr("activewindow").get("title")


def keys_to(title, *args):
    """wtype into the window titled title, never anywhere else"""
    if hypr("activeworkspace")["id"] != WS or active_title() != title:
        raise Abort(f"{title} lost focus")
    subprocess.run(["wtype", *args])


def menu_keys(*args):
    if not layer_open("omarchy-menu"):
        raise Abort("the Omarchy menu is not open")
    subprocess.run(["wtype", *args])


def plain(text):
    return re.sub(r"\x1b(\[[0-9;?<=>]*[ -/]*[@-~]|\][^\x07]*\x07|[@-_])", "", text)


def install_scene(mark, procs):
    """the README's install line in a terminal; setup adds Swedish"""
    font = subprocess.run(["omarchy-font-current"], capture_output=True, text=True).stdout.strip()
    script = "/tmp/osk-promo-term.sh"
    with open(script, "w") as fh:
        fh.write('clear\nprintf "\\n  \\e[1;32m❯\\e[0m "\nIFS= read -r line\neval "$line"\nsleep 600\n')
    open(TERM_LOG, "w").close()
    procs.append(subprocess.Popen(
        ["foot", "-T", INSTALL_TITLE, "-o", f"font={font}:size=25",
         "script", "-qfc", f"bash {script}", TERM_LOG],
        start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
    if not wait(lambda: active_title() == INSTALL_TITLE, 5):
        raise Abort("the install terminal did not open")
    time.sleep(1.2)

    log = open(TERM_LOG, errors="ignore")
    seen = {"all": "", "since": ""}

    def read():
        new = log.read()
        seen["all"] += new
        seen["since"] += new
        return new

    def until(pred, timeout):
        end, quiet = time.time() + timeout, time.time()
        while time.time() < end:
            if read():
                quiet = time.time()
            elif pred() and time.time() - quiet > 0.5:
                return True
            time.sleep(0.1)
        raise Abort(f"setup stalled: {plain(seen['all'])[-300:]!r}")

    mark("install")
    keys_to(INSTALL_TITLE, "-d", "28", INSTALL)
    time.sleep(0.6)
    keys_to(INSTALL_TITLE, "-k", "Return")
    mark("download")
    until(lambda: "Which languages" in plain(seen["all"]) and "enter" in plain(seen["since"]), 60)
    mark("languages")
    time.sleep(1.5)
    keys_to(INSTALL_TITLE, "-d", "22", *["-k", "Down"] * SWEDISH_DOWNS)
    time.sleep(0.7)
    keys_to(INSTALL_TITLE, "x")
    time.sleep(1.1)
    seen["since"] = ""
    keys_to(INSTALL_TITLE, "-k", "Return")
    mark("questions")
    # the other questions: keep the current answers
    while "omarchy-osk: done" not in plain(seen["all"]):
        until(lambda: "enter" in plain(seen["since"]) or "omarchy-osk: done" in plain(seen["all"]), 60)
        if "omarchy-osk: done" in plain(seen["all"]):
            break
        time.sleep(0.8)
        seen["since"] = ""
        keys_to(INSTALL_TITLE, "-k", "Return")
    mark("installed")
    langs = open(CONF).read()
    if 'OSK_LANGS="en,fi,sv"' not in langs:
        raise Abort(f"setup did not add Swedish: {langs!r}")
    time.sleep(2.5)
    term = next(c for c in hypr("clients") if c["title"] == INSTALL_TITLE)
    os.kill(term["pid"], signal.SIGTERM)
    wait(lambda: not any(c["title"] == INSTALL_TITLE for c in hypr("clients")), 3)
    time.sleep(0.8)


def menu_scene(mark):
    """setup again, from Setup > On-screen keyboard in the Omarchy menu"""
    mark("menu")
    subprocess.run(["omarchy", "menu", "summon", "setup"], capture_output=True)
    if not wait(lambda: layer_open("omarchy-menu"), 3):
        raise Abort("the Omarchy menu did not open")
    time.sleep(1.4)
    menu_keys("-k", "Up")              # wraps round to the last row: On-screen keyboard
    time.sleep(1.0)
    menu_keys("-k", "Return")
    if not wait(lambda: any(c["title"] == "Omarchy" for c in hypr("clients")), 5):
        raise Abort("setup did not open from the menu")
    time.sleep(4.5)
    mark("menu_end")
    for c in hypr("clients"):
        if c["title"] == "Omarchy" and c["workspace"]["id"] == WS:
            os.kill(c["pid"], signal.SIGTERM)
    wait(lambda: not any(c["title"] == "Omarchy" for c in hypr("clients")), 3)


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
    saved = {}
    for f in (CONF, SETUP_STATE):
        if os.path.exists(f):
            shutil.copy(f, f + ".promo-backup")
            saved[f] = f + ".promo-backup"
    with open(STATE, "w") as fh:
        fh.write("en\n")
    dnd = subprocess.run(["omarchy-shell", "notifications", "dndState"], capture_output=True,
                         text=True).stdout.strip()
    subprocess.run(["omarchy-shell", "notifications", "setDnd", "on"], capture_output=True)
    if any(c["workspace"]["id"] == WS for c in hypr("clients")):
        sys.exit(f"workspace {WS} is not empty")
    osk("USR1")                       # hidden while installing

    prev_ws = hypr("activeworkspace")["id"]
    focus_workspace(WS)
    if hypr("activeworkspace")["id"] != WS:
        sys.exit(f"could not switch to workspace {WS}")
    procs, rec, daemon = [], None, None
    try:
        # the virtual pointer may still rest on the bar button (tooltip)
        subprocess.run([VPTR], input="move 1080 700\n", text=True)
        time.sleep(1.0)
        if os.path.exists(out):
            os.remove(out)
        rec = subprocess.Popen(["gpu-screen-recorder", "-w", "eDP-1", "-f", "30", "-fm", "vfr", "-c", "mkv",
                                "-cursor", "no", "-k", "auto", "-fallback-cpu-encoding", "yes",
                                "-q", "very_high", "-o", out],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        # the clock starts with the recording, which takes a moment to begin
        wait(lambda: os.path.exists(out) and os.path.getsize(out) > 0, 10)
        t0 = time.time()
        time.sleep(2.0)
        mark("start")

        # 1. setup from the menu (recorded first: setup reloads the patched
        # menu, which then doesn't draw for a while; the edit puts it after
        # the install), then the one-line install and its setup
        menu_scene(mark)
        time.sleep(1.0)
        install_scene(mark, procs)

        # auto-show service that believes the Type Cover is detached
        mark("prepare")
        subprocess.run(["systemctl", "--user", "stop", "omarchy-osk-autoshow"])
        env = dict(os.environ, OSK_COVER_NAME="no-such-cover-for-demo", OSK_DEBUG="1")
        daemon = subprocess.Popen([os.path.expanduser("~/.local/bin/omarchy-osk-autoshow")], env=env,
                                  stdout=open("/tmp/osk-promo-autoshow.log", "w"), stderr=subprocess.STDOUT)
        subprocess.run(["pkill", "-x", "omarchy-oskd"])
        time.sleep(0.3)
        subprocess.Popen(["omarchy-osk", "--hidden"], start_new_session=True,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(3.0)

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
        # a window without a text field (your wallpaper in an image viewer)
        # takes focus: the keyboard goes; it closes: the keyboard is back
        mark("autohide")
        viewer = subprocess.Popen(["imv", bg], start_new_session=True,
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        procs.append(viewer)
        wait(lambda: hypr("activewindow").get("class") == "imv", 5)
        if not wait(lambda: not keyboard_visible(), 2):
            print("auto-hide did not trigger", flush=True)
        time.sleep(1.2)
        os.killpg(viewer.pid, signal.SIGTERM)
        if not wait(lambda: hypr("activewindow").get("title") == TITLE, 3):
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

        # the language added in setup: swipe the space bar (English, Finnish, Swedish)
        mark("language")
        vptr(ENTER)
        vptr(SPACE_SWIPE)
        time.sleep(0.5)
        vptr(SPACE_SWIPE)
        keys.use("fi")                # Swedish has the same keys as Finnish
        time.sleep(0.7)
        tap("^")
        for w in ["hej", "från", "^sverige"]:
            if w[0] == "^":
                tap("^")
                w = w[1:]
            swipe(w)
        hold_slide(".", U)            # !
        time.sleep(0.6)
        vptr(SPACE_SWIPE)             # back to English
        keys.use("en")
        vptr(ENTER)
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
        time.sleep(6.0)               # hooks restyle and restart the keyboard
        wait(lambda: hypr("activewindow").get("title") == TITLE, 3)
        click_window(TITLE)           # tap the text field again, as you would
        if not wait(keyboard_visible, 3):
            print("keyboard not back after the theme change", flush=True)
        time.sleep(0.8)

        # 4. keep typing in the new colours
        mark("follows")
        for w in ["and", "it", "follows", "my", "theme"]:
            swipe(w)
        tap(".")
        time.sleep(0.8)

        # 5. toggle from the bar
        if not keyboard_visible():
            click_window(TITLE)
            wait(keyboard_visible, 3)
            time.sleep(0.8)
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
        if daemon:
            daemon.terminate()
        for f, b in saved.items():
            shutil.move(b, f)
        subprocess.run(["omarchy-shell", "notifications", "setDnd", dnd or "off"], capture_output=True)
        subprocess.run(["systemctl", "--user", "restart", "omarchy-osk-autoshow"])
        if saved_lang is None:
            os.remove(STATE)
        else:
            with open(STATE, "w") as fh:
                fh.write(saved_lang)
        subprocess.run([os.path.expanduser("~/.local/bin/omarchy-osk-restart")])
        # leave it hidden, as auto-show expects (the restart starts it in the background)
        wait(lambda: subprocess.run(["pgrep", "-x", "omarchy-oskd"],
                                    stdout=subprocess.DEVNULL).returncode == 0, 5)
        time.sleep(1.0)
        osk("USR1")
        with open(os.path.splitext(out)[0] + ".json", "w") as fh:
            json.dump(marks, fh)
    print(out)


if __name__ == "__main__":
    main()
