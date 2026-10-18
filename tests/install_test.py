#!/usr/bin/python3 -I
"""The installer, setup and uninstaller, end to end in a throwaway home.

omarchy, hyprctl, systemctl, pkill/pgrep and notify-send are replaced by
stubs that log what they are asked; the scripts themselves and Omarchy's
real plugin sources (for the patches) are used. Nothing outside the
temporary home is touched.

usage: tests/install_test.py [--keep]   (after `cargo build --release`)
"""

import http.server
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, ".."))

STUBS = {
    "omarchy": r'''#!/bin/bash
echo "omarchy $*" >>"$STUB_LOG"
case "$1 $2" in
  "version "*) echo "${FAKE_OMARCHY_VERSION:-4.0.4-1}" ;;
  "plugin clone")
    name=${3#omarchy.}
    dir="$HOME/.config/omarchy/plugins/tester.$name"
    mkdir -p "$dir"
    cp -a "/usr/share/omarchy/shell/plugins/$name/." "$dir/"
    jq --arg id "tester.$name" --arg src "$3" '.id = $id | .omarchy.clonedFrom = $src' \
      "/usr/share/omarchy/shell/plugins/$name/manifest.json" >"$dir/manifest.json" ;;
  "plugin remove") rm -rf "$HOME/.config/omarchy/plugins/$3" ;;
  "plugin enable")
    # like Omarchy's shell: a plugin copied in a moment ago is not known yet
    seen="$HOME/.stub-seen-$3"
    [[ -e $seen ]] || { touch "$seen"; echo "Unknown plugin $3" >&2; exit 1; } ;;
esac
exit 0
''',
    "hyprctl": r'''#!/bin/bash
echo "hyprctl $*" >>"$STUB_LOG"
case "$*" in
  "getoption input:kb_layout -j") echo '{"str": "fi"}' ;;
  "getoption input:kb_variant -j") echo '{"str": ""}' ;;
  "layers -j") echo '{}' ;;
esac
exit 0
''',
    "systemctl": '#!/bin/bash\necho "systemctl $*" >>"$STUB_LOG"\n',
    "pkill": '#!/bin/bash\necho "pkill $*" >>"$STUB_LOG"\nexit 1\n',
    "pgrep": '#!/bin/bash\nexit 1\n',
    "notify-send": '#!/bin/bash\necho "notify-send $*" >>"$STUB_LOG"\n',
}

LOOKNFEEL = "-- my Hyprland look\nhl.config({ general = { gaps_in = 5 } })\n"
# your own Omarchy menu extension: no trailing comma allowed, // inside a string
MENU = """{
  // my menu /* not a block */
  "personal": {"icon":"","label":"Personal"},
  /* a block
     comment */
  "personal.site": {"icon":"","label":"Site","action":"xdg-open https://example.com//x"}
}
"""


def jsonc(text):
    """Omarchy's JSONC: comments out, then strict JSON (no trailing commas)"""
    out, i, n, in_str = [], 0, len(text), False
    while i < n:
        c = text[i]
        if in_str:
            out.append(c)
            if c == "\\":
                out.append(text[i + 1])
                i += 1
            elif c == '"':
                in_str = False
        elif c == '"':
            in_str = True
            out.append(c)
        elif text.startswith("//", i):
            i = text.find("\n", i) if "\n" in text[i:] else n
            continue
        elif text.startswith("/*", i):
            i = text.find("*/", i) + 2
            continue
        else:
            out.append(c)
        i += 1
    try:
        return json.loads("".join(out))
    except ValueError:
        return None

results = []


def check(name, ok, detail=""):
    results.append(ok)
    print(f"{'PASS' if ok else 'FAIL'}  {name}" + ("" if ok else f": {detail}"), flush=True)


def main():
    keep = "--keep" in sys.argv
    base = tempfile.mkdtemp(prefix="osk-install-")
    home = os.path.join(base, "home")
    stubs = os.path.join(base, "stubs")
    os.makedirs(stubs)
    for name, text in STUBS.items():
        p = os.path.join(stubs, name)
        open(p, "w").write(text)
        os.chmod(p, 0o755)
    os.makedirs(os.path.join(home, ".config/hypr"))
    open(os.path.join(home, ".config/hypr/looknfeel.lua"), "w").write(LOOKNFEEL)
    os.makedirs(os.path.join(home, ".config/omarchy/extensions"))
    open(os.path.join(home, ".config/omarchy/extensions/omarchy-menu.jsonc"), "w").write(MENU)

    # a release: staged tree, tarball and checksums, served over HTTP
    rel = os.path.join(base, "release")
    os.makedirs(rel)
    stage = os.path.join(base, "stage", "omarchy-osk")
    subprocess.run([os.path.join(ROOT, "tools/stage.sh"), stage], check=True, stdout=subprocess.DEVNULL)
    open(os.path.join(stage, "share/omarchy-osk/TESTED"), "w").write(
        "version=test\nomarchy=4.0.4\ndevice=Surface Pro 3 with the Type Cover\n")
    subprocess.run(["tar", "-czf", os.path.join(rel, "omarchy-osk-x86_64.tar.gz"), "-C",
                    os.path.dirname(stage), "omarchy-osk"], check=True)
    subprocess.run("sha256sum omarchy-osk-x86_64.tar.gz > SHA256SUMS", shell=True, cwd=rel, check=True)
    class Quiet(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *a):
            super().__init__(*a, directory=rel)

        def log_message(self, *a):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Quiet)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{server.server_address[1]}"

    log = os.path.join(base, "stub.log")
    env = {"HOME": home, "PATH": f"{stubs}:{home}/.local/bin:/usr/bin:/bin", "STUB_LOG": log,
           "LANG": "fi_FI.UTF-8", "OSK_RELEASE_URL": url, "TERM": "dumb"}

    def run(*cmd, **kw):
        p = subprocess.run(list(cmd), env={**env, **kw.pop("env", {})}, capture_output=True, text=True, **kw)
        return p

    def path(*p):
        return os.path.join(home, *p)

    def read(*p):
        try:
            return open(path(*p)).read()
        except OSError:
            return ""

    def logged(text):
        return text in open(log).read() if os.path.exists(log) else False

    # ---------------------------------------------------------------- install
    p = run("bash", os.path.join(ROOT, "install.sh"), "--yes")
    check("install from the release", p.returncode == 0, p.stdout + p.stderr)
    for f in [".local/bin/omarchy-oskd", ".local/bin/omarchy-osk", ".local/bin/omarchy-osk-setup",
              ".local/bin/omarchy-osk-uninstall", ".local/bin/omarchy-osk-tested",
              ".local/share/omarchy-osk/languages.txt", ".local/share/omarchy-osk/dict/fi.txt",
              ".local/share/omarchy-osk/layouts/qwerty.txt", ".local/share/omarchy-osk/TESTED",
              ".local/lib/omarchy-osk/omarchy-look", ".config/systemd/user/omarchy-osk-autoshow.service"]:
        check(f"installed {f}", os.path.exists(path(f)))
    conf = read(".config/omarchy-osk/config")
    check("languages: English and the system's", 'OSK_LANGS="en,fi"' in conf, conf)
    check("typing layout from Hyprland", 'OSK_XKB="fi"' in conf, conf)
    check("keyboard height", 'OSK_HEIGHT_RATIO="0.40"' in conf, conf)
    has_cover = "Type Cover" in open("/proc/bus/input/devices").read()
    if has_cover:
        check("auto-show with the Type Cover", 'OSK_AUTOSHOW="cover"' in conf and "Type Cover" in conf, conf)
    for h in ("theme-set.d", "font-set.d", "post-update.d"):
        check(f"hook {h}", os.access(path(".config/omarchy/hooks", h, "omarchy-osk"), os.X_OK))
    check("bar button", os.path.exists(path(".config/omarchy/plugins/omarchy-osk/manifest.json"))
          and logged("omarchy plugin enable user.osk right"))
    for name, needle in (("lock", "omarchy-osk"), ("menu", "OnDemand"), ("image-picker", "OnDemand")):
        d = path(".config/omarchy/plugins", f"tester.{name}")
        text = " ".join(open(os.path.join(dp, f), errors="ignore").read()
                        for dp, _, fs in os.walk(d) for f in fs if f.endswith(".qml")) if os.path.isdir(d) else ""
        check(f"patched copy of Omarchy's {name}", needle in text)
    lnf = read(".config/hypr/looknfeel.lua")
    check("layer rule added", lnf.startswith(LOOKNFEEL) and lnf.count('namespace = "^omarchy-osk$"') == 1, lnf)
    check("auto-show service enabled", logged("systemctl --user enable omarchy-osk-autoshow.service"))
    desktop = read(".local/share/applications/omarchy-osk-setup.desktop")
    check("app launcher entry", "Exec=omarchy-launch-floating-terminal-with-presentation "
          + path(".local/bin/omarchy-osk-setup") in desktop, desktop)
    menu = read(".config/omarchy/extensions/omarchy-menu.jsonc")
    items = jsonc(menu) or {}
    check("Omarchy menu entry, your entries kept", "setup.osk" in items and "personal.site" in items
          and items["setup.osk"]["action"].endswith("omarchy-osk-setup"), menu)

    # ---------------------------------------------------------------- interactive
    # the gum questions in a terminal, Enter on each: the same as --yes
    before = read(".config/omarchy-osk/config")
    pid, fd = os.forkpty()
    if pid == 0:
        os.execve(path(".local/bin/omarchy-osk-setup"), [path(".local/bin/omarchy-osk-setup")],
                  {**env, "TERM": "xterm-256color", "COLUMNS": "100", "LINES": "40"})
    import fcntl
    import select
    import struct
    import termios
    import time
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 100, 0, 0))
    out = b""
    since = b""  # what it showed since the last Enter
    end = time.time() + 60
    status = None
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.3)
        if r:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                break
            if not chunk:
                break
            out += chunk
            since += chunk
        elif b"enter" in since:
            os.write(fd, b"\r")  # a question waits (gum's key help is up): take its default
            since = b""
        done, st = os.waitpid(pid, os.WNOHANG)
        if done:
            status = os.waitstatus_to_exitcode(st)
            break
    if status is None:
        try:
            _, st = os.waitpid(pid, 0)  # it closed the terminal: collect it
            status = os.waitstatus_to_exitcode(st)
        except ChildProcessError:
            pass
    text = out.decode(errors="ignore")
    check("interactive setup ends", status == 0, text[-600:])
    check("interactive defaults keep the settings", read(".config/omarchy-osk/config") == before,
          read(".config/omarchy-osk/config"))
    plain = re.sub(r"\x1b(\[[0-9;?<=>]*[ -/]*[@-~]|[@-_])", "", text)
    missing = [q for q in ("Which languages", "Show the keyboard automatically", "Keyboard height")
               if q not in plain]
    check("interactive setup asks", not missing, f"never asked: {missing}")

    # ---------------------------------------------------------------- setup again
    p = run(path(".local/bin/omarchy-osk-setup"), "--yes", "--langs", "en,fi,de-DE", "--download", "no",
            "--height", "large", "--autoshow", "always")
    conf = read(".config/omarchy-osk/config")
    check("setup again", p.returncode == 0, p.stdout + p.stderr)
    check("new languages", 'OSK_LANGS="en,fi,de-DE"' in conf, conf)
    check("new height", 'OSK_HEIGHT_RATIO="0.47"' in conf and conf.count("OSK_HEIGHT_RATIO") == 1, conf)
    check("always show", 'OSK_AUTOSHOW="always"' in conf, conf)
    check("no word list without download", not os.path.exists(path(".local/share/omarchy-osk/dict/de-DE.txt")))
    check("layer rule only once", read(".config/hypr/looknfeel.lua").count('namespace = "^omarchy-osk$"') == 1)
    check("menu entry only once", read(".config/omarchy/extensions/omarchy-menu.jsonc").count('"setup.osk"') == 1)

    # a word list downloaded and filtered for the layout (needs the network)
    p = run(path(".local/bin/omarchy-osk-setup"), "--yes", "--langs", "en,fi,de-DE", "--download", "yes")
    words = read(".local/share/omarchy-osk/dict/de-DE.txt").splitlines()
    check("downloaded German word list", len(words) > 10000 and any(w.startswith("für ") for w in words),
          f"{len(words)} words; {p.stdout[-400:]}")

    # never: the service goes
    p = run(path(".local/bin/omarchy-osk-setup"), "--yes", "--autoshow", "never")
    check("never: service disabled", logged("systemctl --user disable --now omarchy-osk-autoshow.service"))

    # without the patches: Omarchy's own plugins come back
    p = run(path(".local/bin/omarchy-osk-setup"), "--yes", "--patches", "no")
    check("patches off", p.returncode == 0 and not os.path.exists(path(".config/omarchy/plugins/tester.lock"))
          and not os.path.exists(path(".config/omarchy/hooks/post-update.d/omarchy-osk"))
          and logged("omarchy plugin enable omarchy.lock"), p.stdout + p.stderr)

    # ---------------------------------------------------------------- versions
    tested = path(".local/bin/omarchy-osk-tested")
    for ver, want in (("4.0.4-1", 0), ("4.0.9-1", 1), ("4.1.0-1", 2), ("5.0.0-1", 2), ("3.9.0-1", 2)):
        p = run(tested, env={"FAKE_OMARCHY_VERSION": ver})
        check(f"Omarchy {ver}: status {want}", p.returncode == want, p.stdout)
    p = run(tested, "--notify", env={"FAKE_OMARCHY_VERSION": "4.1.0-1"})
    check("notification on a newer Omarchy", logged("notify-send -a omarchy-osk"))

    # a damaged download is refused
    with open(os.path.join(rel, "omarchy-osk-x86_64.tar.gz"), "ab") as f:
        f.write(b"x")
    p = run("bash", os.path.join(ROOT, "install.sh"), "--yes")
    check("damaged download refused", p.returncode != 0 and "checksum" in p.stderr, p.stderr)

    # ---------------------------------------------------------------- uninstall
    p = run(path(".local/bin/omarchy-osk-setup"), "--yes", "--patches", "yes")
    p = run(path(".local/bin/omarchy-osk-uninstall"))
    check("uninstall", p.returncode == 0, p.stdout + p.stderr)
    left = [os.path.relpath(os.path.join(dp, f), home) for dp, _, fs in os.walk(home) for f in fs]
    stray = [f for f in left if "omarchy-osk" in f and not f.startswith(".config/omarchy-osk/")]
    check("nothing left but the settings", not stray, stray)
    check("patched copies gone", not any("tester." in f for f in left), [f for f in left if "tester." in f])
    check("layer rule removed, the rest intact", read(".config/hypr/looknfeel.lua") == LOOKNFEEL,
          repr(read(".config/hypr/looknfeel.lua")))
    check("menu entry removed, the rest intact", read(".config/omarchy/extensions/omarchy-menu.jsonc") == MENU,
          repr(read(".config/omarchy/extensions/omarchy-menu.jsonc")))
    check("settings kept", os.path.exists(path(".config/omarchy-osk/config")))

    server.shutdown()
    if keep:
        print(f"kept {base}")
    else:
        shutil.rmtree(base)
    print(f"\n{sum(results)}/{len(results)} passed")
    sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    main()
