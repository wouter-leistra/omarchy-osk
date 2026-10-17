//! Gesture ("swipe") typing decoder.
//!
//! The finger path is resampled and compared against the ideal path through
//! the key centres of every plausible dictionary word (same start and end
//! key region). Shape distance is combined with word frequency, which is
//! roughly how SHARK2-style decoders used by Gboard/SwiftKey started out.

use std::fs::File;
use std::io::{BufRead, BufReader};

pub const MAX_WORD: usize = 48; // bytes of utf-8 per word
pub const MAX_CPS: usize = 32; // codepoints per word
pub const MAX_KEYS: usize = 64;
pub const MAX_RESULTS: usize = 3;
const MAX_LANGS: usize = 8;

const RESAMPLE: usize = 48;
const SHAPE_WEIGHT: f64 = 9.0; // nats per key-width of mean path deviation
const LEN_WEIGHT: f64 = 2.5; // penalty for path length mismatch
const END_RADIUS: f64 = 1.15; // start/end key must be within this many keys
const PASS_RADIUS: f64 = 0.9; // every letter must be passed this closely

/// geometry of a letter key, in surface pixels
#[derive(Clone, Copy, Debug)]
pub struct Key {
    /// lowercase codepoint the key types
    pub cp: u32,
    pub cx: f64,
    pub cy: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Geom {
    pub keys: Vec<Key>,
    /// typical letter key width/height
    pub kw: f64,
    pub kh: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Results {
    pub words: Vec<String>,
}

/// Sorted with glibc's qsort in the layout of the C version's word struct,
/// so words of equal frequency keep the order they always had.
#[repr(C)]
#[derive(Clone, Copy)]
struct Word {
    utf8: [u8; MAX_WORD],
    cps: [u32; MAX_CPS],
    ncps: u8,
    lp: f32, // log relative frequency, <= 0
    lang: u8,
}

impl Word {
    fn text(&self) -> &[u8] {
        let n = self.utf8.iter().position(|&b| b == 0).unwrap_or(MAX_WORD);
        &self.utf8[..n]
    }
}

const NO_LANG: i32 = -2;

pub struct Dict {
    words: Vec<Word>,
    lang_codes: Vec<String>,
    /// -1: all languages, NO_LANG: none
    active: i32,
}

/// UTF-8 decoding that also accepts invalid input, as the keyboard always did
pub fn utf8_decode(s: &[u8], max: usize) -> Vec<u32> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() && s[i] != 0 && out.len() < max {
        let b = s[i];
        let (mut c, len) = if b < 0x80 {
            (b as u32, 1)
        } else if b & 0xE0 == 0xC0 {
            ((b & 0x1F) as u32, 2)
        } else if b & 0xF0 == 0xE0 {
            ((b & 0x0F) as u32, 3)
        } else {
            ((b & 0x07) as u32, 4)
        };
        for k in 1..len {
            if i + k >= s.len() || s[i + k] == 0 {
                return out;
            }
            c = (c << 6) | (s[i + k] & 0x3F) as u32;
        }
        i += len;
        out.push(c);
    }
    out
}

/// sscanf "%lf": the numeric prefix
fn parse_count(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    let mut end = 0;
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut seen_exp = false;
    while end < b.len() {
        let c = b[end];
        if c.is_ascii_digit() {
            seen_digit = true;
        } else if (c == b'+' || c == b'-') && (end == 0 || matches!(b[end - 1], b'e' | b'E')) {
        } else if c == b'.' && !seen_dot && !seen_exp {
            seen_dot = true;
        } else if (c == b'e' || c == b'E') && seen_digit && !seen_exp {
            seen_exp = true;
        } else {
            break;
        }
        end += 1;
    }
    while end > 0 && !s[..end].parse::<f64>().is_ok() {
        end -= 1;
    }
    if end == 0 {
        None
    } else {
        s[..end].parse().ok()
    }
}

extern "C" fn cmp_alpha(a: *const libc::c_void, b: *const libc::c_void) -> libc::c_int {
    let (x, y) = unsafe { (&*(a as *const Word), &*(b as *const Word)) };
    let c = unsafe { libc::strcmp(x.utf8.as_ptr() as *const libc::c_char, y.utf8.as_ptr() as *const libc::c_char) };
    if c != 0 {
        return c;
    }
    if x.lang != y.lang {
        return x.lang as i32 - y.lang as i32;
    }
    (x.lp < y.lp) as i32 - (x.lp > y.lp) as i32
}

extern "C" fn cmp_freq(a: *const libc::c_void, b: *const libc::c_void) -> libc::c_int {
    let (x, y) = unsafe { (&*(a as *const Word), &*(b as *const Word)) };
    (x.lp < y.lp) as i32 - (x.lp > y.lp) as i32
}

fn qsort(words: &mut [Word], cmp: extern "C" fn(*const libc::c_void, *const libc::c_void) -> libc::c_int) {
    unsafe {
        libc::qsort(words.as_mut_ptr() as *mut libc::c_void, words.len(), std::mem::size_of::<Word>(), Some(cmp));
    }
}

/// resample a polyline to RESAMPLE evenly spaced points, returns length
fn resample(xs: &[f64], ys: &[f64], ox: &mut [f64; RESAMPLE], oy: &mut [f64; RESAMPLE]) -> f64 {
    let n = xs.len();
    let mut len = 0.0;
    for i in 1..n {
        len += (xs[i] - xs[i - 1]).hypot(ys[i] - ys[i - 1]);
    }
    if n == 1 || len == 0.0 {
        ox.fill(xs[0]);
        oy.fill(ys[0]);
        return 0.0;
    }
    let step = len / (RESAMPLE - 1) as f64;
    let mut acc = 0.0;
    let (mut seg, mut k) = (1, 1);
    ox[0] = xs[0];
    oy[0] = ys[0];
    let (mut px, mut py) = (xs[0], ys[0]);
    while k < RESAMPLE - 1 && seg < n {
        let d = (xs[seg] - px).hypot(ys[seg] - py);
        if acc + d >= step && d > 0.0 {
            let t = (step - acc) / d;
            px += t * (xs[seg] - px);
            py += t * (ys[seg] - py);
            ox[k] = px;
            oy[k] = py;
            k += 1;
            acc = 0.0;
        } else {
            acc += d;
            px = xs[seg];
            py = ys[seg];
            seg += 1;
        }
    }
    while k < RESAMPLE {
        ox[k] = xs[n - 1];
        oy[k] = ys[n - 1];
        k += 1;
    }
    len
}

fn insert_result(out: &mut Vec<Vec<u8>>, costs: &mut [f64; MAX_RESULTS], w: &[u8], cost: f64) {
    if out.iter().any(|o| o == w) {
        return;
    }
    let mut pos = out.len();
    while pos > 0 && costs[pos - 1] > cost {
        pos -= 1;
    }
    if pos >= MAX_RESULTS {
        return;
    }
    if out.len() == MAX_RESULTS {
        out.pop();
    }
    let mut i = out.len();
    while i > pos {
        costs[i] = costs[i - 1];
        i -= 1;
    }
    out.insert(pos, w.to_vec());
    costs[pos] = cost;
}

fn to_results(words: Vec<Vec<u8>>) -> Results {
    Results { words: words.into_iter().map(|w| String::from_utf8_lossy(&w).into_owned()).collect() }
}

impl Default for Dict {
    fn default() -> Self {
        Dict { words: Vec::new(), lang_codes: Vec::new(), active: -1 }
    }
}

impl Dict {
    fn load_one(&mut self, path: &str, lang: u8) {
        let Ok(f) = File::open(path) else {
            eprintln!("omarchy-osk: cannot open dictionary {path}");
            return;
        };
        let mut maxcount = 0.0;
        for line in BufReader::new(f).split(b'\n') {
            let Ok(line) = line else { break };
            let line = String::from_utf8_lossy(&line);
            let mut it = line.split_ascii_whitespace();
            let (Some(w), Some(c)) = (it.next(), it.next()) else { continue };
            let Some(count) = parse_count(c) else { continue };
            if maxcount == 0.0 {
                maxcount = count;
            }
            let mut wd = Word { utf8: [0; MAX_WORD], cps: [0; MAX_CPS], ncps: 0, lp: 0.0, lang };
            let wb = &w.as_bytes()[..w.len().min(MAX_WORD - 1)];
            wd.utf8[..wb.len()].copy_from_slice(wb);
            let cps = utf8_decode(wb, MAX_CPS);
            wd.cps[..cps.len()].copy_from_slice(&cps);
            wd.ncps = cps.len() as u8;
            wd.lp = (count / maxcount).ln() as f32;
            self.words.push(wd);
        }
    }

    /// Load one or more "word count" frequency lists (comma separated paths).
    pub fn load(&mut self, paths: &str) -> usize {
        for p in paths.split(',').filter(|p| !p.is_empty()) {
            if self.lang_codes.len() >= MAX_LANGS {
                break;
            }
            // language code from the file name, e.g. ".../fi.txt" -> "fi"
            let base = p.rsplit('/').next().unwrap_or(p);
            let code: String = base.split('.').next().unwrap_or("").chars().take(15).collect();
            let lang = self.lang_codes.len() as u8;
            self.lang_codes.push(code);
            self.load_one(p, lang);
        }
        // words shared between languages keep their best frequency, and the
        // merged list is ordered by frequency for completions
        qsort(&mut self.words, cmp_alpha);
        let mut out: Vec<Word> = Vec::with_capacity(self.words.len());
        for w in &self.words {
            if out.last().is_none_or(|o| o.text() != w.text() || o.lang != w.lang) {
                out.push(*w);
            }
        }
        self.words = out;
        qsort(&mut self.words, cmp_freq);
        eprintln!("omarchy-osk: {} dictionary words", self.words.len());
        self.words.len()
    }

    pub fn loaded(&self) -> bool {
        !self.words.is_empty()
    }

    pub fn lang_count(&self) -> usize {
        self.lang_codes.len()
    }

    pub fn lang_code(&self, i: usize) -> &str {
        self.lang_codes.get(i).map(|s| s.as_str()).unwrap_or("")
    }

    /// use the dictionary of this language; false (and no words) if there
    /// is none. "de-CH" falls back to de.txt
    pub fn set_lang_code(&mut self, code: &str) -> bool {
        let base = code.find(['-', '_']).unwrap_or(code.len());
        for pass in 0..2 {
            for (i, c) in self.lang_codes.iter().enumerate() {
                let hit = if pass == 1 { c.as_bytes() == &code.as_bytes()[..base] } else { c == code };
                if hit {
                    self.active = i as i32;
                    return true;
                }
            }
        }
        self.active = if self.lang_codes.is_empty() { -1 } else { NO_LANG };
        self.lang_codes.is_empty()
    }

    /// a dictionary is in use: gestures and completions work
    pub fn lang_active(&self) -> bool {
        !self.words.is_empty() && self.active != NO_LANG
    }

    fn lang_ok(&self, w: &Word) -> bool {
        self.active == -1 || w.lang as i32 == self.active
    }

    /// Decode a gesture path into the most likely words.
    pub fn decode(&self, g: &Geom, xs: &[f64], ys: &[f64]) -> Results {
        let npts = xs.len();
        let mut out: Vec<Vec<u8>> = Vec::new();
        if self.words.is_empty() || npts < 2 || g.keys.is_empty() {
            return Results::default();
        }
        // work in key units so that horizontal and vertical error weigh alike
        let nx: Vec<f64> = xs.iter().map(|x| x / g.kw).collect();
        let ny: Vec<f64> = ys.iter().map(|y| y / g.kh).collect();
        let (mut px, mut py) = ([0.0; RESAMPLE], [0.0; RESAMPLE]);
        let plen = resample(&nx, &ny, &mut px, &mut py);

        let keys = &g.keys[..g.keys.len().min(MAX_KEYS)];
        let kx: Vec<f64> = keys.iter().map(|k| k.cx / g.kw).collect();
        let ky: Vec<f64> = keys.iter().map(|k| k.cy / g.kh).collect();
        let start_ok: Vec<bool> = (0..keys.len()).map(|i| (kx[i] - nx[0]).hypot(ky[i] - ny[0]) <= END_RADIUS).collect();
        let end_ok: Vec<bool> = (0..keys.len()).map(|i| (kx[i] - nx[npts - 1]).hypot(ky[i] - ny[npts - 1]) <= END_RADIUS).collect();

        let mut costs = [0.0; MAX_RESULTS];
        let (mut tx, mut ty) = ([0.0; MAX_CPS], [0.0; MAX_CPS]);
        let (mut rx, mut ry) = ([0.0; RESAMPLE], [0.0; RESAMPLE]);

        for wd in &self.words {
            if !self.lang_ok(wd) {
                continue;
            }
            // template: key centres of the letters, repeats collapsed,
            // apostrophes/hyphens are not swiped
            let (mut nt, mut first, mut last) = (0usize, None, None);
            let mut bad = false;
            for &cp in &wd.cps[..wd.ncps as usize] {
                if cp == '\'' as u32 || cp == '-' as u32 {
                    continue;
                }
                let Some(ki) = keys.iter().position(|k| k.cp == cp) else {
                    bad = true;
                    break;
                };
                if first.is_none() {
                    first = Some(ki);
                    if !start_ok[ki] {
                        bad = true;
                        break;
                    }
                }
                if last == Some(ki) {
                    continue;
                }
                last = Some(ki);
                tx[nt] = kx[ki];
                ty[nt] = ky[ki];
                nt += 1;
            }
            let Some(last) = last else { continue };
            if bad || nt < 1 || !end_ok[last] {
                continue;
            }
            if nt == 1 && plen > 1.2 {
                continue;
            }
            // every letter has to be passed, in order
            let mut pi = 0;
            for t in 0..nt {
                while pi < npts && (nx[pi] - tx[t]).hypot(ny[pi] - ty[t]) > PASS_RADIUS {
                    pi += 1;
                }
                if pi >= npts {
                    bad = true;
                    break;
                }
            }
            if bad {
                continue;
            }
            let tlen = resample(&tx[..nt], &ty[..nt], &mut rx, &mut ry);
            let mut d = 0.0;
            for i in 0..RESAMPLE {
                d += (rx[i] - px[i]).hypot(ry[i] - py[i]);
            }
            d /= RESAMPLE as f64;
            let lenpen = (plen - tlen).abs() / if plen > tlen { plen } else { tlen + 1e-6 };
            let cost = d * SHAPE_WEIGHT + lenpen * LEN_WEIGHT - wd.lp as f64;
            insert_result(&mut out, &mut costs, wd.text(), cost);
        }
        to_results(out)
    }

    /// Most frequent dictionary words starting with prefix (lowercase
    /// utf-8), excluding the prefix itself.
    pub fn complete(&self, prefix: &str) -> Results {
        let p = prefix.as_bytes();
        let mut out: Vec<Vec<u8>> = Vec::new();
        if p.is_empty() {
            return Results::default();
        }
        // the lists are sorted by frequency, so the first hits are the best
        for w in &self.words {
            if out.len() >= MAX_RESULTS {
                break;
            }
            let t = w.text();
            if !self.lang_ok(w) || t.len() <= p.len() || &t[..p.len()] != p {
                continue;
            }
            if !out.iter().any(|o| o == t) {
                out.push(t.to_vec());
            }
        }
        to_results(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // the offline check of tests/decode_test.c: a sloppy finger on a 2160px
    // wide fi layout
    #[test]
    fn decodes_finnish_words() {
        let mut d = Dict::default();
        d.load(&format!("{}/dict/fi.txt", env!("CARGO_MANIFEST_DIR")));
        let rows = ["qwertyuiopå", "asdfghjklöä", "zxcvbnm"];
        let offs = [0.0, 0.0, 2.0];
        let (w, unit, kh) = (2160.0, 2160.0 / 11.0, 120.0);
        let _ = w;
        let mut g = Geom { kw: unit, kh, ..Default::default() };
        for (r, row) in rows.iter().enumerate() {
            for (i, c) in row.chars().enumerate() {
                g.keys.push(Key { cp: c as u32, cx: (offs[r] + i as f64 + 0.5) * unit, cy: 84.0 + (r as f64 + 0.5) * kh });
            }
        }
        for word in ["kiitos", "talo", "koira", "tämä"] {
            let (mut xs, mut ys) = (Vec::new(), Vec::new());
            for c in word.chars() {
                let k = g.keys.iter().find(|k| k.cp == c as u32).unwrap();
                let (tx, ty) = (k.cx, k.cy);
                if let (Some(&px), Some(&py)) = (xs.last(), ys.last()) {
                    for s in 1..=12 {
                        xs.push(px + (tx - px) * s as f64 / 12.0);
                        ys.push(py + (ty - py) * s as f64 / 12.0);
                    }
                } else {
                    xs.push(tx);
                    ys.push(ty);
                }
            }
            let r = d.decode(&g, &xs, &ys);
            assert_eq!(r.words.first().map(|s| s.as_str()), Some(word), "{word}: {:?}", r.words);
        }
    }

    #[test]
    fn completes_by_frequency() {
        let mut d = Dict::default();
        d.load(&format!("{}/dict/en.txt", env!("CARGO_MANIFEST_DIR")));
        let r = d.complete("hel");
        assert_eq!(r.words.len(), 3);
        assert!(r.words.iter().all(|w| w.starts_with("hel") && w != "hel"));
    }
}
