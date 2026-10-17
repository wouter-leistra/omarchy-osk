//! Gesture ("swipe") typing decoder.
//!
//! The finger path is resampled and compared against the ideal path through
//! the key centres of every plausible dictionary word (same start and end
//! key region). Shape distance is combined with word frequency, which is
//! roughly how SHARK2-style decoders used by Gboard/SwiftKey started out.
//!
//! The dictionary is compact (12 bytes per word plus its text). For the
//! layout in use every word's key sequence is worked out once and the words
//! are grouped by their first key, so a gesture only looks at words that
//! start near where it started. Candidates are still visited in frequency
//! order, so the result is the same as checking every word.

use std::cell::RefCell;
use std::collections::HashMap;

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

/// a dictionary word: its text in the text buffer, frequency and language
/// (12 bytes, stored as is in the cache file)
#[repr(C)]
#[derive(Clone, Copy)]
struct Entry {
    off: u32,
    len: u8,
    lang: u8,
    lp: f32, // log relative frequency, <= 0
}

/// key sequences of the words of one language on one layout
struct Templates {
    keys: Vec<u32>, // the layout's key codepoints, in order
    active: i32,
    /// per word: offset into seq, number of keys (0: can't be swiped)
    at: Vec<(u32, u8)>,
    seq: Vec<u8>,
    /// word indices by first and last key (first * keys + last), in
    /// frequency order
    by_ends: Vec<Vec<u32>>,
}

const NO_LANG: i32 = -2;

/// the dictionary's arrays: built from the word lists, or mapped from the
/// cache file (clean pages the kernel can share and drop)
enum Store {
    Built {
        text: Vec<u8>,
        /// most frequent first
        words: Vec<Entry>,
        /// word indices in byte order of their text, for prefixes
        alpha: Vec<u32>,
    },
    Mapped {
        map: *const u8,
        len: usize,
        words: (usize, usize),
        alpha: (usize, usize),
        text: (usize, usize),
    },
}

impl Drop for Store {
    fn drop(&mut self) {
        if let Store::Mapped { map, len, .. } = self {
            unsafe { libc::munmap(*map as *mut libc::c_void, *len) };
        }
    }
}

pub struct Dict {
    store: Store,
    lang_codes: Vec<String>,
    /// -1: all languages, NO_LANG: none
    active: i32,
    templates: RefCell<Vec<Templates>>,
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

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

/// the number at the start (after blanks), as sscanf's %lf reads it
fn parse_count(s: &[u8]) -> Option<f64> {
    let s = &s[s.iter().position(|&b| !is_space(b))?..];
    let mut end = 0;
    if matches!(s.first(), Some(b'+' | b'-')) {
        end = 1;
    }
    let digits_start = end;
    while end < s.len() && s[end].is_ascii_digit() {
        end += 1;
    }
    if end < s.len() && s[end] == b'.' {
        end += 1;
        while end < s.len() && s[end].is_ascii_digit() {
            end += 1;
        }
    }
    if end == digits_start || (end == digits_start + 1 && s[digits_start] == b'.') {
        return None;
    }
    if end < s.len() && (s[end] == b'e' || s[end] == b'E') {
        let mut e = end + 1;
        if e < s.len() && (s[e] == b'+' || s[e] == b'-') {
            e += 1;
        }
        if e < s.len() && s[e].is_ascii_digit() {
            while e < s.len() && s[e].is_ascii_digit() {
                e += 1;
            }
            end = e;
        }
    }
    std::str::from_utf8(&s[..end]).ok()?.parse().ok()
}

/// "word count" as sscanf("%47s %lf") reads it
fn parse_line(line: &[u8]) -> Option<(&[u8], f64)> {
    let start = line.iter().position(|&b| !is_space(b))?;
    let rest = &line[start..];
    let token = rest.iter().position(|&b| is_space(b)).unwrap_or(rest.len());
    let wlen = token.min(MAX_WORD - 1);
    let count = parse_count(&rest[wlen..])?;
    Some((&rest[..wlen], count))
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

fn insert_result<'a>(out: &mut Vec<&'a [u8]>, costs: &mut [f64; MAX_RESULTS], w: &'a [u8], cost: f64) {
    if out.iter().any(|o| *o == w) {
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
    out.insert(pos, w);
    costs[pos] = cost;
}

fn to_results(words: Vec<&[u8]>) -> Results {
    Results { words: words.into_iter().map(|w| String::from_utf8_lossy(w).into_owned()).collect() }
}

impl Default for Dict {
    fn default() -> Self {
        Dict {
            store: Store::Built { text: Vec::new(), words: Vec::new(), alpha: Vec::new() },
            lang_codes: Vec::new(),
            active: -1,
            templates: RefCell::new(Vec::new()),
        }
    }
}

const CACHE_MAGIC: &[u8; 8] = b"OSKWRDS1";

/// FNV-1a, for the cache key
fn fnv(h: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(h, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn cache_dir() -> std::path::PathBuf {
    match std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        Some(d) => std::path::PathBuf::from(d).join("omarchy-osk"),
        None => std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache/omarchy-osk"),
    }
}

fn section<T>(base: *const u8, (off, n): (usize, usize)) -> &'static [T] {
    unsafe { std::slice::from_raw_parts(base.add(off) as *const T, n) }
}

impl Dict {
    fn text(&self) -> &[u8] {
        match &self.store {
            Store::Built { text, .. } => text,
            Store::Mapped { map, text, .. } => section(*map, *text),
        }
    }

    fn words(&self) -> &[Entry] {
        match &self.store {
            Store::Built { words, .. } => words,
            Store::Mapped { map, words, .. } => section(*map, *words),
        }
    }

    fn alpha(&self) -> &[u32] {
        match &self.store {
            Store::Built { alpha, .. } => alpha,
            Store::Mapped { map, alpha, .. } => section(*map, *alpha),
        }
    }

    fn word(&self, e: &Entry) -> &[u8] {
        &self.text()[e.off as usize..e.off as usize + e.len as usize]
    }

    /// the cache file for these word lists: their paths, sizes and times
    fn cache_path(paths: &[&str]) -> std::path::PathBuf {
        use std::os::unix::fs::MetadataExt;
        let mut h = fnv(0xcbf29ce484222325, CACHE_MAGIC);
        h = fnv(h, &(std::mem::size_of::<Entry>() as u32).to_le_bytes());
        for p in paths {
            h = fnv(h, p.as_bytes());
            if let Ok(m) = std::fs::metadata(p) {
                h = fnv(h, &m.len().to_le_bytes());
                h = fnv(h, &m.mtime().to_le_bytes());
                h = fnv(h, &m.mtime_nsec().to_le_bytes());
                h = fnv(h, &m.ino().to_le_bytes());
            }
            h = fnv(h, b"\0");
        }
        cache_dir().join(format!("words-{h:016x}.bin"))
    }

    /// map a cache file written by save_cache, if it is intact
    fn load_cache(&mut self, path: &std::path::Path) -> bool {
        let Ok(f) = std::fs::File::open(path) else { return false };
        let Ok(len) = f.metadata().map(|m| m.len() as usize) else { return false };
        if len < 32 {
            return false;
        }
        use std::os::fd::AsRawFd;
        let map = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_SHARED, f.as_raw_fd(), 0) };
        if map == libc::MAP_FAILED {
            return false;
        }
        let map = map as *const u8;
        let head = unsafe { std::slice::from_raw_parts(map, 32) };
        let u32_at = |i: usize| u32::from_le_bytes(head[i..i + 4].try_into().unwrap()) as usize;
        let (nlangs, nwords, ntext) = (u32_at(8), u32_at(12), u32_at(16));
        let words = 32 + nlangs * 16;
        let alpha = words + nwords * std::mem::size_of::<Entry>();
        let text = alpha + nwords * 4;
        if &head[..8] != CACHE_MAGIC || nlangs > MAX_LANGS || text + ntext != len {
            unsafe { libc::munmap(map as *mut libc::c_void, len) };
            return false;
        }
        let codes = unsafe { std::slice::from_raw_parts(map.add(32), nlangs * 16) };
        self.lang_codes = codes
            .chunks(16)
            .map(|c| String::from_utf8_lossy(&c[..c.iter().position(|&b| b == 0).unwrap_or(16)]).into_owned())
            .collect();
        self.store = Store::Mapped { map, len, words: (words, nwords), alpha: (alpha, nwords), text: (text, ntext) };
        true
    }

    fn save_cache(&self, path: &std::path::Path) {
        let mut out = Vec::with_capacity(32 + self.text().len() + self.words().len() * 16);
        out.extend_from_slice(CACHE_MAGIC);
        for n in [self.lang_codes.len(), self.words().len(), self.text().len(), 0] {
            out.extend_from_slice(&(n as u32).to_le_bytes());
        }
        out.extend_from_slice(&[0; 8]);
        for c in &self.lang_codes {
            let mut b = [0u8; 16];
            let n = c.len().min(15);
            b[..n].copy_from_slice(&c.as_bytes()[..n]);
            out.extend_from_slice(&b);
        }
        let words = self.words();
        out.extend_from_slice(unsafe { std::slice::from_raw_parts(words.as_ptr() as *const u8, std::mem::size_of_val(words)) });
        for a in self.alpha() {
            out.extend_from_slice(&a.to_le_bytes());
        }
        out.extend_from_slice(self.text());
        let Some(dir) = path.parent() else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        // older caches of other word lists go
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let n = e.file_name();
                let n = n.to_string_lossy();
                if n.starts_with("words-") && e.path() != path {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&tmp, &out).is_ok() && std::fs::rename(&tmp, path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }

    fn load_one(&mut self, path: &str, lang: u8, text: &mut Vec<u8>, out: &mut Vec<Entry>) {
        let Ok(data) = std::fs::read(path) else {
            eprintln!("omarchy-osk: cannot open dictionary {path}");
            return;
        };
        let mut maxcount = 0.0;
        let mut rest = &data[..];
        while !rest.is_empty() {
            // fgets with a 256 byte buffer: up to a newline or 255 bytes
            let n = rest.iter().take(255).position(|&b| b == b'\n').map(|i| i + 1).unwrap_or(rest.len().min(255));
            let (line, next) = rest.split_at(n);
            rest = next;
            let Some((w, count)) = parse_line(line) else { continue };
            if maxcount == 0.0 {
                maxcount = count;
            }
            out.push(Entry { off: text.len() as u32, len: w.len() as u8, lang, lp: (count / maxcount).ln() as f32 });
            text.extend_from_slice(w);
        }
    }

    /// Load one or more "word count" frequency lists (comma separated
    /// paths), from the cache when it was built from these same files.
    pub fn load(&mut self, paths: &str) -> usize {
        let list: Vec<&str> = paths.split(',').filter(|p| !p.is_empty()).take(MAX_LANGS).collect();
        let cache = std::env::var_os("OSK_NO_CACHE").is_none().then(|| Self::cache_path(&list));
        self.load_from(&list, cache.as_deref())
    }

    fn load_from(&mut self, list: &[&str], cache: Option<&std::path::Path>) -> usize {
        self.templates.borrow_mut().clear();
        let cached = cache.is_some_and(|c| self.load_cache(c));
        if !cached {
            self.build(list);
            if let Some(c) = cache {
                self.save_cache(c);
            }
        } else {
            for p in list {
                if std::fs::metadata(p).is_err() {
                    eprintln!("omarchy-osk: cannot open dictionary {p}");
                }
            }
        }
        eprintln!("omarchy-osk: {} dictionary words", self.words().len());
        self.words().len()
    }

    fn build(&mut self, list: &[&str]) {
        let (mut text, mut words) = (Vec::new(), Vec::new());
        self.lang_codes.clear();
        for p in list {
            // language code from the file name, e.g. ".../fi.txt" -> "fi"
            let base = p.rsplit('/').next().unwrap_or(p);
            let code: String = base.split('.').next().unwrap_or("").chars().take(15).collect();
            let lang = self.lang_codes.len() as u8;
            self.lang_codes.push(code);
            self.load_one(p, lang, &mut text, &mut words);
        }
        // words shared between languages keep their best frequency, and the
        // merged list is ordered by frequency (ties alphabetically) for
        // completions
        let word = |e: &Entry| &text[e.off as usize..e.off as usize + e.len as usize];
        let desc = |a: f32, b: f32| b.partial_cmp(&a).unwrap_or(std::cmp::Ordering::Equal);
        words.sort_unstable_by(|a, b| word(a).cmp(word(b)).then(a.lang.cmp(&b.lang)).then(desc(a.lp, b.lp)));
        words.dedup_by(|b, a| word(a) == word(b) && a.lang == b.lang);
        words.sort_by(|a, b| desc(a.lp, b.lp));
        let mut alpha: Vec<u32> = (0..words.len() as u32).collect();
        alpha.sort_unstable_by(|&a, &b| word(&words[a as usize]).cmp(word(&words[b as usize])).then(a.cmp(&b)));
        text.shrink_to_fit();
        words.shrink_to_fit();
        self.store = Store::Built { text, words, alpha };
    }

    pub fn loaded(&self) -> bool {
        !self.words().is_empty()
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
        !self.words().is_empty() && self.active != NO_LANG
    }

    fn lang_ok(&self, w: &Entry) -> bool {
        self.active == -1 || w.lang as i32 == self.active
    }

    /// key sequences of the active language's words on this layout:
    /// letters on the same key in a row count once, ' and - are not swiped
    fn build_templates(&self, keys: &[u32]) -> Templates {
        let mut index: HashMap<u32, u8> = HashMap::new();
        for (i, &cp) in keys.iter().enumerate().rev() {
            index.insert(cp, i as u8);
        }
        let mut t = Templates {
            keys: keys.to_vec(),
            active: self.active,
            at: Vec::with_capacity(self.words().len()),
            seq: Vec::new(),
            by_ends: vec![Vec::new(); keys.len() * keys.len()],
        };
        for (wi, e) in self.words().iter().enumerate() {
            let start = t.seq.len();
            let mut ok = self.lang_ok(e);
            let mut last = None;
            if ok {
                for cp in utf8_decode(self.word(e), MAX_CPS) {
                    if cp == '\'' as u32 || cp == '-' as u32 {
                        continue;
                    }
                    let Some(&ki) = index.get(&cp) else {
                        ok = false;
                        break;
                    };
                    if last != Some(ki) {
                        t.seq.push(ki);
                        last = Some(ki);
                    }
                }
            }
            let nt = t.seq.len() - start;
            if !ok || nt == 0 {
                t.seq.truncate(start);
                t.at.push((start as u32, 0));
                continue;
            }
            t.at.push((start as u32, nt as u8));
            let (first, last) = (t.seq[start] as usize, t.seq[start + nt - 1] as usize);
            t.by_ends[first * keys.len() + last].push(wi as u32);
        }
        t.seq.shrink_to_fit();
        t
    }

    /// the key sequences for this layout and language, built if needed
    fn templates_for(&self, keys: &[Key]) -> usize {
        let cps: Vec<u32> = keys.iter().map(|k| k.cp).collect();
        let mut cache = self.templates.borrow_mut();
        match cache.iter().position(|t| t.keys == cps && t.active == self.active) {
            Some(i) => i,
            None => {
                if cache.len() >= 2 {
                    cache.remove(0);
                }
                cache.push(self.build_templates(&cps));
                cache.len() - 1
            }
        }
    }

    /// get ready for gestures on this layout (done while idle, so the first
    /// gesture is as quick as the others)
    pub fn prepare(&self, g: &Geom) {
        if !self.words().is_empty() && !g.keys.is_empty() && self.active != NO_LANG {
            self.templates_for(&g.keys[..g.keys.len().min(MAX_KEYS)]);
        }
    }

    /// Decode a gesture path into the most likely words.
    pub fn decode(&self, g: &Geom, xs: &[f64], ys: &[f64]) -> Results {
        let npts = xs.len();
        if self.words().is_empty() || npts < 2 || g.keys.is_empty() {
            return Results::default();
        }
        let keys = &g.keys[..g.keys.len().min(MAX_KEYS)];
        let ti = self.templates_for(keys);
        let cache = self.templates.borrow();
        let t = &cache[ti];

        // work in key units so that horizontal and vertical error weigh alike
        let nx: Vec<f64> = xs.iter().map(|x| x / g.kw).collect();
        let ny: Vec<f64> = ys.iter().map(|y| y / g.kh).collect();
        let (mut px, mut py) = ([0.0; RESAMPLE], [0.0; RESAMPLE]);
        let plen = resample(&nx, &ny, &mut px, &mut py);
        let kx: Vec<f64> = keys.iter().map(|k| k.cx / g.kw).collect();
        let ky: Vec<f64> = keys.iter().map(|k| k.cy / g.kh).collect();
        let end_ok: Vec<bool> = (0..keys.len()).map(|i| (kx[i] - nx[npts - 1]).hypot(ky[i] - ny[npts - 1]) <= END_RADIUS).collect();

        // words starting and ending at keys near the ends of the path, in
        // frequency order
        let nk = keys.len();
        let mut cand: Vec<u32> = Vec::new();
        for f in (0..nk).filter(|&i| (kx[i] - nx[0]).hypot(ky[i] - ny[0]) <= END_RADIUS) {
            for l in (0..nk).filter(|&i| end_ok[i]) {
                cand.extend_from_slice(&t.by_ends[f * nk + l]);
            }
        }
        cand.sort_unstable();

        // path points within reach of each key, in path order: "every
        // letter has to be passed, in order" becomes a few searches in these
        let reach: Vec<Vec<u32>> = (0..keys.len())
            .map(|k| (0..npts as u32).filter(|&i| (nx[i as usize] - kx[k]).hypot(ny[i as usize] - ky[k]) <= PASS_RADIUS).collect())
            .collect();

        let mut out: Vec<&[u8]> = Vec::new();
        let mut costs = [0.0; MAX_RESULTS];
        let (mut tx, mut ty) = ([0.0; MAX_CPS], [0.0; MAX_CPS]);
        let (mut rx, mut ry) = ([0.0; RESAMPLE], [0.0; RESAMPLE]);
        'words: for &wi in &cand {
            let e = &self.words()[wi as usize];
            // a cost is never below the word's own -lp, and the candidates
            // come most frequent first: once that can't beat the third
            // result, no later word can either
            if out.len() == MAX_RESULTS && -(e.lp as f64) >= costs[MAX_RESULTS - 1] {
                break;
            }
            let (s, nt) = t.at[wi as usize];
            let nt = nt as usize;
            if nt == 1 && plen > 1.2 {
                continue;
            }
            let seq = &t.seq[s as usize..s as usize + nt];
            // every letter has to be passed, in order: the first point near
            // each key at or after the point that passed the previous one
            let mut pi = 0u32;
            for &ki in seq {
                let r = &reach[ki as usize];
                match r.get(r.partition_point(|&i| i < pi)) {
                    Some(&i) => pi = i,
                    None => continue 'words,
                }
            }
            for (j, &ki) in seq.iter().enumerate() {
                tx[j] = kx[ki as usize];
                ty[j] = ky[ki as usize];
            }
            let tlen = resample(&tx[..nt], &ty[..nt], &mut rx, &mut ry);
            let mut d = 0.0;
            for i in 0..RESAMPLE {
                d += (rx[i] - px[i]).hypot(ry[i] - py[i]);
            }
            d /= RESAMPLE as f64;
            let lenpen = (plen - tlen).abs() / if plen > tlen { plen } else { tlen + 1e-6 };
            let cost = d * SHAPE_WEIGHT + lenpen * LEN_WEIGHT - e.lp as f64;
            insert_result(&mut out, &mut costs, self.word(e), cost);
        }
        to_results(out)
    }

    fn completes(&self, e: &Entry, p: &[u8]) -> bool {
        let w = self.word(e);
        self.lang_ok(e) && w.len() > p.len() && w.starts_with(p)
    }

    /// Most frequent dictionary words starting with prefix (lowercase
    /// utf-8), excluding the prefix itself.
    pub fn complete(&self, prefix: &str) -> Results {
        let p = prefix.as_bytes();
        if p.is_empty() {
            return Results::default();
        }
        let pick = |order: &mut dyn Iterator<Item = &Entry>| {
            let mut out: Vec<&[u8]> = Vec::new();
            for e in order {
                let w = self.word(e);
                if self.completes(e, p) && !out.contains(&w) {
                    out.push(w);
                    if out.len() == MAX_RESULTS {
                        break;
                    }
                }
            }
            out
        };
        // common prefixes: the most frequent words have them
        const QUICK: usize = 1024;
        let out = pick(&mut self.words().iter().take(QUICK));
        if out.len() == MAX_RESULTS || self.words().len() <= QUICK {
            return to_results(out);
        }
        // otherwise only the words with the prefix, in frequency order
        let lo = self.alpha().partition_point(|&i| self.word(&self.words()[i as usize]) < p);
        let mut hits: Vec<u32> = self.alpha()[lo..]
            .iter()
            .copied()
            .take_while(|&i| self.word(&self.words()[i as usize]).starts_with(p))
            .collect();
        hits.sort_unstable();
        to_results(pick(&mut hits.iter().map(|&i| &self.words()[i as usize])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // a sloppy finger on a 2160px wide fi layout
    #[test]
    fn decodes_finnish_words() {
        let mut d = Dict::default();
        d.load_from(&[&format!("{}/dict/fi.txt", env!("CARGO_MANIFEST_DIR"))], None);
        let rows = ["qwertyuiopå", "asdfghjklöä", "zxcvbnm"];
        let offs = [0.0, 0.0, 2.0];
        let (unit, kh) = (2160.0 / 11.0, 120.0);
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
        d.load_from(&[&format!("{}/dict/en.txt", env!("CARGO_MANIFEST_DIR"))], None);
        let r = d.complete("hel");
        assert_eq!(r.words.len(), 3);
        assert!(r.words.iter().all(|w| w.starts_with("hel") && w != "hel"));
        // the slow path: a rare prefix
        let r = d.complete("zyg");
        assert!(r.words.iter().all(|w| w.starts_with("zyg")));
    }

    #[test]
    fn cache_holds_the_same_dictionary() {
        let root = env!("CARGO_MANIFEST_DIR");
        let (en, fi) = (format!("{root}/dict/en.txt"), format!("{root}/dict/fi.txt"));
        let list = [en.as_str(), fi.as_str()];
        let dir = std::env::temp_dir().join(format!("osk-cache-test-{}", std::process::id()));
        let cache = dir.join("words-test.bin");
        let (mut built, mut mapped) = (Dict::default(), Dict::default());
        built.load_from(&list, Some(&cache));
        mapped.load_from(&list, Some(&cache));
        assert!(matches!(built.store, Store::Built { .. }) && matches!(mapped.store, Store::Mapped { .. }));
        assert_eq!(built.text(), mapped.text());
        assert_eq!(built.alpha(), mapped.alpha());
        assert_eq!(built.lang_codes, mapped.lang_codes);
        assert!(built.words().iter().zip(mapped.words()).all(|(a, b)| (a.off, a.len, a.lang, a.lp.to_bits()) == (b.off, b.len, b.lang, b.lp.to_bits())));
        assert_eq!(built.complete("ki"), mapped.complete("ki"));
        // a changed word list is not taken from the old cache
        let other = Dict::cache_path(&list);
        assert_ne!(other, Dict::cache_path(&[en.as_str()]));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_lines_like_sscanf() {
        assert_eq!(parse_line(b"hello 123\n"), Some((&b"hello"[..], 123.0)));
        assert_eq!(parse_line(b"  hej\t7"), Some((&b"hej"[..], 7.0)));
        assert_eq!(parse_line(b"word\n"), None);
        assert_eq!(parse_line(b"\n"), None);
        assert_eq!(parse_line(b"x 1e3"), Some((&b"x"[..], 1000.0)));
    }
}
