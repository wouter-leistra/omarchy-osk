//! Gesture decoder driver for tests/golden.py.
//!   osk-decode DICTS GEOMETRY < input
//! GEOMETRY: "kw kh" then "cp cx cy" per key. Each input line is a path of
//! "x,y" points, "?prefix" for completions or "LANG code"; each output line
//! is the number of results and the words.

use omarchy_osk::swipe::{Dict, Geom, Key};
use std::io::BufRead;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // OSK_TIME=1: microseconds per call on stderr, for tests/bench.py
    let timing = std::env::var_os("OSK_TIME").is_some();
    let mut d = Dict::default();
    let t0 = Instant::now();
    d.load(&args[1]);
    if timing {
        eprintln!("load {:.0}", t0.elapsed().as_secs_f64() * 1e6);
    }
    let text = std::fs::read_to_string(&args[2]).expect("geometry");
    let mut nums = text.split_whitespace();
    let mut g = Geom { kw: nums.next().unwrap().parse().unwrap(), kh: nums.next().unwrap().parse().unwrap(), ..Default::default() };
    while let (Some(cp), Some(x), Some(y)) = (nums.next(), nums.next(), nums.next()) {
        g.keys.push(Key { cp: cp.parse().unwrap(), cx: x.parse().unwrap(), cy: y.parse().unwrap() });
    }
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        if let Some(code) = line.strip_prefix("LANG ") {
            d.set_lang_code(code);
            println!("lang {code}");
            continue;
        }
        let complete = line.starts_with('?');
        let mut t = Instant::now();
        let r = if let Some(p) = line.strip_prefix('?') {
            d.complete(p)
        } else {
            let (mut xs, mut ys) = (Vec::new(), Vec::new());
            for p in line.split(' ') {
                if let Some((x, y)) = p.split_once(',') {
                    if let (Ok(x), Ok(y)) = (x.parse(), y.parse()) {
                        xs.push(x);
                        ys.push(y);
                    }
                }
            }
            t = Instant::now();
            d.decode(&g, &xs, &ys)
        };
        if timing {
            eprintln!("{} {:.1}", if complete { "complete" } else { "decode" }, t.elapsed().as_secs_f64() * 1e6);
        }
        print!("{}", r.words.len());
        for w in &r.words {
            print!(" {w}");
        }
        println!();
    }
}
