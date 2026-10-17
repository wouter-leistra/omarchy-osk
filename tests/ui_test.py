#!/usr/bin/python3 -I
"""End-to-end UI tests for omarchy-osk.

Drives a virtual pointer over the on-screen keyboard and checks what arrives
in throwaway foot windows. Nobody may use the machine while this runs: every
step first checks that the test window has focus and aborts otherwise, so
synthetic input never lands in a real application.

Needs: the keyboard installed (make install-user), tests/vptr built at
/tmp/vptr (cc -o /tmp/vptr tests/vptr.c tests/proto/vp.c ...), Hyprland.
The auto-show service is replaced by a copy that pretends the Type Cover is
detached, and restored afterwards.
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

VPTR = "/tmp/vptr"
OUT = "/tmp/osk-ui"
STATE = os.path.expanduser("~/.local/state/omarchy-osk/language")
U = keys.W / 11


class Abort(Exception):
    pass


def sh(*cmd):
    return subprocess.run(cmd, capture_output=True, text=True).stdout


def hypr(what):
    return json.loads(sh("hyprctl", what, "-j"))


def active_title():
    return hypr("activewindow").get("title", "")


def keyboard_visible():
    return any(l.get("namespace") == "omarchy-osk"
               for mon in hypr("layers").values()
               for level in mon["levels"].values() for l in level)


def osk(sig):
    subprocess.run(["pkill", f"-{sig}", "-x", "omarchy-oskd"])


def vptr(lines):
    subprocess.run([VPTR], input="\n".join(lines) + "\n", text=True)


def wait(pred, timeout=3.0):
    end = time.time() + timeout
    while time.time() < end:
        if pred():
            return True
        time.sleep(0.1)
    return pred()


class Field:
    """A foot window running a readline prompt that saves the line."""

    def __init__(self, name, preset=""):
        self.name, self.out = name, f"{OUT}/{name}.out"
        if os.path.exists(self.out):
            os.remove(self.out)
        subprocess.Popen(["foot", "-T", name, "bash", "-c",
                          'IFS= read -r -e -i "$1" l; printf "%s" "$l" > "$2"', "_", preset, self.out],
                         start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if not wait(lambda: self.client(), 5):
            raise Abort(f"{name}: window did not appear")
        time.sleep(0.8)  # let it settle at its tiled position

    def client(self):
        return next((c for c in hypr("clients") if c["title"] == self.name), None)

    def focus(self):
        for _ in range(3):
            c = self.client()
            if not c:
                raise Abort(f"{self.name}: window gone")
            x = c["at"][0] + c["size"][0] // 2
            y = c["at"][1] + 60
            vptr([f"move {x} {y}", "down", "up", "sleep 300"])
            if active_title() == self.name:
                return
            time.sleep(0.5)
        self.guard()

    def guard(self):
        if active_title() != self.name:
            wins = [(c["title"], c["at"], c["size"]) for c in hypr("clients")]
            raise Abort(f"focus is on '{active_title()}', not {self.name}; "
                        f"windows {wins}, cursor {sh('hyprctl', 'cursorpos').strip()}")

    def send(self, *chunks, closes=False):
        """chunks: keys.py tokens (str), lists of raw vptr lines or callables;
        closes: the last chunk ends the prompt, so the window goes away"""
        for chunk in chunks:
            self.guard()
            if not keyboard_visible():
                osk("USR2")
                wait(keyboard_visible)
                time.sleep(0.3)
            if callable(chunk):
                chunk()
            else:
                vptr(keys.commands([chunk]) if isinstance(chunk, str) else chunk)
            time.sleep(0.1)
        if not closes:
            self.guard()

    def result(self):
        self.send("tap:|", closes=True)
        wait(lambda: os.path.exists(self.out), 3)
        try:
            with open(self.out) as f:
                return f.read()
        except OSError:
            return None

    def close(self):
        c = self.client()
        if c:
            os.kill(c["pid"], signal.SIGTERM)


def tap_at(x, y):
    return [f"move {x:.0f} {y:.0f}", "down", "sleep 40", "up", "sleep 150"]


def hold_slide(x, y, x2, ms=500):
    return [f"move {x:.0f} {y:.0f}", "down", f"sleep {ms}", f"to {x2:.0f} {y:.0f}",
            "sleep 100", "up", "sleep 150"]


def row_y(r):
    return keys.Y0 + keys.SUG + r * keys.ROW + keys.ROW / 2


QWERTY123 = tap_at(0.75 * U, row_y(3))   # ?123 / ABC
SYM2 = tap_at(1.0 * U, row_y(2))         # =\< from ?123
SPACE_SWIPE = [f"move {4.5 * U:.0f} {row_y(3):.0f}", "down", f"to {6.5 * U:.0f} {row_y(3):.0f}",
               "up", "sleep 200"]


def best_completion(prefix, lang="en"):
    with open(os.path.join(HERE, "..", "dict", f"{lang}.txt")) as f:
        for line in f:
            w = line.split()[0]
            if w.startswith(prefix) and w != prefix:
                return w


results = []


def check(name, got, want):
    ok = got == want
    results.append((name, ok, got, want))
    print(f"{'PASS' if ok else 'FAIL'}  {name}: got {got!r}" + ("" if ok else f", want {want!r}"), flush=True)


def single(name, steps, want, preset=""):
    f = Field(name, preset)
    try:
        f.focus()
        f.send(*steps)
        check(name, f.result(), want)
    finally:
        f.close()
        time.sleep(0.3)


def main():
    os.makedirs(OUT, exist_ok=True)
    if not os.path.exists(VPTR):
        sys.exit("build tests/vptr.c to /tmp/vptr first")

    # apps asking for attention (focus_on_activate) must not steal focus
    # from the test windows mid-test
    focus_on_activate = json.loads(sh("hyprctl", "getoption", "misc:focus_on_activate", "-j"))["bool"]
    set_focus_on_activate(False)

    saved_lang = open(STATE).read() if os.path.exists(STATE) else None
    os.makedirs(os.path.dirname(STATE), exist_ok=True)
    with open(STATE, "w") as fh:
        fh.write("en\n")

    # auto-show service that believes the cover is detached
    subprocess.run(["systemctl", "--user", "stop", "omarchy-osk-autoshow"])
    env = dict(os.environ, OSK_COVER_NAME="no-such-cover-for-tests", OSK_DEBUG="1")
    daemon = subprocess.Popen([os.path.expanduser("~/.local/bin/omarchy-osk-autoshow")], env=env,
                              stdout=open(f"{OUT}/autoshow.log", "w"), stderr=subprocess.STDOUT)
    subprocess.run(["pkill", "-x", "omarchy-oskd"])
    time.sleep(0.3)
    subprocess.Popen(["omarchy-osk"], start_new_session=True,
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(2.5)

    try:
        single("tap", ["tap:hej_"], "hej ")
        single("swipe+punct", ["swipe:hello", "swipe:world", "tap:."], "hello world. ")
        single("double-space", ["tap:ok__"], "ok. ")
        best = best_completion("sugges")
        single("suggestion-backspace", ["tap:sugges", "sugg:1", "tap:<", "tap:x"], best + "x")
        single("swipe-backspace", ["swipe:hello", "tap:<", "tap:ok"], "ok")
        single("longpress", [hold_slide(*keys.pos("q"), keys.pos("q")[0]),
                             hold_slide(*keys.pos("e"), keys.pos("e")[0] + U),
                             hold_slide(*keys.pos("n"), keys.pos("n")[0]),
                             hold_slide(*keys.pos("."), keys.pos(".")[0])], "1éñ?")
        single("language", [SPACE_SWIPE, "lang:fi", "swipe:kiitos", SPACE_SWIPE, "lang:en"],
               "kiitos ")
        check("language-saved", open(STATE).read().strip(), "en")
        # each language has its own layout: å ä ö are keys in Finnish, and
        # long-press keys in English (a: á à â ã æ ä å ā)
        single("layouts", [SPACE_SWIPE, "lang:fi", "tap:åäö_", SPACE_SWIPE, "lang:en",
                           lambda: vptr(hold_slide(*keys.pos("a"),
                                                   keys.pos("a")[0] + 5 * keys.key_width()))],
               "åäö ä")
        single("cursor", ["tap:abc", hold_slide(6.5 * U, row_y(3), 6.5 * U - 2.4 * U * 5 / 14), "tap:x"],
               "axbc")
        single("symbols", [QWERTY123, tap_at(0.5 * U, row_y(1)), tap_at(2.5 * U, row_y(1)), QWERTY123],
               "@€")
        single("ctrl-oneshot", ["tap:bc", QWERTY123, SYM2, tap_at(0.5 * U, row_y(1)), QWERTY123,
                                "tap:ax"], "xbc")
        # Super latched, keyboard hidden and shown again: must not leak
        single("super-cleared", [QWERTY123, SYM2, tap_at(2.5 * U, row_y(1)),
                                 hide_show, "tap:y"], "y")

        # word memory is per text field
        a, b = Field("osk-a"), Field("osk-b", "abcdef")
        try:
            a.focus()
            a.send("swipe:hello")
            time.sleep(1.0)
            b.focus()
            time.sleep(0.5)
            b.send("tap:<")
            check("focus-reset", b.result(), "abcde")
            a.focus()
            check("focus-reset-other", a.result(), "hello ")
        finally:
            a.close()
            b.close()
            time.sleep(0.3)

        # auto-show follows text-field focus changes
        a, b = Field("osk-auto"), Field("osk-auto-b")
        try:
            osk("USR1")
            wait(lambda: not keyboard_visible())
            a.focus()
            check("autoshow-on-focus", wait(keyboard_visible, 2), True)
            bg = sorted(os.listdir("/usr/share/omarchy/themes/catppuccin/backgrounds"))[0]
            viewer = subprocess.Popen(["imv", "/usr/share/omarchy/themes/catppuccin/backgrounds/" + bg],
                                      start_new_session=True, stdout=subprocess.DEVNULL,
                                      stderr=subprocess.DEVNULL)
            wait(lambda: hypr("activewindow").get("class") == "imv", 4)
            check("autohide-without-field", wait(lambda: not keyboard_visible(), 2), True)
            viewer.terminate()
            time.sleep(0.5)
            a.focus()
            check("autoshow-again", wait(keyboard_visible, 2), True)
        finally:
            a.close()
            b.close()

        osk("USR2")
        wait(keyboard_visible)
        subprocess.run([os.path.expanduser("~/.local/bin/omarchy-osk-restart")])
        time.sleep(2)
        check("restart-keeps-visible", keyboard_visible(), True)
    except Abort as e:
        print(f"ABORTED: {e}", flush=True)
        results.append(("aborted", False, str(e), None))
    finally:
        set_focus_on_activate(focus_on_activate)
        daemon.terminate()
        subprocess.run(["systemctl", "--user", "start", "omarchy-osk-autoshow"])
        if saved_lang is None:
            os.remove(STATE)
        else:
            with open(STATE, "w") as fh:
                fh.write(saved_lang)
        subprocess.run([os.path.expanduser("~/.local/bin/omarchy-osk-restart")])

    failed = [r for r in results if not r[1]]
    print(f"\n{len(results) - len(failed)}/{len(results)} passed")
    sys.exit(1 if failed else 0)


def set_focus_on_activate(on):
    sh("hyprctl", "eval",
       "hl.config({ misc = { focus_on_activate = %s } })" % ("true" if on else "false"))


def hide_show():
    osk("USR1")
    time.sleep(0.6)
    osk("USR2")
    time.sleep(0.8)


if __name__ == "__main__":
    main()
