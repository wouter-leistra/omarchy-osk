#!/usr/bin/python3 -I
"""Cut the raw promo recording (tests/promo.py) into a fast-paced clip with
captions, ready for X: 1800x1200, 30 fps, H.264. The time setup takes to
restart things between the menu and the demo is left out.

usage: tests/promo_edit.py RAW.mkv OUT.mp4
"""

import json
import os
import subprocess
import sys

BOLD = "/usr/share/fonts/TTF/JetBrainsMonoNerdFont-Bold.ttf"
REGULAR = "/usr/share/fonts/TTF/JetBrainsMonoNerdFont-Regular.ttf"
W, H = 1800, 1200


def esc(text):
    return text.replace("\\", "\\\\").replace(":", "\\:").replace("'", "’").replace("%", "\\%")


DIM = "drawbox=x=0:y=0:w=iw:h=ih:color=black@0.72:t=fill"


def caption(text, y=250, size=50):
    return (f"drawtext=fontfile={BOLD}:text='{esc(text)}':fontsize={size}:fontcolor=white:"
            f"box=1:boxcolor=black@0.62:boxborderw=26:x=(w-text_w)/2:y={y}")


def sync(raw, m):
    """seconds the marks run ahead of the video: the first scene opens the
    Omarchy menu over the wallpaper, so find the frame where it appears"""
    stats = subprocess.run(
        ["ffmpeg", "-v", "error", "-t", "60", "-i", raw, "-vf",
         "crop=400:800:880:340,signalstats,metadata=print:key=lavfi.signalstats.YAVG:file=-",
         "-f", "null", "-"], capture_output=True, text=True).stdout
    t, base = None, None
    for line in stats.splitlines():
        if "pts_time:" in line:
            t = float(line.split("pts_time:")[1].split()[0])
        elif "YAVG=" in line:
            v = float(line.split("YAVG=")[1])
            base = v if base is None else base
            if abs(v - base) > 8:
                return t - m["menu"] - 0.15    # the menu takes a moment to draw
    return 0.0


def main():
    raw, out = sys.argv[1], sys.argv[2]
    m = json.load(open(os.path.splitext(raw)[0] + ".json"))
    shift = sync(raw, m)
    print(f"marks shifted by {shift:+.2f}s")
    m = {k: v + shift for k, v in m.items()}
    # (start, end, speed, filters before scaling, captions)
    low = 1030                     # captions under the terminal text
    zoom = ["crop=1440:960:360:300"]  # the Omarchy menu and its floating terminal
    segs = [
        # the empty desktop before the menu scene (recorded first), slowed down
        (max(0.3, m["menu"] - 1.7), m["menu"], 0.6, [], [DIM, caption("omarchy-osk", y=420, size=96),
                                      caption("a Gboard-style touch keyboard for Omarchy", y=580, size=40)]),
        (m["install"], m["download"], 1.6, [], [caption("One line to install", y=low)]),
        (m["download"], m["languages"], 1.5, [], [caption("Prebuilt, checksummed, no root", y=low)]),
        (m["languages"], m["questions"], 1.3, [], [caption("Pick your languages: 100+ layouts", y=low)]),
        (m["questions"], m["installed"] + 1.5, 3.0, [], [caption("…a few questions, and done", y=low)]),
        (m["menu"], m["menu_end"], 1.3, zoom, [caption("Change it any time from the Omarchy menu", y=50)]),
        (m["autoshow"], m["autohide"], 1.0, [], [caption("Pops up when a text field gets focus")]),
        (m["autohide"], m["typing"], 1.15, [], [caption("…and gets out of the way when not")]),
        (m["typing"], m["typing"] + 6.0, 1.3, [], [caption("Swipe or tap to type")]),
        (m["typing"] + 6.0, m["language"], 1.3, [], [caption("Long-press for numbers & symbols")]),
        (m["language"], m["super"], 1.3, [], [caption("Swipe the space bar to switch language")]),
        (m["super"], m["switcher"], 1.0, [], [caption("Real Shift, Ctrl and Super keys")]),
        (m["switcher"], m["theme"], 1.25, [], [caption("Super+Ctrl+Shift+Space → theme switcher")]),
        (m["theme"], m["follows"], 3.0, [], [caption("Switch the Omarchy theme…")]),
        (m["follows"], m["toggle"], 1.3, [], [caption("…and the keyboard follows it")]),
        (m["toggle"], m["end"], 1.0, [], [caption("One tap in the bar to toggle")]),
        (m["end"], m["end"] + 3.0, 1.0, [], [DIM, caption("omarchy-osk", y=330, size=96),
                                             caption("made for the Surface Pro 3 and its Type Cover", y=490, size=38),
                                             caption("github.com/wouter-leistra/omarchy-osk", y=620, size=44)]),
    ]
    parts, labels = [], []
    for i, (a, b, speed, pre, caps) in enumerate(segs):
        chain = [f"trim=start={a:.3f}:end={b:.3f}", "setpts=(PTS-STARTPTS)/%.3f" % speed] + pre + [
                 f"scale={W}:{H}:flags=lanczos", "fps=30"] + caps
        parts.append(f"[0:v]{','.join(chain)}[v{i}]")
        labels.append(f"[v{i}]")
    graph = ";".join(parts) + f";{''.join(labels)}concat=n={len(segs)}:v=1:a=0,format=yuv420p[out]"
    subprocess.run(["ffmpeg", "-y", "-v", "error", "-i", raw, "-filter_complex", graph, "-map", "[out]",
                    "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-profile:v", "high",
                    "-movflags", "+faststart", "-an", out], check=True)


if __name__ == "__main__":
    main()
