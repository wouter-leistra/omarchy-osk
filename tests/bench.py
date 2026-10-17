#!/usr/bin/python3 -I
"""Speed and resource use of the keyboard, reproducibly.

  decoder   gesture decoding and completions on the 2400 swipes and prefixes
            of the golden tests (time per call, dictionary load)
  trace     the keyboard without Wayland on golden-test scripts: CPU time for
            startup and per input step (key handling and drawing), peak memory
  daemon    (--live) the real daemon on this desktop: time until ready and
            until shown, memory (RSS and PSS) hidden and shown, CPU and
            wakeups while idle. It shows a second keyboard for a few seconds.

usage: tests/bench.py [--runs N] [--live] [--json FILE]
                      [--trace BIN] [--decode BIN] [--daemon BIN]
The C version is measured with the same inputs by pointing the options at
its programs (see Performance in README.md).
"""

import argparse
import json
import os
import re
import signal
import statistics
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, ".."))
sys.path.insert(0, HERE)
import golden  # noqa: E402

THEME_ARGS = ["--fn", golden.FONT, "-R", "14", *golden.THEME]
DICTS = ",".join(os.path.join(ROOT, "dict", f"{l}.txt") for l in ("en", "fi"))
TRACE_SCENARIOS = ("typing", "swiping", "longpress", "languages", "shift")


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(round(p / 100 * (len(xs) - 1))))]


def bench_decoder(binary, runs):
    """word lists load from scratch on the first run (cold), later runs may
    use a cache the program keeps (warm); the first gesture on a layout may
    include one-off preparation"""
    res = {"load_ms": [], "cold_ms": [], "first_us": [], "decode_us": [], "complete_us": []}
    base = tempfile.mkdtemp(prefix="osk-bench-")
    dicts = ",".join(os.path.join(ROOT, "dict", f"{l}.txt") for l in ("en", "fi", "sv"))
    env = dict(os.environ, OSK_TIME="1", XDG_CACHE_HOME=os.path.join(base, "cache"))
    for run in range(runs + 1):
        for lang, layout in golden.DECODE_LANGS:
            geom, text = golden.decode_input(lang, layout, 300)
            g = os.path.join(base, f"{lang}.geo")
            open(g, "w").write(geom)
            p = subprocess.run([binary, dicts, g], input=text, capture_output=True, text=True, env=env)
            first = True
            for line in p.stderr.splitlines():
                parts = line.split()
                if len(parts) == 2 and parts[0] in ("load", "decode", "complete"):
                    v = float(parts[1])
                    if parts[0] == "load":
                        res["cold_ms" if run == 0 and lang == "en" else "load_ms"].append(v / 1000)
                    elif parts[0] == "decode" and first:
                        res["first_us"].append(v)
                        first = False
                    elif run > 0:
                        res[f"{parts[0]}_us"].append(v)
    return {
        "dict_load_cold_ms": statistics.median(res["cold_ms"]),
        "dict_load_ms": statistics.median(res["load_ms"]),
        "decode_us_first": statistics.median(res["first_us"]),
        "decode_us_p50": pct(res["decode_us"], 50),
        "decode_us_p95": pct(res["decode_us"], 95),
        "decode_us_max": max(res["decode_us"]),
        "complete_us_p50": pct(res["complete_us"], 50),
        "complete_us_p95": pct(res["complete_us"], 95),
    }


def trace_once(binary, lines, langs="en,fi", w=2160, h=576, base=None):
    base = base or tempfile.mkdtemp(prefix="osk-bench-")
    args = [binary, "-W", str(w), "-H", str(h), *THEME_ARGS, "--data", os.path.join(ROOT, "data"),
            "--langs", langs, "--dict", DICTS]
    env = dict(os.environ, XDG_RUNTIME_DIR=base, XDG_STATE_HOME=base, XDG_CACHE_HOME=os.path.join(base, "cache"))
    p = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         text=True, env=env, cwd=base)
    p.stdin.write("\n".join(lines) + "\n")
    p.stdin.close()
    _, _, ru = os.wait4(p.pid, 0)
    return ru.ru_utime + ru.ru_stime, ru.ru_maxrss


REPEAT = 4  # scripts run this many times over, so steps outweigh startup


def bench_trace(binary, runs):
    base = tempfile.mkdtemp(prefix="osk-bench-")
    cold = trace_once(binary, [], base=base)[0]
    startup = [trace_once(binary, [], base=base)[0] for _ in range(runs)]
    peak = 0
    per_step = {}
    for name in TRACE_SCENARIOS:
        langs, dicts, build, extra, w, h, rows_of = golden.scenarios()[name]
        t = golden.Script(golden.Geo(golden.load_rows(rows_of), w, h, swipe=bool(dicts)))
        build(t)
        lines = [l for l in t.lines if not l.startswith(("png ", "keymap ", "#"))] * REPEAT
        steps = sum(1 for l in lines if not l.startswith("sleep"))
        cpu = []
        for _ in range(runs):
            c, rss = trace_once(binary, lines, langs or "en,fi", w, h, base=base)
            cpu.append(c)
            peak = max(peak, rss)
        per_step[name] = (statistics.median(cpu) - statistics.median(startup)) / steps * 1000
    return {
        "startup_cold_cpu_ms": cold * 1000,
        "startup_cpu_ms": statistics.median(startup) * 1000,
        "step_cpu_ms": {k: round(v, 3) for k, v in per_step.items()},
        "step_cpu_ms_mean": statistics.mean(per_step.values()),
        "peak_rss_mb": peak / 1024,
    }


def proc_mem(pid):
    rss = pss = 0
    for line in open(f"/proc/{pid}/smaps_rollup"):
        if line.startswith("Rss:"):
            rss = int(line.split()[1])
        elif line.startswith("Pss:"):
            pss = int(line.split()[1])
    return rss / 1024, pss / 1024


def proc_cpu(pid):
    f = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
    ticks = int(f[11]) + int(f[12])
    sw = 0
    for line in open(f"/proc/{pid}/status"):
        if line.startswith(("voluntary_ctxt_switches", "nonvoluntary_ctxt_switches")):
            sw += int(line.split()[1])
    return ticks / os.sysconf("SC_CLK_TCK"), sw


def bench_daemon(binary, runs):
    out = {"ready_ms": [], "show_ms": [], "hidden_rss_mb": [], "hidden_pss_mb": [], "shown_rss_mb": [],
           "shown_pss_mb": [], "idle_cpu_ms_per_10s": [], "idle_wakeups_per_10s": []}
    cache = tempfile.mkdtemp(prefix="osk-bench-cache-")
    for run in range(runs + 1):
        base = tempfile.mkdtemp(prefix="osk-bench-")
        args = [binary, "-D", "--hidden", "-L", "576", "-H", "576", "--xkb", "fi", *THEME_ARGS,
                "--data", os.path.join(ROOT, "data"), "--langs", "en,fi", "--dict", DICTS]
        log = open(os.path.join(base, "log"), "w+")
        env = dict(os.environ, XDG_RUNTIME_DIR=os.environ.get("XDG_RUNTIME_DIR", base), XDG_STATE_HOME=base,
                   XDG_CACHE_HOME=cache)
        t0 = time.monotonic()
        p = subprocess.Popen(args, stdout=log, stderr=log, env=env)

        def wait_for(pattern, timeout=10):
            end = time.monotonic() + timeout
            while time.monotonic() < end:
                log.seek(0)
                m = re.search(pattern, log.read())
                if m:
                    return m
                time.sleep(0.002)
            log.seek(0)
            raise RuntimeError(f"{pattern!r} not seen; log:\n" + log.read()[-3000:])

        try:
            wait_for(r"using xkb layout")
            if run == 0:
                out.setdefault("ready_cold_ms", []).append((time.monotonic() - t0) * 1000)
                continue
            out["ready_ms"].append((time.monotonic() - t0) * 1000)
            time.sleep(1.0)
            r, s = proc_mem(p.pid)
            out["hidden_rss_mb"].append(r)
            out["hidden_pss_mb"].append(s)
            t1 = time.monotonic()
            p.send_signal(signal.SIGUSR2)
            wait_for(r"show: configured after \d+ms, drawing took \d+ms")
            out["show_ms"].append((time.monotonic() - t1) * 1000)
            time.sleep(1.5)
            r, s = proc_mem(p.pid)
            out["shown_rss_mb"].append(r)
            out["shown_pss_mb"].append(s)
            c0, w0 = proc_cpu(p.pid)
            time.sleep(10)
            c1, w1 = proc_cpu(p.pid)
            out["idle_cpu_ms_per_10s"].append((c1 - c0) * 1000)
            out["idle_wakeups_per_10s"].append(w1 - w0)
            p.send_signal(signal.SIGUSR1)
            time.sleep(0.3)
        finally:
            p.terminate()
            p.wait()
    return {k: statistics.median(v) for k, v in out.items()}


def main():
    rel = os.path.join(ROOT, "target", "release")
    ap = argparse.ArgumentParser()
    ap.add_argument("--trace", default=os.path.join(rel, "osk-trace"))
    ap.add_argument("--decode", default=os.path.join(rel, "osk-decode"))
    ap.add_argument("--daemon", default=os.path.join(rel, "omarchy-oskd"))
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--live", action="store_true")
    ap.add_argument("--json")
    ap.add_argument("--only", help="comma separated: decoder,trace,daemon")
    a = ap.parse_args()
    for k in ("trace", "decode", "daemon"):
        setattr(a, k, os.path.abspath(getattr(a, k)))

    only = set(a.only.split(",")) if a.only else {"decoder", "trace", "daemon"}
    r = {"binary_kb": os.path.getsize(a.daemon) / 1024}
    if "decoder" in only:
        r["decoder"] = bench_decoder(a.decode, max(1, a.runs // 2))
    if "trace" in only:
        r["trace"] = bench_trace(a.trace, a.runs)
    if a.live and "daemon" in only:
        r["daemon"] = bench_daemon(a.daemon, max(1, a.runs // 2))

    def show(d, indent=""):
        for k, v in d.items():
            if isinstance(v, dict):
                print(f"{indent}{k}:")
                show(v, indent + "  ")
            else:
                print(f"{indent}{k:24} {v:10.3f}" if isinstance(v, float) else f"{indent}{k:24} {v:>10}")

    show(r)
    if a.json:
        json.dump(r, open(a.json, "w"), indent=1)


if __name__ == "__main__":
    main()
