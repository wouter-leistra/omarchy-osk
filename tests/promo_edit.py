#!/usr/bin/python3 -I
"""Cut the raw promo recording (tests/promo.py) into a fast-paced clip with
captions, ready for X: 1800x1200, 60 fps, H.264.

usage: tests/promo_edit.py RAW.mp4 OUT.mp4
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


def caption(text, y=560, size=50):
    return (f"drawtext=fontfile={BOLD}:text='{esc(text)}':fontsize={size}:fontcolor=white:"
            f"box=1:boxcolor=black@0.62:boxborderw=26:x=(w-text_w)/2:y={y}")


def main():
    raw, out = sys.argv[1], sys.argv[2]
    m = json.load(open(os.path.splitext(raw)[0] + ".json"))
    # (start, end, speed, captions)
    segs = [
        (0.2, m["autoshow"], 1.0, [DIM, caption("omarchy-osk", y=420, size=96),
                                    caption("a touch keyboard for Omarchy on the Surface Pro 3", y=580, size=40)]),
        (m["autoshow"], m["autohide"], 1.0, [caption("Pops up when a text field gets focus")]),
        (m["autohide"], m["typing"], 1.15, [caption("…and gets out of the way when not")]),
        (m["typing"], m["typing"] + 6.0, 1.3, [caption("Swipe or tap to type")]),
        (m["typing"] + 6.0, m["symbols"], 1.2, [caption("Long-press for numbers & symbols")]),
        (m["symbols"], m["super"], 1.3, [caption("Long-press for numbers & symbols")]),
        (m["super"], m["switcher"], 1.0, [caption("Real Shift, Ctrl and Super keys")]),
        (m["switcher"], m["theme"], 1.25, [caption("Super+Ctrl+Shift+Space → theme switcher")]),
        (m["theme"], m["follows"], 3.0, [caption("Switch the Omarchy theme…")]),
        (m["follows"], m["toggle"], 1.3, [caption("…and the keyboard follows it")]),
        (m["toggle"], m["end"], 1.0, [caption("One tap in the bar to toggle")]),
        (m["end"], m["end"] + 2.2, 1.0, [DIM, caption("omarchy-osk", y=420, size=96),
                                         caption("github.com/wouter-leistra/omarchy-osk", y=580, size=44)]),
    ]
    parts, labels = [], []
    for i, (a, b, speed, caps) in enumerate(segs):
        chain = [f"trim=start={a:.3f}:end={b:.3f}", "setpts=(PTS-STARTPTS)/%.3f" % speed,
                 f"scale={W}:{H}:flags=lanczos", "fps=60"] + caps
        parts.append(f"[0:v]{','.join(chain)}[v{i}]")
        labels.append(f"[v{i}]")
    graph = ";".join(parts) + f";{''.join(labels)}concat=n={len(segs)}:v=1:a=0,format=yuv420p[out]"
    subprocess.run(["ffmpeg", "-y", "-v", "error", "-i", raw, "-filter_complex", graph, "-map", "[out]",
                    "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-profile:v", "high",
                    "-movflags", "+faststart", "-an", out], check=True)


if __name__ == "__main__":
    main()
