//! The keyboard without Wayland, for tests/golden.py.
//!
//! Takes the options of omarchy-oskd (plus -W, -H and -S for width, height
//! and scale) and a script on stdin:
//!   down X Y | move X Y | up | cancel    touch events (surface coordinates)
//!   longpress                            the long-press timer fires
//!   sleep MS                             a real pause
//!   hide | show | reset | private on|off what the signals do
//!   png NAME                             NAME.kbd.png and NAME.popup.png
//!   keymap FILE                          the uploaded keymap
//! and prints every virtual keyboard request and, after each step, the
//! keyboard state.

use omarchy_osk::keyboard::{Kbd, Output};
use omarchy_osk::options;
use std::io::{BufRead, Write};

struct Log;

impl Output for Log {
    fn key(&mut self, _time: u32, key: u32, state: u32) {
        println!("key {key} {state}");
    }
    fn modifiers(&mut self, d: u32, l: u32, k: u32, g: u32) {
        println!("mods {d} {l} {k} {g}");
    }
    fn keymap(&mut self, text: &str) {
        println!("keymap 1 {}", text.len() + 1);
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().collect();
    let (mut width, mut height, mut scale) = (2160u32, 576u32, 1.0f64);
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-W" if i + 1 < args.len() => {
                width = args[i + 1].parse().unwrap_or(width);
                args.drain(i..i + 2);
            }
            "-S" if i + 1 < args.len() => {
                scale = args[i + 1].parse().unwrap_or(scale);
                args.drain(i..i + 2);
            }
            "-H" if i + 1 < args.len() => {
                height = args[i + 1].parse().unwrap_or(height);
                args.drain(i..i + 2);
            }
            _ => i += 1,
        }
    }
    let opts = options::parse(&args);
    let mut kb: Kbd = opts.build_kbd();
    let mut out = Log;
    kb.init(&mut out);
    options::load_fonts(&mut kb);

    // the layer surface configure of main.c
    kb.w = width;
    kb.h = height;
    kb.scale = scale;
    kb.layout = kb.layers[0];
    kb.prevlayout = kb.layout;
    kb.last_abc_layout = kb.layout;
    kb.resize();

    let stdin = std::io::stdin();
    let mut t: u32 = 1000;
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let mut it = line.split_whitespace();
        let Some(cmd) = it.next() else { continue };
        if cmd.starts_with('#') {
            continue;
        }
        let args: Vec<&str> = it.collect();
        let num = |i: usize| args.get(i).and_then(|v| v.parse::<i32>().ok());
        t += 10;
        println!("> {line}");
        match cmd {
            "down" if num(1).is_some() => kb.touch_down(&mut out, t, num(0).unwrap(), num(1).unwrap()),
            "move" if num(1).is_some() => kb.touch_motion(&mut out, t, num(0).unwrap(), num(1).unwrap()),
            "up" => kb.touch_up(&mut out, t),
            "cancel" => kb.touch_cancel(&mut out, t),
            "longpress" => {
                if kb.longpress_at != 0 {
                    kb.longpress();
                }
            }
            "sleep" if num(0).is_some() => std::thread::sleep(std::time::Duration::from_millis(num(0).unwrap() as u64)),
            "hide" => kb.on_hide(&mut out),
            "show" => kb.resize(),
            "reset" => kb.reset_text(),
            "private" if !args.is_empty() => kb.set_private(args[0] == "on"),
            "png" if !args.is_empty() => {
                if let Some(s) = kb.surf.as_mut() {
                    let _ = s.write_png(&format!("{}.kbd.png", args[0]));
                }
                if let Some(s) = kb.popup_surf.as_mut() {
                    let _ = s.write_png(&format!("{}.popup.png", args[0]));
                }
            }
            "keymap" if !args.is_empty() => {
                let _ = std::fs::write(args[0], kb.xkb.as_ref().map(|x| x.text.as_str()).unwrap_or(""));
            }
            _ => println!("? unknown command"),
        }
        let mut state = format!(
            "= layout={} lang={} mods={} word=\"{}\" sugg={}",
            kb.layouts[kb.layout].name,
            kb.langs[kb.cur_lang].code,
            kb.mods,
            kb.word,
            kb.sugg.words.len()
        );
        for w in &kb.sugg.words {
            state += &format!(" \"{w}\"");
        }
        state += &format!(" alt={}", if kb.alt_open { kb.alt_cps.len() } else { 0 });
        println!("{state}");
        std::io::stdout().flush().ok();
    }
}
