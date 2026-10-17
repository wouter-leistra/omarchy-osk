//! Command line options, as main.c parses them.

use crate::draw::Color;
use crate::keyboard::{self, Kbd, Scheme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dock {
    Full,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WlLayer {
    Background,
    Bottom,
    Top,
    Overlay,
}

pub struct Options {
    pub schemes: Vec<Scheme>,
    pub trail: Color,
    pub layers: Option<String>,
    pub landscape_layers: Option<String>,
    pub normal_height: u32,
    pub landscape_height: u32,
    pub height: u32,
    pub rounding: i32,
    pub font: Option<String>,
    pub debug: bool,
    pub xkb_layout: Option<String>,
    pub xkb_variant: Option<String>,
    pub langs: Option<String>,
    pub data: Option<String>,
    pub dict: Option<String>,
    pub no_swipe: bool,
    pub hidden: bool,
    pub show_popup: bool,
    pub show_highlight: bool,
    pub exclusive: bool,
    pub im_auto: bool,
    pub layer: WlLayer,
    pub dock: Dock,
    pub width_ratio: f64,
    pub custom_width: u32,
    pub corner_radius: i32,
    pub alpha: Option<u8>,
    pub bg_alpha: Option<u8>,
}

pub const KBD_PIXEL_HEIGHT: u32 = 560;
pub const KBD_PIXEL_LANDSCAPE_HEIGHT: u32 = 560;

/// "rrggbb" or "rrggbbaa" into a bgra colour; anything else leaves it
pub fn set_color(c: &mut Color, hex: &str) {
    if hex.len() != 6 && hex.len() != 8 {
        return;
    }
    let byte = |i: usize| {
        let pair = hex.get(i * 2..i * 2 + 2).unwrap_or("");
        // strtol: the valid hex prefix
        let valid: String = pair.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        u8::from_str_radix(&valid, 16).unwrap_or(0)
    };
    c[2] = byte(0);
    c[1] = byte(1);
    c[0] = byte(2);
    if hex.len() == 8 {
        c[3] = byte(3);
    }
}

fn atoi(s: &str) -> i64 {
    let s = s.trim_start();
    let (neg, d) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let n = d.bytes().take_while(|b| b.is_ascii_digit()).fold(0i64, |n, b| n.wrapping_mul(10).wrapping_add((b - b'0') as i64));
    if neg { -n } else { n }
}

fn atof(s: &str) -> f64 {
    let s = s.trim_start();
    let mut end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && i == 0) {
            end = i + 1;
        } else {
            break;
        }
    }
    s[..end].parse().unwrap_or(0.0)
}

pub fn usage(argv0: &str) {
    eprintln!("usage: {argv0} [-hov] [-H height] [-L landscape height] [-fn font] [-l layers]");
    eprint!(
        "Options:
  -D          - Enable debug
  --xkb [layout[:variant]] - xkb layout to type with (default fi)
  --dict [file,file]     - Word frequency lists for gesture typing
  --langs [tag[:layout],...] - Languages, each with its layout
                           (default: the dictionaries)
  --data [dir:dir]       - Layout data search path (default:
                           ~/.config/omarchy-osk:~/.local/share/omarchy-osk)
  --no-swipe             - Disable gesture typing and suggestions
  --trail [rrggbb|aa]    - Set color of the gesture trail
  --fg-accent [rrggbb|aa]    - Set color of the enter key
  --press-accent [rrggbb|aa] - Set color of the pressed enter key
  --text-accent [rrggbb|aa]  - Set color of text on the enter key
  -H [int]    - Height in pixels
  -L [int]    - Landscape height in pixels
  -R [int]    - Rounding radius in pixels
  --fn [font] - Set font (e.g: DejaVu Sans 20)
  --hidden    - Start hidden (send SIGUSR2 to show)
  --no-popup             - Disable the key-press popup
  --no-highlight         - Don't highlight a key while pressed
  --no-feedback          - Disable all key-press feedback (--no-popup and --no-highlight)
  --alpha [int]          - Set alpha value for all colors [0-255]
  --auto                 - Automatically toggle visibility based on focus
  --bg [rrggbb|aa]       - Set color of background
  --fg [rrggbb|aa]       - Set color of keys
  --fg-sp [rrggbb|aa]    - Set color of special keys
  --press [rrggbb|aa]    - Set color of pressed keys
  --press-sp [rrggbb|aa] - Set color of pressed special keys
  --swipe [rrggbb|aa]    - Set color of swiped keys
  --swipe-sp [rrggbb|aa] - Set color of swiped special keys
  --text [rrggbb|aa]     - Set color of text on keys
  --text-sp [rrggbb|aa]  - Set color of text on special keys
  --text-press [rrggbb|aa]    - Set color of text on pressed keys
  --text-press-sp [rrggbb|aa] - Set color of text on pressed special keys
  --text-swipe [rrggbb|aa]    - Set color of text on swiped keys
  --text-swipe-sp [rrggbb|aa] - Set color of text on swiped special keys
  --list-layers      - Print the list of available layers
  -l                 - Comma separated list of layers
  --landscape-layers - Comma separated list of landscape layers
  --dock [left|right|full] - Dock keyboard to left or right edge
  --ratio [0.3-1.0]        - Screen width ratio for docked keyboard
  --width [pixels]         - Custom fixed width in pixels
  --corner-radius [pixels] - Corner radius of keyboard window
  --bg-alpha [0-255]       - Independent opacity for keyboard baseplate
  --non-exclusive    - Allow the keyboard to overlap windows. Do not request an exclusive zone from the compositor
  --wayland-layer [background|bottom|top|overlay] - Wayland layer shell z index
"
    );
}

pub fn parse(args: &[String]) -> Options {
    let schemes = keyboard::default_schemes();
    let mut o = Options {
        trail: schemes[0].swipe,
        schemes,
        layers: std::env::var("WVKBD_LAYERS").ok(),
        landscape_layers: std::env::var("WVKBD_LANDSCAPE_LAYERS").ok(),
        normal_height: std::env::var("WVKBD_HEIGHT").map(|v| atoi(&v) as u32).unwrap_or(KBD_PIXEL_HEIGHT),
        landscape_height: std::env::var("WVKBD_LANDSCAPE_HEIGHT").map(|v| atoi(&v) as u32).unwrap_or(KBD_PIXEL_LANDSCAPE_HEIGHT),
        height: KBD_PIXEL_LANDSCAPE_HEIGHT,
        rounding: keyboard::DEFAULT_ROUNDING,
        font: None,
        debug: false,
        xkb_layout: None,
        xkb_variant: None,
        langs: None,
        data: None,
        dict: None,
        no_swipe: false,
        hidden: false,
        show_popup: true,
        show_highlight: true,
        exclusive: true,
        im_auto: false,
        layer: WlLayer::Overlay,
        dock: Dock::Full,
        width_ratio: 1.0,
        custom_width: 0,
        corner_radius: -1,
        alpha: None,
        bg_alpha: None,
    };
    let argv0 = args.first().map(|s| s.as_str()).unwrap_or("omarchy-oskd");
    let mut i = 1;
    let need = |i: usize| -> &String {
        if i + 1 >= args.len() {
            usage(argv0);
            std::process::exit(1);
        }
        &args[i + 1]
    };
    while i < args.len() {
        let a = args[i].as_str();
        // "-x" and "--x" are the same for the long wvkbd options
        let long = a.strip_prefix("--").or_else(|| a.strip_prefix('-')).unwrap_or("");
        let color = |c: &mut Color| {
            set_color(c, need(i));
        };
        match (a, long) {
            ("-v" | "--version", _) => {
                println!("wvkbd-omarchy-osk-rust");
                std::process::exit(0);
            }
            ("-h" | "--help", _) => {
                usage(argv0);
                std::process::exit(0);
            }
            ("-l", _) => {
                o.layers = Some(need(i).clone());
                i += 1;
            }
            (_, "landscape-layers") => {
                o.landscape_layers = Some(need(i).clone());
                i += 1;
            }
            (_, "bg") => {
                color(&mut o.schemes[0].bg);
                i += 1;
            }
            (_, "alpha") => {
                o.alpha = Some(atoi(need(i)) as u8);
                i += 1;
            }
            ("--bg-alpha", _) => {
                o.bg_alpha = Some(atoi(need(i)) as u8);
                i += 1;
            }
            (_, "fg") => {
                color(&mut o.schemes[0].fg);
                i += 1;
            }
            (_, "fg-sp") => {
                color(&mut o.schemes[1].fg);
                i += 1;
            }
            (_, "press") => {
                color(&mut o.schemes[0].high);
                i += 1;
            }
            (_, "press-sp") => {
                color(&mut o.schemes[1].high);
                i += 1;
            }
            (_, "swipe") => {
                color(&mut o.schemes[0].swipe);
                i += 1;
            }
            (_, "swipe-sp") => {
                color(&mut o.schemes[1].swipe);
                i += 1;
            }
            (_, "text") => {
                color(&mut o.schemes[0].text);
                i += 1;
            }
            (_, "text-sp") => {
                color(&mut o.schemes[1].text);
                i += 1;
            }
            (_, "text-press") => {
                color(&mut o.schemes[0].text_press);
                i += 1;
            }
            (_, "text-press-sp") => {
                color(&mut o.schemes[1].text_press);
                i += 1;
            }
            (_, "text-swipe") => {
                color(&mut o.schemes[0].text_swipe);
                i += 1;
            }
            (_, "text-swipe-sp") => {
                color(&mut o.schemes[1].text_swipe);
                i += 1;
            }
            ("-H", _) => {
                o.normal_height = atoi(need(i)) as u32;
                i += 1;
            }
            ("-L", _) => {
                o.landscape_height = atoi(need(i)) as u32;
                o.height = o.landscape_height;
                i += 1;
            }
            ("-R", _) => {
                o.rounding = atoi(need(i)) as i32;
                i += 1;
            }
            ("-D", _) => o.debug = true,
            (_, "fn") => {
                o.font = Some(need(i).clone());
                i += 1;
            }
            ("--xkb", _) => {
                let v = need(i).clone();
                match v.split_once(':') {
                    Some((l, var)) => {
                        o.xkb_layout = Some(l.into());
                        o.xkb_variant = Some(var.into());
                    }
                    None => o.xkb_layout = Some(v),
                }
                i += 1;
            }
            ("--langs", _) => {
                o.langs = Some(need(i).clone());
                i += 1;
            }
            ("--data", _) => {
                o.data = Some(need(i).clone());
                i += 1;
            }
            ("--dict", _) => {
                o.dict = Some(need(i).clone());
                i += 1;
            }
            ("--no-swipe", _) => o.no_swipe = true,
            ("--trail", _) => {
                set_color(&mut o.trail, need(i));
                i += 1;
            }
            ("--fg-accent", _) => {
                color(&mut o.schemes[2].fg);
                i += 1;
            }
            ("--press-accent", _) => {
                color(&mut o.schemes[2].high);
                i += 1;
            }
            ("--text-accent", _) => {
                let v = need(i).clone();
                set_color(&mut o.schemes[2].text, &v);
                set_color(&mut o.schemes[2].text_press, &v);
                i += 1;
            }
            (_, "hidden") => o.hidden = true,
            (_, "no-popup") => o.show_popup = false,
            (_, "no-highlight") => o.show_highlight = false,
            (_, "no-feedback") => {
                o.show_popup = false;
                o.show_highlight = false;
            }
            (_, "list-layers") => {
                for n in ["abc", "sym"] {
                    println!("{n}");
                }
                std::process::exit(0);
            }
            (_, "non-exclusive") => o.exclusive = false,
            (_, "auto") => o.im_auto = true,
            (_, "wayland-layer") => {
                o.layer = match need(i).as_str() {
                    "background" => WlLayer::Background,
                    "bottom" => WlLayer::Bottom,
                    "top" => WlLayer::Top,
                    "overlay" => WlLayer::Overlay,
                    _ => {
                        usage(argv0);
                        std::process::exit(1);
                    }
                };
                i += 1;
            }
            ("--dock", _) => {
                match need(i).as_str() {
                    "left" | "right" => {
                        o.dock = if need(i) == "left" { Dock::Left } else { Dock::Right };
                        if o.width_ratio == 1.0 {
                            o.width_ratio = 0.68;
                        }
                    }
                    _ => {
                        o.dock = Dock::Full;
                        o.width_ratio = 1.0;
                    }
                }
                i += 1;
            }
            ("--corner-radius", _) => {
                o.corner_radius = atoi(need(i)) as i32;
                i += 1;
            }
            ("--width", _) => {
                o.custom_width = atoi(need(i)) as u32;
                i += 1;
            }
            ("--ratio", _) => {
                o.width_ratio = atof(need(i)).clamp(0.3, 1.0);
                i += 1;
            }
            _ => {
                eprintln!("Invalid argument: {a}");
                usage(argv0);
                std::process::exit(1);
            }
        }
        i += 1;
    }
    o
}

fn default_data_dirs() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let cfg = std::env::var("XDG_CONFIG_HOME").ok().filter(|s| !s.is_empty()).unwrap_or(format!("{home}/.config"));
    let data = std::env::var("XDG_DATA_HOME").ok().filter(|s| !s.is_empty()).unwrap_or(format!("{home}/.local/share"));
    format!("{cfg}/omarchy-osk:{data}/omarchy-osk")
}

impl Options {
    /// the keyboard as main.c sets it up before connecting to Wayland
    pub fn build_kbd(&self) -> Kbd {
        let mut schemes = self.schemes.clone();
        // every scheme shares the keyboard background
        for i in 1..schemes.len() {
            schemes[i].bg = schemes[0].bg;
        }
        let mut kb = Kbd::new(schemes);
        kb.debug = self.debug;
        kb.trail = self.trail;
        kb.show_popup = self.show_popup;
        kb.show_highlight = self.show_highlight;
        if let Some(l) = &self.layers {
            kb.layers = Kbd::parse_layers(l);
        }
        if let Some(l) = &self.landscape_layers {
            kb.landscape_layers = Kbd::parse_layers(l);
        }
        if let Some(l) = &self.xkb_layout {
            kb.xkb_layout = l.clone();
        }
        if let Some(v) = &self.xkb_variant {
            kb.xkb_variant = v.clone();
        }
        kb.lang_list = self.langs.clone();
        kb.data_dirs = self.data.clone();

        if let Some(d) = self.dict.as_ref().filter(|_| !self.no_swipe) {
            kb.dict.load(d);
        }
        kb.swipe_enabled = kb.dict.loaded();
        // one language per dictionary unless --langs says otherwise
        if kb.lang_list.is_none() && kb.dict.lang_count() > 0 {
            let list: Vec<&str> = (0..kb.dict.lang_count()).map(|i| kb.dict.lang_code(i)).collect();
            kb.lang_list = Some(list.join(","));
        }
        if kb.data_dirs.is_none() {
            kb.data_dirs = Some(default_data_dirs());
        }

        kb.corner_radius = if self.corner_radius >= 0 {
            self.corner_radius
        } else if self.dock != Dock::Full {
            16
        } else {
            0
        };
        if let Some(alpha) = self.alpha {
            let bg = self.bg_alpha.unwrap_or(alpha);
            for s in &mut kb.schemes[..2] {
                s.bg[3] = bg;
                s.fg[3] = alpha;
                s.high[3] = alpha;
            }
        } else if let Some(bg) = self.bg_alpha {
            kb.schemes[0].bg[3] = bg;
            kb.schemes[1].bg[3] = bg;
        }
        for s in &mut kb.schemes {
            if let Some(f) = &self.font {
                s.font = f.clone();
            }
            if self.rounding != keyboard::DEFAULT_ROUNDING {
                s.rounding = self.rounding;
            }
        }
        kb
    }
}

/// pango font descriptions for the schemes (after kbd init, as main.c)
pub fn load_fonts(kb: &mut Kbd) {
    for s in &mut kb.schemes {
        s.font_desc = Some(pango::FontDescription::from_string(&s.font));
    }
}
