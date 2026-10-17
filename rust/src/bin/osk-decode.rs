//! Rust side of tests/ref/decode_ref.c: the same input, the same output.

use omarchy_osk::swipe::{Dict, Geom, Key};
use std::io::BufRead;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut d = Dict::default();
    d.load(&args[1]);
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
            d.decode(&g, &xs, &ys)
        };
        print!("{}", r.words.len());
        for w in &r.words {
            print!(" {w}");
        }
        println!();
    }
}
