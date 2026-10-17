//! The keyboard: layouts, key handling, gestures, suggestions, long press
//! and drawing. Wayland requests go through Output.

use crate::draw::{Canvas, Color};
use crate::keymap::Xkb;
use crate::langs::{self, Common, Lang};
use crate::swipe::{self, Dict, Geom, Results};
use crate::unicode;

pub mod keys {
    pub const KEY_ESC: u32 = 1;
    pub const KEY_BACKSPACE: u32 = 14;
    pub const KEY_TAB: u32 = 15;
    pub const KEY_ENTER: u32 = 28;
    pub const KEY_LEFTCTRL: u32 = 29;
    pub const KEY_LEFTSHIFT: u32 = 42;
    pub const KEY_LEFTALT: u32 = 56;
    pub const KEY_SPACE: u32 = 57;
    pub const KEY_HOME: u32 = 102;
    pub const KEY_UP: u32 = 103;
    pub const KEY_PAGEUP: u32 = 104;
    pub const KEY_LEFT: u32 = 105;
    pub const KEY_RIGHT: u32 = 106;
    pub const KEY_END: u32 = 107;
    pub const KEY_DOWN: u32 = 108;
    pub const KEY_PAGEDOWN: u32 = 109;
    pub const KEY_LEFTMETA: u32 = 125;
}
use keys::*;

pub const MAX_ALTS: usize = 16;
const LONGPRESS_MS: i64 = 350;
const SHIFT_DOUBLE_TAP_MS: i64 = 400;
const SUGG_SLOTS: u32 = 3;
pub const KEY_BORDER: u32 = 5;
pub const DEFAULT_FONT: &str = "Sans 20";
pub const DEFAULT_ROUNDING: i32 = 10;

// modifiers passed to the virtual keyboard (wl_keyboard / xkb mod bits)
pub const SHIFT: u8 = 1;
pub const CAPSLOCK: u8 = 2;
pub const CTRL: u8 = 4;
pub const ALT: u8 = 8;
pub const SUPER: u8 = 64;

const WL_PRESSED: u32 = 1;
const WL_RELEASED: u32 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyType {
    Pad,
    Code,
    Mod,
    Layout,
    BackLayer,
    NextLayer,
    Char,
    Hide,
    EndRow,
    Last,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Draw {
    None,
    Unpress,
    Press,
}

#[derive(Clone, Debug)]
pub struct Key {
    pub label: String,
    pub shift_label: String,
    pub width: f64,
    pub ktype: KeyType,
    pub code: u32,
    /// target of Layout keys
    pub layout: usize,
    pub scheme: u8,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// Char key typing a letter: swipes, shifts, words
    pub letter: bool,
    /// 1-based row in the language layout, 0 if fixed
    pub lang_row: u8,
    pub lang_col: u8,
    /// long-press keys for the active language
    pub alts: Vec<u32>,
}

impl Key {
    fn new(label: &str, shift: &str, width: f64, ktype: KeyType) -> Key {
        Key {
            label: label.into(),
            shift_label: shift.into(),
            width,
            ktype,
            code: 0,
            layout: 0,
            scheme: 0,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            letter: false,
            lang_row: 0,
            lang_col: 0,
            alts: Vec::new(),
        }
    }
    fn code(mut self, c: u32) -> Key {
        self.code = c;
        self
    }
    fn scheme(mut self, s: u8) -> Key {
        self.scheme = s;
        self
    }
    fn to(mut self, l: usize) -> Key {
        self.layout = l;
        self
    }
}

pub const ABC: usize = 0;
pub const SYM: usize = 1;
pub const SYM2: usize = 2;
pub const NUM_LAYOUTS: usize = 3;

pub struct Layout {
    /// index into Kbd::sets
    pub set: usize,
    pub name: &'static str,
    /// an alphabetical layout (a primary input layout)
    pub abc: bool,
    pub keyheight: u32,
}

#[derive(Clone, Debug)]
pub struct Scheme {
    pub fg: Color,
    pub bg: Color,
    pub high: Color,
    pub swipe: Color,
    pub text: Color,
    pub text_press: Color,
    pub text_swipe: Color,
    pub font: String,
    pub rounding: i32,
    pub font_desc: Option<pango::FontDescription>,
}

/// 0: letters, 1: modifiers/specials, 2: accent (enter); colours normally
/// come from the omarchy theme through omarchy-osk
pub fn default_schemes() -> Vec<Scheme> {
    let s = |fg: Color, high: Color, text: Color| Scheme {
        bg: [0x2e, 0x1e, 0x1e, 255],
        fg,
        high,
        swipe: [0xfa, 0xb4, 0x89, 255],
        text,
        text_press: text,
        text_swipe: text,
        font: DEFAULT_FONT.into(),
        rounding: DEFAULT_ROUNDING,
        font_desc: None,
    };
    vec![
        s([0x44, 0x32, 0x31, 255], [0x70, 0x5b, 0x58, 255], [0xf4, 0xd6, 0xcd, 255]),
        s([0x22, 0x16, 0x16, 255], [0x70, 0x5b, 0x58, 255], [0xf4, 0xd6, 0xcd, 255]),
        s([0xfa, 0xb4, 0x89, 255], [0xfd, 0xd4, 0xb8, 255], [0x2e, 0x1e, 0x1e, 255]),
    ]
}

const HIDE_LABEL: &str = "\u{f030f}";
const ENTER_LABEL: &str = "\u{23ce}";

fn ch(l: &str, s: &str) -> Key {
    Key::new(l, s, 1.0, KeyType::Char)
}

fn end_row() -> Key {
    Key::new("", "", 0.0, KeyType::EndRow)
}

fn bottom_row(label: &str, layer: usize) -> Vec<Key> {
    vec![
        Key::new(label, label, 1.5, KeyType::Layout).to(layer).scheme(1),
        ch(",", ","),
        Key::new("", "", 5.0, KeyType::Code).code(KEY_SPACE),
        ch(".", "."),
        Key::new(ENTER_LABEL, ENTER_LABEL, 1.5, KeyType::Code).code(KEY_ENTER).scheme(2),
        Key::new(HIDE_LABEL, HIDE_LABEL, 1.0, KeyType::Hide).scheme(1),
        Key::new("", "", 0.0, KeyType::Last),
    ]
}

fn chars(keys: &mut Vec<Key>, list: &[(&str, &str)]) {
    keys.extend(list.iter().map(|(l, s)| ch(l, s)));
}

/// layout.omarchy.h: the Finnish letter layout (the template for the
/// language layouts), ?123 and =\<
pub fn builtin_sets() -> Vec<Vec<Key>> {
    let bksp = || Key::new("⌫", "⌫", 2.0, KeyType::Code).code(KEY_BACKSPACE).scheme(1);
    let mut abc = Vec::new();
    for row in ["qwertyuiopå", "asdfghjklöä"] {
        for c in row.chars() {
            let l = c.to_string();
            abc.push(ch(&l, &l.to_uppercase()));
        }
        abc.push(end_row());
    }
    abc.push(Key::new("⇧", "⇪", 2.0, KeyType::Mod).code(SHIFT as u32).scheme(1));
    for c in "zxcvbnm".chars() {
        let l = c.to_string();
        abc.push(ch(&l, &l.to_uppercase()));
    }
    abc.push(bksp());
    abc.push(end_row());
    abc.extend(bottom_row("?123", SYM));

    let mut sym = Vec::new();
    chars(&mut sym, &[("1", "1"), ("2", "2"), ("3", "3"), ("4", "4"), ("5", "5"), ("6", "6"), ("7", "7"), ("8", "8"), ("9", "9"), ("0", "0"), ("%", "%")]);
    sym.push(end_row());
    chars(&mut sym, &[("@", "@"), ("#", "#"), ("€", "€"), ("$", "$"), ("_", "_"), ("&", "&"), ("-", "-"), ("+", "+"), ("(", "("), (")", ")"), ("/", "/")]);
    sym.push(end_row());
    sym.push(Key::new("=\\<", "=\\<", 2.0, KeyType::Layout).to(SYM2).scheme(1));
    chars(&mut sym, &[("*", "*"), ("\"", "\""), ("'", "'"), (":", ":"), (";", ";"), ("!", "!"), ("?", "?")]);
    sym.push(bksp());
    sym.push(end_row());
    sym.extend(bottom_row("ABC", ABC));

    let sp = |l: &str, code: u32| Key::new(l, l, 1.0, KeyType::Code).code(code).scheme(1);
    let md = |l: &str, m: u8| Key::new(l, l, 1.0, KeyType::Mod).code(m as u32).scheme(1);
    let mut sym2 = vec![sp("Esc", KEY_ESC), sp("Tab", KEY_TAB)];
    chars(&mut sym2, &[("~", "~"), ("`", "`"), ("|", "|"), ("\\", "\\"), ("{", "{"), ("}", "}"), ("[", "["), ("]", "]"), ("^", "^")]);
    sym2.push(end_row());
    sym2.extend([md("Ctrl", CTRL), md("Alt", ALT), md("Super", SUPER)]);
    sym2.extend([sp("←", KEY_LEFT), sp("↓", KEY_DOWN), sp("↑", KEY_UP), sp("→", KEY_RIGHT), sp("Home", KEY_HOME), sp("End", KEY_END), sp("PgUp", KEY_PAGEUP), sp("PgDn", KEY_PAGEDOWN)]);
    sym2.push(end_row());
    sym2.push(Key::new("?123", "?123", 2.0, KeyType::Layout).to(SYM).scheme(1));
    chars(&mut sym2, &[("<", "<"), (">", ">"), ("=", "="), ("£", "£"), ("§", "§"), ("½", "½"), ("¤", "¤")]);
    sym2.push(bksp());
    sym2.push(end_row());
    sym2.extend(bottom_row("ABC", ABC));

    vec![abc, sym, sym2]
}

pub fn layer_by_name(name: &str) -> Option<usize> {
    ["abc", "sym", "sym2"].iter().position(|n| *n == name)
}

/// What the keyboard sends to the compositor.
pub trait Output {
    fn key(&mut self, time: u32, key: u32, state: u32);
    fn modifiers(&mut self, depressed: u32, latched: u32, locked: u32, group: u32);
    fn keymap(&mut self, text: &str);
    /// the hide key was pressed
    fn hide(&mut self) {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceMode {
    Tap,
    Cursor,
    Lang,
}

/// a key: (key set, index)
pub type KeyRef = (usize, usize);

/// monotonic milliseconds
pub fn monotime() -> i64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as i64 * 1000 + ts.tv_nsec as i64 / 1_000_000
}

pub struct Kbd {
    pub debug: bool,
    pub show_popup: bool,
    pub show_highlight: bool,

    /// 0: the built-in letter layout, 1: ?123, 2: =\<, 3..: languages
    pub sets: Vec<Vec<Key>>,
    pub layouts: Vec<Layout>,
    pub layout: usize,
    pub schemes: Vec<Scheme>,

    pub w: u32,
    pub h: u32,
    pub scale: f64,
    pub corner_radius: i32,
    /// canvases draw into images of their own (tests) instead of waiting for
    /// a buffer from the compositor
    pub own_images: bool,
    pub landscape: bool,
    pub mods: u8,
    pub last_press: Option<KeyRef>,
    pub prevlayout: usize,
    pub layer_index: usize,
    pub last_abc_layout: usize,
    pub last_abc_index: usize,
    pub last_press_offset: i64,
    pub layers: Vec<usize>,
    pub landscape_layers: Vec<usize>,

    pub surf: Option<Canvas>,
    pub popup_surf: Option<Canvas>,
    last_popup: (u32, u32, u32, u32),

    pub xkb_layout: String,
    pub xkb_variant: String,
    pub xkb: Option<Xkb>,

    // gesture typing
    pub dict: Dict,
    pub swipe_enabled: bool,
    pub private_mode: bool,
    pub down_key: Option<KeyRef>,
    pub swiping: bool,
    pts_x: Vec<f64>,
    pts_y: Vec<f64>,
    pub trail: Color,

    // suggestion strip and text tracking
    pub suggest_h: u32,
    pub sugg_pressed: i32,
    pub sugg: Results,
    sugg_replace: bool,
    pub word: String,
    committed: String,
    after_commit: bool,
    commit_swiped: bool,
    after_space: bool,
    last_shift_tap: i64,
    autocap_latched: bool,

    // long-press alternates
    pub longpress_at: i64,
    pub alt_open: bool,
    pub alt_cps: Vec<u32>,
    alt_sel: usize,
    alt_x: u32,
    alt_y: u32,
    alt_w: u32,
    alt_h: u32,
    hint_font: Option<pango::FontDescription>,

    // languages, each with its own letter layout
    pub lang_list: Option<String>,
    pub data_dirs: Option<String>,
    pub langs: Vec<Lang>,
    common: Common,
    lang_sets: Vec<usize>,
    pub cur_lang: usize,

    // space bar gestures
    pub space_mode: SpaceMode,
    lang_dir: i32,
    held_mods: u8,
    last_typed: i64,
}

fn key_is_space(k: &Key) -> bool {
    k.ktype == KeyType::Code && k.code == KEY_SPACE
}

fn key_is_letter(k: &Key) -> bool {
    k.ktype == KeyType::Char && k.letter
}

fn capitalize(s: &str, all: bool) -> String {
    let cps = swipe::utf8_decode(s.as_bytes(), swipe::MAX_CPS);
    let mut out = String::new();
    for (i, &cp) in cps.iter().enumerate() {
        if out.len() >= swipe::MAX_WORD - 5 {
            break;
        }
        unicode::push_cp(&mut out, if all || i == 0 { unicode::upper(cp) } else { cp });
    }
    out
}

fn starts_upper(s: &str) -> bool {
    match s.chars().next() {
        Some(c) => unicode::upper(c as u32) == c as u32 && unicode::lower(c as u32) != c as u32,
        None => false,
    }
}

fn lang_state_path() -> String {
    match std::env::var("XDG_STATE_HOME") {
        Ok(s) if !s.is_empty() => format!("{s}/omarchy-osk/language"),
        _ => format!("{}/.local/state/omarchy-osk/language", std::env::var("HOME").unwrap_or_default()),
    }
}

fn utf8_len(s: &str) -> usize {
    s.chars().count()
}

/// slot order on screen: left, middle, right; the middle one is the
/// primary result (index 0)
const SLOT_TO_INDEX: [usize; 3] = [1, 0, 2];

impl Kbd {
    pub fn new(schemes: Vec<Scheme>) -> Kbd {
        Kbd {
            debug: false,
            show_popup: true,
            show_highlight: true,
            sets: builtin_sets(),
            layouts: vec![
                Layout { set: 0, name: "abc", abc: true, keyheight: 0 },
                Layout { set: 1, name: "sym", abc: false, keyheight: 0 },
                Layout { set: 2, name: "sym2", abc: false, keyheight: 0 },
            ],
            layout: ABC,
            trail: schemes[0].swipe,
            schemes,
            w: 0,
            h: 0,
            scale: 1.0,
            corner_radius: 0,
            own_images: true,
            landscape: true,
            mods: 0,
            last_press: None,
            prevlayout: ABC,
            layer_index: 0,
            last_abc_layout: ABC,
            last_abc_index: 0,
            last_press_offset: 0,
            layers: vec![ABC, SYM],
            landscape_layers: vec![ABC, SYM],
            surf: None,
            popup_surf: None,
            last_popup: (0, 0, 0, 0),
            xkb_layout: "fi".into(),
            xkb_variant: String::new(),
            xkb: None,
            dict: Dict::default(),
            swipe_enabled: false,
            private_mode: false,
            down_key: None,
            swiping: false,
            pts_x: Vec::new(),
            pts_y: Vec::new(),
            suggest_h: 0,
            sugg_pressed: -1,
            sugg: Results::default(),
            sugg_replace: false,
            word: String::new(),
            committed: String::new(),
            after_commit: false,
            commit_swiped: false,
            after_space: false,
            last_shift_tap: 0,
            autocap_latched: false,
            longpress_at: 0,
            alt_open: false,
            alt_cps: Vec::new(),
            alt_sel: 0,
            alt_x: 0,
            alt_y: 0,
            alt_w: 0,
            alt_h: 0,
            hint_font: None,
            lang_list: None,
            data_dirs: None,
            langs: Vec::new(),
            common: Common::default(),
            lang_sets: Vec::new(),
            cur_lang: 0,
            space_mode: SpaceMode::Tap,
            lang_dir: 1,
            held_mods: 0,
            last_typed: 0,
        }
    }

    pub fn key(&self, r: KeyRef) -> &Key {
        &self.sets[r.0][r.1]
    }

    fn layout_set(&self, l: usize) -> usize {
        self.layouts[l].set
    }

    /* ------------------------------------------------------------------ */
    /* typing through the xkb keymap                                       */

    /// touch $XDG_RUNTIME_DIR/omarchy-osk-typed whenever text is sent, so
    /// the auto-show daemon can tell our own typing apart from a focus change
    fn mark_typed(&mut self) {
        let now = monotime();
        if now - self.last_typed < 100 {
            return;
        }
        self.last_typed = now;
        let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
        let path = format!("{dir}/omarchy-osk-typed");
        use std::os::unix::fs::OpenOptionsExt;
        if let Ok(f) = std::fs::OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(&path) {
            let now = std::time::SystemTime::now();
            let _ = f.set_times(std::fs::FileTimes::new().set_accessed(now).set_modified(now));
        }
    }

    fn key_mods(o: &mut dyn Output, time: u32, mods: u8, state: u32) {
        if mods & CTRL != 0 {
            o.key(time, KEY_LEFTCTRL, state);
        }
        if mods & ALT != 0 {
            o.key(time, KEY_LEFTALT, state);
        }
        if mods & SUPER != 0 {
            o.key(time, KEY_LEFTMETA, state);
        }
    }

    /// Press one key with the given modifiers. Ctrl/Alt/Super are only held
    /// for the duration of this key, so the compositor never sees a
    /// modifier that stays down while the keyboard waits (or hides).
    fn tap_code(&mut self, o: &mut dyn Output, time: u32, code: u32, mods: u32) {
        self.mark_typed();
        Self::key_mods(o, time, mods as u8, WL_PRESSED);
        o.modifiers(mods, 0, 0, 0);
        o.key(time, code, WL_PRESSED);
        o.key(time, code, WL_RELEASED);
        Self::key_mods(o, time, mods as u8, WL_RELEASED);
        o.modifiers(0, 0, 0, 0);
    }

    fn type_cp(&mut self, o: &mut dyn Output, time: u32, cp: u32, extra_mods: u32) {
        let Some((kc, mask)) = self.xkb.as_mut().and_then(|x| x.lookup(cp)) else {
            eprintln!("omarchy-osk: U+{cp:04X} is not in the keymap");
            return;
        };
        self.tap_code(o, time, kc - 8, mask | extra_mods);
    }

    fn type_utf8(&mut self, o: &mut dyn Output, time: u32, s: &str) {
        for cp in swipe::utf8_decode(s.as_bytes(), swipe::MAX_CPS * 2) {
            self.type_cp(o, time, cp, 0);
        }
    }

    fn backspace(&mut self, o: &mut dyn Output, time: u32, count: usize) {
        for _ in 0..count {
            self.tap_code(o, time, KEY_BACKSPACE, 0);
        }
    }

    /* ------------------------------------------------------------------ */
    /* layouts                                                             */

    pub fn switch_layout(&mut self, l: usize, layer_index: usize) {
        self.prevlayout = self.layout;
        if self.layer_index != self.last_abc_index && self.layouts[self.layout].abc {
            self.last_abc_layout = self.layout;
            self.last_abc_index = self.layer_index;
        }
        self.layer_index = layer_index;
        self.layout = l;
        if self.debug {
            eprintln!("Switching to layout {}, layer_index {}", self.layouts[l].name, layer_index);
        }
        self.draw_layout();
    }

    fn next_layer(&mut self, invert: bool) {
        let layers = if self.landscape { &self.landscape_layers } else { &self.layers };
        let count = layers.len();
        let mut i = self.layer_index;
        if invert {
            i = if i > 0 { i - 1 } else { count - 1 };
        } else {
            i = (i + 1) % count;
        }
        let l = layers[i];
        self.switch_layout(l, i);
    }

    pub fn get_rows(&self, l: usize) -> u32 {
        let set = &self.sets[self.layout_set(l)];
        set.iter().take_while(|k| k.ktype != KeyType::Last).filter(|k| k.ktype == KeyType::EndRow).count() as u32 + 1
    }

    fn row_length(keys: &[Key], from: usize) -> f64 {
        keys[from..].iter().take_while(|k| k.ktype != KeyType::Last && k.ktype != KeyType::EndRow).map(|k| k.width).sum()
    }

    pub fn init_layout(&mut self, l: usize, width: u32, height: u32, yoff: u32) {
        let rows = self.get_rows(l);
        let keyheight = (height.wrapping_sub(yoff)) / rows;
        self.layouts[l].keyheight = keyheight;
        let set = self.layout_set(l);
        let keys = &mut self.sets[set];
        let (mut x, mut y) = (0u32, yoff);
        let mut rowlength = Self::row_length(keys, 0);
        let mut rowwidth = 0.0;
        let mut i = 0;
        while keys[i].ktype != KeyType::Last {
            if keys[i].ktype == KeyType::EndRow {
                y += keyheight;
                x = 0;
                rowwidth = 0.0;
                rowlength = Self::row_length(keys, i + 1);
            } else if keys[i].width > 0.0 {
                let k = &mut keys[i];
                k.x = x;
                k.y = y;
                k.w = ((width as f64 / rowlength) * k.width) as u32;
                x += k.w;
                rowwidth += k.width;
                if (x as f64) < (rowwidth / rowlength) * width as f64 {
                    k.w += 1;
                    x += 1;
                }
            }
            keys[i].h = keyheight;
            i += 1;
        }
    }

    pub fn get_key(&self, x: u32, y: u32) -> Option<KeyRef> {
        let set = self.layout_set(self.layout);
        let keys = &self.sets[set];
        let mut i = 0;
        while keys[i].ktype != KeyType::Last {
            let k = &keys[i];
            if k.ktype != KeyType::EndRow && x >= k.x && y >= k.y && x < k.x.wrapping_add(k.w) && y < k.y.wrapping_add(k.h) {
                if k.ktype != KeyType::Pad {
                    return Some((set, i));
                }
                // the gap beside a centred row belongs to its outer key
                let next = &keys[i + 1];
                if !matches!(next.ktype, KeyType::EndRow | KeyType::Last | KeyType::Pad) {
                    return Some((set, i + 1));
                }
                if i > 0 && !matches!(keys[i - 1].ktype, KeyType::EndRow | KeyType::Pad) {
                    return Some((set, i - 1));
                }
                return None;
            }
            i += 1;
        }
        None
    }

    /* ------------------------------------------------------------------ */
    /* classic key handling (modifiers, special keys)                      */

    fn shifted(&self) -> bool {
        self.mods & SHIFT != 0 || self.mods & CAPSLOCK != 0
    }

    /// drop one-shot modifiers after a key was typed
    fn unlatch(&mut self) {
        let oneshot = self.mods & (SHIFT | CTRL | ALT | SUPER);
        if oneshot == 0 {
            return;
        }
        self.mods &= !oneshot;
        self.autocap_latched = false;
        self.draw_layout();
    }

    fn autocap(&mut self) {
        if !self.private_mode && self.mods & CAPSLOCK == 0 && self.layouts[self.layout].abc {
            self.mods |= SHIFT;
            self.autocap_latched = true;
            self.draw_layout();
        }
    }

    /// password entry: plain typing, no suggestions, gestures, automatic
    /// capitals or smart punctuation, and nothing of the text is kept
    pub fn set_private(&mut self, on: bool) {
        if self.private_mode == on {
            return;
        }
        self.private_mode = on;
        if self.autocap_latched {
            self.mods &= !SHIFT;
            self.autocap_latched = false;
        }
        self.reset_text();
        self.draw_layout();
    }

    /// forget latched modifiers (caps lock stays)
    pub fn clear_mods(&mut self, o: &mut dyn Output) {
        self.mods &= CAPSLOCK;
        self.autocap_latched = false;
        o.modifiers(0, 0, 0, 0);
    }

    pub fn unpress_key(&mut self, o: &mut dyn Output, time: u32) {
        let Some(r) = self.last_press.take() else { return };
        let k = self.key(r);
        if k.ktype == KeyType::Code {
            o.key(time, k.code, WL_RELEASED);
            Self::key_mods(o, time, self.held_mods, WL_RELEASED);
            o.modifiers(0, 0, 0, 0);
            self.held_mods = 0;
            if self.mods & (SHIFT | CTRL | ALT | SUPER) != 0 {
                self.unlatch();
                return;
            }
        }
        self.draw_key(r, Draw::Unpress);
    }

    pub fn release_key(&mut self, o: &mut dyn Output, time: u32) {
        self.unpress_key(o, time);
        self.clear_last_popup();
    }

    fn motion_key(&mut self, o: &mut dyn Output, time: u32, x: u32, y: u32) {
        let k = self.get_key(x, y);
        if self.last_press.is_some() && self.last_press != k {
            self.unpress_key(o, time);
            self.clear_last_popup();
        }
    }

    fn set_suggestions(&mut self, r: Option<Results>, replace: bool) {
        self.sugg = r.unwrap_or_default();
        self.sugg_replace = replace;
        self.draw_suggestions();
    }

    pub fn reset_text(&mut self) {
        self.word.clear();
        self.committed.clear();
        self.after_commit = false;
        self.after_space = false;
        if !self.sugg.words.is_empty() {
            self.set_suggestions(None, false);
        }
    }

    fn press_key(&mut self, o: &mut dyn Output, r: KeyRef, time: u32) {
        let k = self.key(r).clone();
        match k.ktype {
            KeyType::Code => {
                if k.code == KEY_BACKSPACE {
                    if self.after_commit && !self.committed.is_empty() {
                        if self.commit_swiped {
                            // like Gboard: backspace right after a gesture
                            // removes the whole word
                            let n = utf8_len(&self.committed) + 1;
                            self.backspace(o, time, n);
                            self.reset_text();
                        } else {
                            // after a picked suggestion only the added space
                            // goes, so the word can be continued
                            let w = self.committed.clone();
                            self.backspace(o, time, 1);
                            self.reset_text();
                            self.word = w;
                            self.update_completions();
                        }
                        self.draw_key(r, Draw::Press);
                        self.last_press = None;
                        self.draw_key(r, Draw::Unpress);
                        return;
                    }
                    if !self.word.is_empty() {
                        self.word.pop();
                        self.update_completions();
                    } else {
                        self.reset_text();
                    }
                    self.after_commit = false;
                    self.after_space = false;
                } else {
                    self.reset_text();
                }
                if self.autocap_latched {
                    self.mods &= !SHIFT;
                    self.autocap_latched = false;
                    self.draw_layout();
                }
                // shift only modifies navigation keys, never Enter/Backspace
                let mut code_mods = self.mods & !CAPSLOCK;
                if k.code == KEY_ENTER || k.code == KEY_BACKSPACE {
                    code_mods &= !SHIFT;
                }
                self.held_mods = code_mods;
                Self::key_mods(o, time, code_mods, WL_PRESSED);
                o.modifiers(code_mods as u32, 0, 0, 0);
                self.last_press = Some(r);
                self.last_press_offset = time as i64 - monotime();
                self.draw_key(r, Draw::Press);
                self.mark_typed();
                o.key(time, k.code, WL_PRESSED);
            }
            KeyType::Mod => {
                if k.code == SHIFT as u32 {
                    let now = monotime();
                    if self.mods & CAPSLOCK != 0 {
                        self.mods &= !(CAPSLOCK | SHIFT);
                    } else if self.mods & SHIFT != 0 && now - self.last_shift_tap < SHIFT_DOUBLE_TAP_MS {
                        self.mods = (self.mods & !SHIFT) | CAPSLOCK;
                    } else {
                        self.mods ^= SHIFT;
                    }
                    self.last_shift_tap = now;
                    self.autocap_latched = false;
                    self.draw_layout();
                    return;
                }
                // Ctrl/Alt/Super latch for the next key, nothing is sent yet.
                // The automatic capital is not part of a shortcut
                if self.autocap_latched {
                    self.mods &= !SHIFT;
                    self.autocap_latched = false;
                    self.draw_layout();
                }
                self.mods ^= k.code as u8;
                let on = self.mods & k.code as u8 != 0;
                self.draw_key(r, if on { Draw::Press } else { Draw::Unpress });
            }
            KeyType::Layout => {
                let li = self.get_layer_index(k.layout);
                self.switch_layout(k.layout, li);
            }
            KeyType::NextLayer => self.next_layer(false),
            KeyType::BackLayer => {
                let (l, i) = (self.last_abc_layout, self.last_abc_index);
                self.switch_layout(l, i);
            }
            KeyType::Hide => {
                self.reset_text();
                o.hide();
            }
            _ => {}
        }
    }

    fn get_layer_index(&self, l: usize) -> usize {
        // as kbd_get_layer_index: the position in the layout array
        l
    }

    /* ------------------------------------------------------------------ */
    /* text input: taps, gestures and suggestions                          */

    fn commit_word(&mut self, o: &mut dyn Output, time: u32, w: &str, replace_word: bool) {
        if replace_word {
            let n = utf8_len(&self.word);
            self.backspace(o, time, n);
        }
        self.type_utf8(o, time, w);
        self.type_utf8(o, time, " ");
        self.committed = truncate_bytes(w, swipe::MAX_WORD - 1);
        self.word.clear();
        self.after_commit = true;
        self.after_space = false;
    }

    /* languages */

    /// long-press keys of a key, for the active language
    fn key_alts(&mut self, r: KeyRef) {
        let k = &self.sets[r.0][r.1];
        if k.ktype != KeyType::Char {
            self.sets[r.0][r.1].alts.clear();
            return;
        }
        let l = &self.langs[self.cur_lang];
        let alts = langs::alternates(
            &self.common,
            if l.rows.is_empty() { None } else { Some(l) },
            &k.label,
            if k.lang_row > 0 { Some(k.lang_row as usize - 1) } else { None },
            k.lang_col as usize,
            MAX_ALTS,
        );
        self.sets[r.0][r.1].alts = alts;
    }

    /// The letter layout of a language: its rows, centred, with Shift and
    /// Backspace around the last one (both from the built-in layout, as is
    /// the bottom row).
    fn build_lang_keys(l: &Lang, tmpl: &[Key]) -> Vec<Key> {
        let mut shift = None;
        let mut bksp = None;
        let mut bottom = 0;
        let mut i = 0;
        while tmpl[i].ktype != KeyType::Last {
            let k = &tmpl[i];
            if k.ktype == KeyType::Mod && k.code == SHIFT as u32 && shift.is_none() {
                shift = Some(k);
            }
            if k.ktype == KeyType::Code && k.code == KEY_BACKSPACE && bksp.is_none() {
                bksp = Some(k);
            }
            if k.ktype == KeyType::EndRow {
                bottom = i + 1;
            }
            i += 1;
        }
        let last = l.rows.len() - 1;
        let mut units = (l.rows[last].len() + 2) as f64;
        for r in 0..last {
            units = units.max(l.rows[r].len() as f64);
        }
        let side = (units - l.rows[last].len() as f64) / 2.0;

        let mut keys = Vec::new();
        for (r, row) in l.rows.iter().enumerate() {
            let pad = (units - row.len() as f64) / 2.0;
            if r == last && shift.is_some() {
                let s = shift.unwrap();
                keys.push(Key::new(&s.label, &s.shift_label, side, KeyType::Mod).code(SHIFT as u32).scheme(s.scheme));
            } else if pad > 0.01 {
                keys.push(Key::new("", "", pad, KeyType::Pad));
            }
            for (i, lk) in row.iter().enumerate() {
                let letter = unicode::single(&lk.label).is_some_and(unicode::is_letter);
                let mut k = Key::new(&lk.label, &lk.shift, 1.0, KeyType::Char);
                k.letter = letter;
                k.lang_row = r as u8 + 1;
                k.lang_col = i as u8;
                keys.push(k);
            }
            if r == last && bksp.is_some() {
                let b = bksp.unwrap();
                keys.push(Key::new(&b.label, &b.shift_label, side, KeyType::Code).code(KEY_BACKSPACE).scheme(b.scheme));
            } else if pad > 0.01 {
                keys.push(Key::new("", "", pad, KeyType::Pad));
            }
            keys.push(end_row());
        }
        keys.extend(tmpl[bottom..=i].iter().cloned());
        keys
    }

    fn set_lang(&mut self, i: usize) {
        self.cur_lang = i;
        self.layouts[ABC].set = self.lang_sets[i];
        let code = self.langs[i].code.clone();
        self.dict.set_lang_code(&code);
        for l in 0..NUM_LAYOUTS {
            let set = self.layouts[l].set;
            for k in 0..self.sets[set].len() {
                if self.sets[set][k].ktype == KeyType::Last {
                    break;
                }
                self.key_alts((set, k));
            }
        }
        if self.w != 0 {
            let (w, h, s) = (self.w, self.h, self.suggest_h);
            self.init_layout(ABC, w, h, s);
        }
    }

    /// pick the remembered language, or the first one
    fn load_lang(&mut self) {
        let mut pick = 0;
        if let Ok(s) = std::fs::read_to_string(lang_state_path()) {
            if let Some(code) = s.split_whitespace().next() {
                let code: String = code.chars().take(15).collect();
                if let Some(i) = self.langs.iter().position(|l| l.code == code) {
                    pick = i;
                }
            }
        }
        self.set_lang(pick);
    }

    /// Languages from --langs and the layout data. Without data the
    /// built-in layout is used for every dictionary.
    fn init_langs(&mut self) {
        for set in 0..NUM_LAYOUTS {
            for k in self.sets[set].iter_mut() {
                k.letter = k.ktype == KeyType::Char && k.label != k.shift_label && unicode::single(&k.label).is_some_and(unicode::is_letter);
            }
        }
        if let Some(dirs) = &self.data_dirs {
            self.common = langs::load_common(dirs);
            if let Some(list) = &self.lang_list {
                self.langs = langs::load(list, dirs);
            }
        }
        if self.langs.is_empty() {
            if self.lang_list.is_some() {
                eprintln!("omarchy-osk: no layout data, using the built-in layout");
            }
            let n = self.dict.lang_count().max(1);
            self.langs = (0..n)
                .map(|i| {
                    let code = if self.dict.lang_count() > 0 { self.dict.lang_code(i).to_string() } else { "xkb".into() };
                    Lang { name: code.clone(), code, ..Default::default() }
                })
                .collect();
        }
        let tmpl = self.sets[0].clone();
        self.lang_sets.clear();
        for i in 0..self.langs.len() {
            if self.langs[i].rows.is_empty() {
                self.lang_sets.push(0);
            } else {
                let keys = Self::build_lang_keys(&self.langs[i], &tmpl);
                self.sets.push(keys);
                self.lang_sets.push(self.sets.len() - 1);
            }
            if self.debug {
                let l = &self.langs[i];
                eprintln!(
                    "omarchy-osk: language {} ({}), layout {}",
                    l.code,
                    l.name,
                    if l.rows.is_empty() { "built-in" } else { &l.layout }
                );
            }
        }
        self.load_lang();
    }

    fn switch_lang(&mut self, dir: i32) {
        let n = self.langs.len() as i32;
        if n < 2 {
            return;
        }
        let next = ((self.cur_lang as i32 + dir + n) % n) as usize;
        self.set_lang(next);
        self.reset_text();
        self.draw_layout();
        let path = lang_state_path();
        if let Some(dir) = std::path::Path::new(&path).parent() {
            let _ = std::fs::create_dir(dir);
        }
        let _ = std::fs::write(&path, format!("{}\n", self.langs[self.cur_lang].code));
    }

    fn tap_space(&mut self, o: &mut dyn Output, time: u32) {
        if self.mods & (CTRL | ALT | SUPER) != 0 {
            // a shortcut such as Super+Space, never smart spacing
            let m = (self.mods & (SHIFT | CTRL | ALT | SUPER)) as u32;
            self.type_cp(o, time, ' ' as u32, m);
            self.reset_text();
            self.unlatch();
            return;
        }
        if self.private_mode {
            self.type_cp(o, time, ' ' as u32, 0);
            return;
        }
        if self.after_commit {
            // the gesture already added a space
            self.after_commit = false;
            self.after_space = true;
            self.set_suggestions(None, false);
            return;
        }
        if self.after_space {
            // double space: end the sentence
            self.backspace(o, time, 1);
            self.type_utf8(o, time, ". ");
            self.after_space = false;
            self.word.clear();
            self.autocap();
            return;
        }
        self.after_space = !self.word.is_empty();
        self.word.clear();
        self.type_cp(o, time, ' ' as u32, 0);
        self.set_suggestions(None, false);
    }

    /// completions for the letters typed so far, keeping their capitalisation
    fn update_completions(&mut self) {
        if self.word.is_empty() {
            self.set_suggestions(None, false);
            return;
        }
        let mut lower = String::new();
        for cp in swipe::utf8_decode(self.word.as_bytes(), swipe::MAX_CPS) {
            unicode::push_cp(&mut lower, unicode::lower(cp));
        }
        let mut r = self.dict.complete(&lower);
        if starts_upper(&self.word) {
            for w in r.words.iter_mut() {
                *w = capitalize(w, false);
            }
        }
        self.set_suggestions(Some(r), false);
    }

    fn tap_cp(&mut self, o: &mut dyn Output, time: u32, cp: u32, letter: bool) {
        if self.private_mode {
            // exactly what the key shows, nothing remembered
            let m = (self.mods & (CTRL | ALT | SUPER)) as u32;
            self.type_cp(o, time, cp, m);
            if self.mods & (SHIFT | CTRL | ALT | SUPER) != 0 {
                self.unlatch();
            }
            return;
        }
        let sentence_end = cp == '.' as u32 || cp == '?' as u32 || cp == '!' as u32;
        if (self.after_commit || self.after_space) && !letter && ".,?!:;".chars().any(|c| c as u32 == cp) {
            // punctuation hugs the previous word
            self.backspace(o, time, 1);
            self.type_cp(o, time, cp, 0);
            self.type_utf8(o, time, " ");
            self.reset_text();
            if sentence_end {
                self.autocap();
            }
            return;
        }
        let m = (self.mods & (CTRL | ALT | SUPER)) as u32;
        self.type_cp(o, time, cp, m);
        let had_ctrl = self.mods & (CTRL | ALT | SUPER) != 0;
        self.after_commit = false;
        self.after_space = false;

        if letter && !had_ctrl {
            if self.word.len() + 5 < swipe::MAX_WORD {
                unicode::push_cp(&mut self.word, cp);
            }
            self.update_completions();
        } else {
            self.reset_text();
        }
        if self.mods & (SHIFT | CTRL | ALT | SUPER) != 0 {
            self.unlatch();
        }
        if sentence_end {
            self.autocap();
        }
    }

    fn tap_char(&mut self, o: &mut dyn Output, r: KeyRef, time: u32) {
        let k = self.key(r);
        let text = if self.shifted() { k.shift_label.clone() } else { k.label.clone() };
        let letter = key_is_letter(k);
        let cps = swipe::utf8_decode(text.as_bytes(), 2);
        if cps.len() == 1 {
            self.tap_cp(o, time, cps[0], letter);
        } else if cps.len() > 1 {
            // a letter with combining marks, or several characters
            self.type_utf8(o, time, &text);
            self.reset_text();
            if self.mods & (SHIFT | CTRL | ALT | SUPER) != 0 {
                self.unlatch();
            }
        }
    }

    /// load the fonts and the glyphs of every label now (when idle after
    /// startup), so the first time the keyboard is shown it draws at once
    pub fn warm_up(&self) {
        let Ok(surf) = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1) else { return };
        let Ok(cr) = cairo::Context::new(&surf) else { return };
        let layout = pangocairo::functions::create_layout(&cr);
        let mut labels: Vec<String> = Vec::new();
        for set in &self.sets {
            for k in set {
                labels.push(k.label.clone());
                labels.push(k.shift_label.clone());
            }
        }
        for l in &self.langs {
            labels.push(format!("‹  {}  ›", l.name));
        }
        labels.push("0123456789 abcdefghijklmnopqrstuvwxyzåäö".into());
        let text = labels.join(" ");
        let mut fonts: Vec<pango::FontDescription> = self.schemes.iter().filter_map(|s| s.font_desc.clone()).collect();
        if let Some(f) = self.schemes[0].font_desc.as_ref() {
            let mut h = f.clone();
            h.set_size((f.size() as f64 * 0.5) as i32);
            fonts.push(h);
        }
        for f in &fonts {
            layout.set_font_description(Some(f));
            layout.set_text(&text);
            pangocairo::functions::show_layout(&cr, &layout);
        }
    }

    /// get the gesture decoder ready for the letter layout (when idle)
    pub fn prepare_swipe(&self) {
        if self.swipe_enabled && self.w != 0 && self.dict.lang_active() {
            self.dict.prepare(&self.swipe_geom_of(ABC));
        }
    }

    fn swipe_geom(&self) -> Geom {
        self.swipe_geom_of(self.layout)
    }

    fn swipe_geom_of(&self, layout: usize) -> Geom {
        let mut g = Geom::default();
        let set = self.layout_set(layout);
        for k in self.sets[set].iter().take_while(|k| k.ktype != KeyType::Last) {
            if !key_is_letter(k) || g.keys.len() >= swipe::MAX_KEYS {
                continue;
            }
            let Some(c) = k.label.chars().next() else { continue };
            g.keys.push(swipe::Key { cp: c as u32, cx: k.x as f64 + k.w as f64 / 2.0, cy: k.y as f64 + k.h as f64 / 2.0 });
            if g.kw == 0.0 {
                g.kw = k.w as f64;
                g.kh = k.h as f64;
            }
        }
        g
    }

    fn finish_swipe(&mut self, o: &mut dyn Output, time: u32) {
        let g = self.swipe_geom();
        let mut r = self.dict.decode(&g, &self.pts_x, &self.pts_y);
        if self.debug {
            for (i, w) in r.words.iter().enumerate() {
                eprintln!("swipe candidate {i}: {w}");
            }
        }
        if r.words.is_empty() {
            return;
        }
        let caps = self.mods & CAPSLOCK != 0;
        let shift = self.mods & SHIFT != 0;
        for w in r.words.iter_mut() {
            if caps {
                *w = capitalize(w, true);
            } else if shift || w == "i" || w.starts_with("i'") {
                *w = capitalize(w, false);
            }
        }
        // a gesture right after tapped letters starts a new word
        if !self.word.is_empty() {
            self.type_utf8(o, time, " ");
        }
        let first = r.words[0].clone();
        self.commit_word(o, time, &first, false);
        self.commit_swiped = true;
        self.set_suggestions(Some(r), true);
        if shift {
            self.unlatch();
        }
    }

    fn apply_suggestion(&mut self, o: &mut dyn Output, time: u32, slot: usize) {
        let idx = SLOT_TO_INDEX[slot];
        if idx >= self.sugg.words.len() {
            return;
        }
        let chosen = self.sugg.words[idx].clone();
        if self.sugg_replace {
            if !self.after_commit || chosen == self.committed {
                return;
            }
            let n = utf8_len(&self.committed) + 1;
            self.backspace(o, time, n);
            self.commit_word(o, time, &chosen, false);
            // keep the alternatives, with the chosen one as primary
            self.sugg.words.swap(0, idx);
            self.draw_suggestions();
        } else {
            self.commit_word(o, time, &chosen, true);
            self.commit_swiped = false;
            self.set_suggestions(Some(Results { words: vec![chosen] }), true);
        }
        if self.mods & SHIFT != 0 {
            self.unlatch();
        }
    }

    fn add_point(&mut self, x: f64, y: f64) {
        self.pts_x.push(x);
        self.pts_y.push(y);
    }

    fn trail_width(&self) -> f64 {
        match self.down_key {
            Some(r) => self.key(r).w as f64 * 0.14,
            None => 8.0,
        }
    }

    fn draw_trail_segment(&mut self, i: usize) {
        if i < 1 {
            return;
        }
        let width = self.trail_width();
        let (h, trail) = (self.h as f64, self.trail);
        let (x0, y0, x1, y1) = (self.pts_x[i - 1], self.pts_y[i - 1] + h, self.pts_x[i], self.pts_y[i] + h);
        if let Some(p) = self.popup_surf.as_mut() {
            p.line(trail, x0, y0, x1, y1, width);
        }
    }

    fn clear_trail(&mut self) {
        let (w, h) = (self.w, self.h);
        if let Some(p) = self.popup_surf.as_mut() {
            p.clear(0, h, w, h);
        }
    }

    fn draw_alternates(&mut self) {
        let pad = KEY_BORDER;
        let (s0, s2) = (self.schemes[0].clone(), self.schemes[2].clone());
        let font = s0.font_desc.clone().unwrap_or_default();
        let (ax, ay, aw, ah, n, sel) = (self.alt_x, self.alt_y, self.alt_w, self.alt_h, self.alt_cps.len() as u32, self.alt_sel);
        let cps = self.alt_cps.clone();
        let Some(p) = self.popup_surf.as_mut() else { return };
        p.fill_rectangle(s0.bg, ax.wrapping_sub(pad), ay.wrapping_sub(pad), aw * n + pad * 2, ah + pad * 2, s0.rounding);
        for (i, &cp) in cps.iter().enumerate() {
            let label = unicode::from_cp(cp);
            let on = i == sel;
            let x = ax + i as u32 * aw;
            draw_inset(p, x, ay, aw, ah, KEY_BORDER, if on { s2.fg } else { s0.fg }, s0.rounding);
            p.draw_text(if on { s2.text } else { s0.text }, x, ay, aw, ah, KEY_BORDER, &label, &font);
        }
    }

    fn close_alternates(&mut self) {
        let pad = KEY_BORDER;
        let (ax, ay, aw, ah, n) = (self.alt_x, self.alt_y, self.alt_w, self.alt_h, self.alt_cps.len() as u32);
        if let Some(p) = self.popup_surf.as_mut() {
            p.clear(ax.wrapping_sub(pad), ay.wrapping_sub(pad), aw * n + pad * 2, ah + pad * 2);
        }
        self.alt_open = false;
    }

    pub fn longpress(&mut self) {
        self.longpress_at = 0;
        let dk = self.down_key;
        if let Some(r) = dk {
            if key_is_space(self.key(r)) && self.space_mode == SpaceMode::Tap {
                self.space_mode = SpaceMode::Cursor;
                self.pts_x[0] = *self.pts_x.last().unwrap();
                self.clear_last_popup();
                return;
            }
        }
        if self.debug {
            eprintln!("{} longpress key={} swiping={}", monotime(), dk.map(|r| self.key(r).label.as_str()).unwrap_or("-"), self.swiping as i32);
        }
        let Some(r) = dk else { return };
        if self.key(r).alts.is_empty() || self.swiping {
            return;
        }
        let k = self.key(r);
        let upper = key_is_letter(k) && self.shifted();
        let fit = if k.w != 0 { (self.w / k.w) as usize } else { 1 };
        let n = k.alts.len().min(fit);
        let cps: Vec<u32> = k.alts[..n].iter().map(|&c| if upper { unicode::upper(c) } else { c }).collect();
        let (kx, ky, kw, kh) = (k.x, k.y, k.w, k.h);
        self.alt_cps = cps;
        self.alt_sel = 0;
        self.alt_w = kw;
        self.alt_h = kh;
        let mut x = kx as i32;
        if x + (self.alt_w * n as u32) as i32 > self.w as i32 {
            x = self.w as i32 - (self.alt_w * n as u32) as i32;
        }
        self.alt_x = if x < KEY_BORDER as i32 { KEY_BORDER } else { x as u32 };
        self.alt_y = self.h + ky - kh;
        self.alt_open = true;
        self.clear_last_popup();
        self.draw_alternates();
    }

    pub fn touch_down(&mut self, o: &mut dyn Output, time: u32, x: i32, y: i32) {
        if self.debug {
            eprintln!("{} down {},{}", monotime(), x as u32, y as u32);
        }
        self.unpress_key(o, time);
        if self.alt_open {
            self.close_alternates();
        }
        if x < 0 || y < 0 {
            return;
        }
        if (y as u32) < self.suggest_h {
            self.sugg_pressed = (x as i64 * SUGG_SLOTS as i64 / self.w as i64) as i32;
            self.draw_suggestions();
            return;
        }
        let Some(r) = self.get_key(x as u32, y as u32) else { return };
        let k = self.key(r);
        if k.ktype == KeyType::Char || key_is_space(k) {
            // typed on release, so the touch can still become a gesture
            let longpress = !k.alts.is_empty() || key_is_space(k);
            self.down_key = Some(r);
            self.swiping = false;
            self.pts_x.clear();
            self.pts_y.clear();
            self.add_point(x as f64, y as f64);
            self.draw_key(r, Draw::Press);
            self.space_mode = SpaceMode::Tap;
            self.lang_dir = 1;
            if longpress {
                self.longpress_at = monotime() + LONGPRESS_MS;
            }
            return;
        }
        self.press_key(o, r, time);
    }

    pub fn touch_motion(&mut self, o: &mut dyn Output, time: u32, x: i32, y: i32) {
        if self.sugg_pressed >= 0 {
            return;
        }
        let Some(dk) = self.down_key else {
            if x >= 0 && y >= 0 {
                self.motion_key(o, time, x as u32, y as u32);
            } else {
                self.motion_key(o, time, u32::MAX, u32::MAX);
            }
            return;
        };
        if self.alt_open {
            let sel = (x - self.alt_x as i32) / self.alt_w as i32;
            let sel = sel.clamp(0, self.alt_cps.len() as i32 - 1) as usize;
            if sel != self.alt_sel {
                self.alt_sel = sel;
                self.draw_alternates();
            }
            return;
        }
        let dx = x as f64 - self.pts_x[self.pts_x.len() - 1];
        let dy = y as f64 - self.pts_y[self.pts_y.len() - 1];
        if dx * dx + dy * dy < 4.0 {
            return;
        }
        self.add_point(x as f64, y as f64);
        let k = self.key(dk).clone();

        if key_is_space(&k) {
            let moved = x as f64 - self.pts_x[0];
            if self.space_mode == SpaceMode::Tap {
                // a quick swipe across the space bar switches language
                if moved.abs() > k.w as f64 / 6.0 {
                    self.space_mode = SpaceMode::Lang;
                    self.longpress_at = 0;
                    self.lang_dir = if moved < 0.0 { -1 } else { 1 };
                }
                return;
            }
            if self.space_mode == SpaceMode::Lang {
                self.lang_dir = if moved < 0.0 { -1 } else { 1 };
                return;
            }
            // after a long press, sliding moves the cursor
            let step = k.w as f64 / 14.0;
            if moved.abs() >= step {
                let n = (moved / step) as i32;
                for _ in 0..n.abs() {
                    let m = self.mods & !(CAPSLOCK | if self.autocap_latched { SHIFT } else { 0 });
                    self.tap_code(o, time, if n > 0 { KEY_RIGHT } else { KEY_LEFT }, m as u32);
                }
                self.pts_x[0] += n as f64 * step;
                self.reset_text();
            }
            return;
        }

        if !self.swiping {
            let over = if x < 0 || y < 0 { None } else { self.get_key(x as u32, y as u32) };
            let fx = x as f64 - self.pts_x[0];
            let fy = y as f64 - self.pts_y[0];
            if self.swipe_enabled
                && self.dict.lang_active()
                && !self.private_mode
                && self.layouts[self.layout].abc
                && key_is_letter(&k)
                && over != Some(dk)
                && fx * fx + fy * fy > k.w as f64 * k.w as f64 * 0.36
            {
                self.swiping = true;
                self.longpress_at = 0;
                self.clear_last_popup();
                self.draw_key(dk, Draw::Unpress);
                for i in 1..self.pts_x.len() {
                    self.draw_trail_segment(i);
                }
            }
            return;
        }
        let n = self.pts_x.len();
        self.draw_trail_segment(n - 1);
    }

    pub fn touch_up(&mut self, o: &mut dyn Output, time: u32) {
        if self.debug {
            eprintln!("{} up alt_open={} swiping={}", monotime(), self.alt_open as i32, self.swiping as i32);
        }
        if self.sugg_pressed >= 0 {
            let slot = self.sugg_pressed as usize;
            self.sugg_pressed = -1;
            if slot < SUGG_SLOTS as usize {
                self.apply_suggestion(o, time, slot);
            }
            self.draw_suggestions();
            return;
        }
        let Some(dk) = self.down_key.take() else {
            self.release_key(o, time);
            return;
        };
        self.longpress_at = 0;
        self.clear_last_popup();

        if self.alt_open {
            let cp = self.alt_cps[self.alt_sel];
            self.close_alternates();
            self.draw_key(dk, Draw::Unpress);
            // accented letters continue the current word
            self.tap_cp(o, time, cp, unicode::is_letter(cp));
            self.swiping = false;
            return;
        }
        if key_is_space(self.key(dk)) {
            self.draw_key(dk, Draw::Unpress);
            if self.space_mode == SpaceMode::Lang {
                let d = self.lang_dir;
                self.switch_lang(d);
            } else if self.space_mode == SpaceMode::Tap {
                self.tap_space(o, time);
            }
            self.space_mode = SpaceMode::Tap;
        } else if self.swiping {
            self.clear_trail();
            self.finish_swipe(o, time);
        } else {
            self.draw_key(dk, Draw::Unpress);
            self.tap_char(o, dk, time);
        }
        self.swiping = false;
    }

    /// the compositor took the touch away (e.g. a gesture): drop everything
    /// without typing
    pub fn touch_cancel(&mut self, o: &mut dyn Output, time: u32) {
        self.unpress_key(o, time);
        if self.alt_open {
            self.close_alternates();
        }
        if self.swiping && self.down_key.is_some_and(|r| !key_is_space(self.key(r))) {
            self.clear_trail();
        }
        if let Some(r) = self.down_key {
            self.draw_key(r, Draw::Unpress);
        }
        self.down_key = None;
        self.swiping = false;
        self.longpress_at = 0;
        self.space_mode = SpaceMode::Tap;
        self.sugg_pressed = -1;
        self.clear_last_popup();
        self.draw_suggestions();
    }

    /// what hiding does to the keyboard state (main.c hide())
    pub fn on_hide(&mut self, o: &mut dyn Output) {
        let time = (monotime() + self.last_press_offset) as u32;
        self.unpress_key(o, time);
        self.down_key = None;
        self.swiping = false;
        self.space_mode = SpaceMode::Tap;
        self.longpress_at = 0;
        self.alt_open = false;
        self.clear_mods(o);
        self.sugg_pressed = -1;
        self.reset_text();
    }

    /* ------------------------------------------------------------------ */
    /* drawing                                                             */

    pub fn clear_last_popup(&mut self) {
        let (x, y, w, h) = self.last_popup;
        if w != 0 && h != 0 {
            if let Some(p) = self.popup_surf.as_mut() {
                p.clear(x, y, w, h);
            }
            self.last_popup.2 = 0;
            self.last_popup.3 = 0;
        }
    }

    fn key_label(&self, k: &Key) -> String {
        if k.ktype == KeyType::Mod && k.code == SHIFT as u32 {
            return if self.mods & CAPSLOCK != 0 { k.shift_label.clone() } else { k.label.clone() };
        }
        if k.ktype == KeyType::Char {
            return if self.shifted() { k.shift_label.clone() } else { k.label.clone() };
        }
        if key_is_space(k) && self.langs.len() > 1 {
            let s = format!("‹  {}  ›", self.langs[self.cur_lang].name);
            return truncate_bytes(&s, 79);
        }
        k.label.clone()
    }

    pub fn draw_key(&mut self, r: KeyRef, t: Draw) {
        if self.surf.is_none() {
            return;
        }
        let k = self.key(r).clone();
        let label = self.key_label(&k);
        let scheme = self.schemes[k.scheme as usize].clone();
        let (mut fill, mut text) = (scheme.fg, scheme.text);
        if t == Draw::Press && self.show_highlight {
            fill = scheme.high;
            text = scheme.text_press;
        }
        let font = scheme.font_desc.clone().unwrap_or_default();
        let space_label = key_is_space(&k) && self.hint_font.is_some();
        if space_label {
            text[3] = 170;
        }
        let hint_font = self.hint_font.clone();
        let hint = if !k.alts.is_empty() && hint_font.is_some() && k.lang_row == 1 && (k.lang_col as usize) < 10 {
            let l = &self.langs[self.cur_lang];
            (k.alts[0] == l.number_row[k.lang_col as usize]).then(|| unicode::from_cp(k.alts[0]))
        } else {
            None
        };
        {
            let s = self.surf.as_mut().unwrap();
            s.fill_rectangle(scheme.bg, k.x, k.y, k.w, k.h, 0);
            draw_inset(s, k.x, k.y, k.w, k.h, KEY_BORDER, fill, scheme.rounding);
            s.draw_text(text, k.x, k.y, k.w, k.h, KEY_BORDER, &label, if space_label { hint_font.as_ref().unwrap() } else { &font });
            // the digit on long press of the top row
            if let Some(h) = hint {
                let mut dim = text;
                dim[3] = 140;
                s.draw_text(
                    dim,
                    (k.x as f64 + k.w as f64 * 0.62) as u32,
                    k.y + KEY_BORDER,
                    (k.w as f64 * 0.38 - KEY_BORDER as f64) as u32,
                    (k.h as f64 * 0.4) as u32,
                    0,
                    &h,
                    hint_font.as_ref().unwrap(),
                );
            }
        }
        // magnified preview above the finger for character keys
        if self.show_popup && k.ktype == KeyType::Char && (t == Draw::Press || t == Draw::Unpress) {
            self.clear_last_popup();
            if t == Draw::Unpress {
                return;
            }
            let py = self.h + k.y - k.h;
            self.last_popup = (k.x, py, k.w, k.h);
            if let Some(p) = self.popup_surf.as_mut() {
                draw_inset(p, k.x, py, k.w, k.h, KEY_BORDER, scheme.high, scheme.rounding);
                p.draw_text(scheme.text_press, k.x, py, k.w, k.h, KEY_BORDER, &label, &font);
            }
        }
    }

    pub fn draw_suggestions(&mut self) {
        if self.suggest_h == 0 || self.surf.is_none() || self.w == 0 {
            return;
        }
        let (s0, s1, s2) = (self.schemes[0].clone(), self.schemes[1].clone(), self.schemes[2].clone());
        let font = s0.font_desc.clone().unwrap_or_default();
        let (h, w) = (self.suggest_h, self.w);
        let slot_w = w / SUGG_SLOTS;
        let private = self.private_mode;
        let words = self.sugg.words.clone();
        let pressed = self.sugg_pressed;
        let replace_done = self.sugg_replace && !self.after_commit;
        let s = self.surf.as_mut().unwrap();
        s.fill_rectangle(s0.bg, 0, 0, w, h, 0);
        if private {
            s.draw_text(s0.text, 0, 0, w, h, KEY_BORDER, "󰌾  private: no suggestions", &font);
            return;
        }
        for slot in 0..SUGG_SLOTS {
            let idx = SLOT_TO_INDEX[slot as usize];
            let x = slot * slot_w;
            if idx >= words.len() {
                continue;
            }
            if pressed == slot as i32 {
                draw_inset(s, x, 0, slot_w, h, KEY_BORDER, s0.fg, s0.rounding);
            }
            let primary = idx == 0 && !replace_done;
            s.draw_text(if primary { s2.fg } else { s0.text }, x, 0, slot_w, h, KEY_BORDER, &words[idx], &font);
            if slot > 0 {
                s.fill_rectangle(s1.fg, x, h / 4, 2, h / 2, 0);
            }
        }
    }

    pub fn draw_layout(&mut self) {
        if self.surf.is_none() {
            return;
        }
        let (w, h) = (self.w, self.h);
        let (bg, radius) = (self.schemes[0].bg, self.corner_radius);
        let s = self.surf.as_mut().unwrap();
        if radius > 0 {
            s.clear(0, 0, w, h);
            s.fill_rectangle(bg, 0, 0, w, h, radius);
        } else {
            s.fill_rectangle(bg, 0, 0, w, h, 0);
        }
        self.draw_suggestions();
        let set = self.layout_set(self.layout);
        let mut i = 0;
        while self.sets[set][i].ktype != KeyType::Last {
            let k = &self.sets[set][i];
            if k.ktype == KeyType::Pad || k.ktype == KeyType::EndRow {
                i += 1;
                continue;
            }
            let active = k.ktype == KeyType::Mod && (self.mods & k.code as u8 != 0 || (k.code == SHIFT as u32 && self.mods & CAPSLOCK != 0));
            self.draw_key((set, i), if active { Draw::Press } else { Draw::None });
            i += 1;
        }
    }

    pub fn resize(&mut self) {
        eprintln!("Resize {}x{} {:.6}, {} layouts", self.w, self.h, self.scale, NUM_LAYOUTS);
        self.surf = Some(Canvas::new(self.w, self.h, self.scale, self.own_images));
        self.popup_surf = Some(Canvas::new(self.w, self.h * 2, self.scale, self.own_images));
        if self.hint_font.is_none() {
            if let Some(f) = &self.schemes[0].font_desc {
                let mut h = f.clone();
                h.set_size((f.size() as f64 * 0.5) as i32);
                self.hint_font = Some(h);
            }
        }
        let rows = self.get_rows(ABC);
        self.suggest_h = if self.swipe_enabled { (self.h as f64 / (rows as f64 + 0.7) * 0.7) as u32 } else { 0 };
        let (w, h, s) = (self.w, self.h, self.suggest_h);
        for l in 0..NUM_LAYOUTS {
            self.init_layout(l, w, h, s);
        }
        self.draw_layout();
    }

    /* ------------------------------------------------------------------ */
    /* setup                                                               */

    /// layers from -l / --landscape-layers ("abc,sym")
    pub fn parse_layers(list: &str) -> Vec<usize> {
        let mut out = Vec::new();
        for s in list.split(',').filter(|s| !s.is_empty()) {
            match layer_by_name(s) {
                Some(l) => out.push(l),
                None => {
                    eprintln!("No such layer: {s}");
                    std::process::exit(3);
                }
            }
        }
        if out.is_empty() {
            eprintln!("No layers defined");
            std::process::exit(3);
        }
        out
    }

    pub fn init(&mut self, o: &mut dyn Output) {
        self.layer_index = 0;
        self.last_abc_index = 0;
        self.sugg_pressed = -1;
        let layer = if self.landscape { self.landscape_layers[0] } else { self.layers[0] };
        self.layout = layer;
        self.last_abc_layout = layer;
        self.init_langs();
        self.create_and_upload_keymap(o);
    }

    /// every character of every language (keys, shifted keys and
    /// long-press keys with their upper case) for the keymap
    fn keymap_chars(&mut self) -> (Vec<u32>, Vec<u32>) {
        let mut cps: Vec<u32> = Vec::new();
        let add = |cps: &mut Vec<u32>, cp: u32| {
            if !cps.contains(&cp) && cps.len() < 2048 {
                cps.push(cp);
            }
        };
        let cur = self.cur_lang;
        for li in 0..self.langs.len() {
            self.set_lang(li);
            for l in 0..NUM_LAYOUTS {
                let set = self.layouts[l].set;
                for k in self.sets[set].iter().take_while(|k| k.ktype != KeyType::Last) {
                    if k.ktype != KeyType::Char {
                        continue;
                    }
                    for cp in swipe::utf8_decode(k.label.as_bytes(), 32) {
                        add(&mut cps, cp);
                    }
                    for cp in swipe::utf8_decode(k.shift_label.as_bytes(), 32) {
                        add(&mut cps, cp);
                    }
                    for &a in &k.alts {
                        add(&mut cps, a);
                        add(&mut cps, unicode::upper(a));
                    }
                }
            }
        }
        self.set_lang(cur);
        let mut codes = Vec::new();
        for l in 0..NUM_LAYOUTS {
            let set = self.layouts[l].set;
            for k in self.sets[set].iter().take_while(|k| k.ktype != KeyType::Last) {
                if k.ktype == KeyType::Code {
                    codes.push(k.code);
                }
            }
        }
        (cps, codes)
    }

    /// compile the keymap from the xkb names and hand it to the compositor;
    /// Char keys look their keycodes up in it
    fn create_and_upload_keymap(&mut self, o: &mut dyn Output) {
        if self.xkb.is_some() {
            return;
        }
        self.xkb = Some(Xkb::new(&self.xkb_layout, &self.xkb_variant));
        let (cps, codes) = self.keymap_chars();
        let debug = self.debug;
        let xkb = self.xkb.as_mut().unwrap();
        xkb.extend(&cps, &codes, debug);
        o.keymap(&xkb.text);
        eprintln!("omarchy-osk: using xkb layout {}", self.xkb_layout);
    }
}

pub fn draw_inset(c: &mut Canvas, x: u32, y: u32, w: u32, h: u32, border: u32, color: Color, rounding: i32) {
    c.fill_rectangle(color, x + border, y + border, w.wrapping_sub(border * 2), h.wrapping_sub(border * 2), rounding);
}

fn truncate_bytes(s: &str, max: usize) -> String {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}
