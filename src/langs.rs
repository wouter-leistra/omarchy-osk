//! Layouts and long-press keys per language, read from the data files
//! (data/README.md): languages.txt, layouts/<name>.txt, locales/<tag>.txt
//! and popups.txt, searched in every directory of a ':' separated path so
//! files in the user config directory override the installed ones.

use crate::unicode;
use std::fs;

pub const MAX_POPUPS: usize = 24;
pub const MAX_ROWS: usize = 6;
pub const MAX_ROW_KEYS: usize = 16;
const LABEL_BYTES: usize = 31;
const MAX_TOKENS: usize = MAX_POPUPS + 4;

#[derive(Clone, Debug, Default)]
pub struct LangKey {
    /// typed text
    pub label: String,
    /// shifted text, upper case unless the layout gives one
    pub shift: String,
    /// long-press keys from the layout
    pub popups: Vec<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct Popup {
    pub label: String,
    pub cps: Vec<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct Lang {
    /// as configured, e.g. "fi" or "de-CH"
    pub code: String,
    /// layout file name, without +
    pub layout: String,
    /// shown on the space bar
    pub name: String,
    pub rows: Vec<Vec<LangKey>>,
    /// [popup_keys] of the language
    pub popups: Vec<Popup>,
    /// long-press digits on the top row
    pub number_row: [u32; 10],
}

/// popups.txt and HeliBoard's common accents, for every language
#[derive(Clone, Debug, Default)]
pub struct Common {
    pub common: Vec<Popup>,
    pub accents: Vec<Popup>,
}

fn truncate(s: &str, max: usize) -> String {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// first existing dir/name in the search path
fn read_data(dirs: &str, name: &str) -> Option<String> {
    dirs.split(':')
        .filter(|d| !d.is_empty())
        .find_map(|d| fs::read(format!("{d}/{name}")).ok())
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// lines as fgets with a 512 byte buffer gives them
fn lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for line in text.split_inclusive('\n') {
        let mut rest = line;
        while rest.len() > 511 {
            let mut cut = 511;
            while !rest.is_char_boundary(cut) {
                cut -= 1;
            }
            out.push(&rest[..cut]);
            rest = &rest[cut..];
        }
        out.push(rest);
    }
    out
}

fn tokenize(line: &str) -> Vec<&str> {
    line.split([' ', '\t', '\r', '\n'])
        .filter(|t| !t.is_empty())
        .take(MAX_TOKENS)
        .collect()
}

/// The text a HeliBoard token types, or None for tokens that are not plain
/// text (!icon/..., !autoColumnOrder!4, %, $$$ currency placeholders).
fn token_text(t: &str) -> Option<&str> {
    let b = t.as_bytes();
    if b[0] == b'\\' && b.len() > 1 {
        return Some(&t[1..]);
    }
    if t == "%" || t.contains("$$$") {
        return None;
    }
    if b[0] == b'!' && b.len() > 1 {
        return None;
    }
    if let Some(bar) = t.find('|') {
        if bar != 0 && bar + 1 < t.len() {
            return if b[bar + 1] == b'!' { None } else { Some(&t[bar + 1..]) };
        }
    }
    Some(t)
}

/// append the token as a long-press key if it is a single character
fn add_popup(cps: &mut Vec<u32>, text: Option<&str>) {
    let Some(text) = text else { return };
    if cps.len() >= MAX_POPUPS {
        return;
    }
    if let Some(cp) = unicode::single(text) {
        if !cps.contains(&cp) {
            cps.push(cp);
        }
    }
}

/// a key from the tokens of one layout line ("label popup ...")
fn parse_key(tok: &[&str]) -> Option<LangKey> {
    let label = token_text(tok.first()?)?;
    let mut k = LangKey { label: truncate(label, LABEL_BYTES), ..Default::default() };
    for t in &tok[1..] {
        if let Some(s) = t.strip_prefix("!shift!").filter(|s| !s.is_empty()) {
            k.shift = truncate(s, LABEL_BYTES);
        } else {
            add_popup(&mut k.popups, token_text(t));
        }
    }
    if k.shift.is_empty() {
        for cp in unicode::cps(&k.label) {
            if k.shift.len() >= LABEL_BYTES - 4 {
                break;
            }
            unicode::push_cp(&mut k.shift, unicode::upper(cp));
        }
    }
    Some(k)
}

fn load_layout(l: &mut Lang, dirs: &str) -> bool {
    let Some(text) = read_data(dirs, &format!("layouts/{}.txt", l.layout)) else {
        return false;
    };
    l.rows.clear();
    let mut in_row = false;
    for line in lines(&text) {
        let tok = tokenize(line);
        if tok.is_empty() {
            in_row = false;
            continue;
        }
        if !in_row {
            if l.rows.len() == MAX_ROWS {
                break;
            }
            l.rows.push(Vec::new());
            in_row = true;
        }
        let row = l.rows.last_mut().unwrap();
        if row.len() < MAX_ROW_KEYS {
            if let Some(k) = parse_key(&tok) {
                row.push(k);
            }
        }
    }
    !l.rows.is_empty()
}

fn add_lang_popup(list: &mut Vec<Popup>, tok: &[&str]) {
    let Some(mut label) = token_text(tok[0]) else { return };
    if tok.len() < 2 {
        return;
    }
    if label == "punctuation" {
        label = ".";
    }
    let label = truncate(label, LABEL_BYTES);
    let idx = match list.iter().rposition(|p| p.label == label) {
        Some(i) => i,
        None => {
            list.push(Popup { label, cps: Vec::new() });
            list.len() - 1
        }
    };
    for t in &tok[1..] {
        add_popup(&mut list[idx].cps, token_text(t));
    }
}

fn atoi(s: &str) -> i64 {
    let s = s.trim_start();
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let n: i64 = digits
        .bytes()
        .take_while(|b| b.is_ascii_digit())
        .fold(0, |n, b| n.saturating_mul(10).saturating_add((b - b'0') as i64));
    if neg { -n } else { n }
}

/// read a locale_key_texts style file; extra keys only for "+" layouts
fn load_locale(dirs: &str, name: &str, popups: &mut Vec<Popup>, mut l: Option<&mut Lang>, extra_keys: bool) -> bool {
    let Some(text) = read_data(dirs, name) else { return false };
    let mut section = String::new();
    for line in lines(&text) {
        if line.starts_with('[') {
            // sscanf("[%31[^]]"): at least one character up to ']'
            let inner: String = line[1..].chars().take_while(|&c| c != ']').collect();
            if !inner.is_empty() {
                section = truncate(&inner, 31);
            }
            continue;
        }
        let tok = tokenize(line);
        if tok.is_empty() {
            continue;
        }
        if section == "popup_keys" {
            add_lang_popup(popups, &tok);
        } else if section == "extra_keys" && extra_keys {
            // "2: ö ø" appends ö (long press ø) to the second row
            if let Some(l) = l.as_deref_mut() {
                let row = atoi(tok[0]) - 1;
                if row < 0 || row as usize >= l.rows.len() || tok.len() < 2 || l.rows[row as usize].len() >= MAX_ROW_KEYS {
                    continue;
                }
                if let Some(k) = parse_key(&tok[1..]) {
                    l.rows[row as usize].push(k);
                }
            }
        } else if section == "number_row" && tok.len() == 10 {
            if let Some(l) = l.as_deref_mut() {
                for (i, t) in tok.iter().enumerate() {
                    if let Some(c) = t.chars().next() {
                        l.number_row[i] = c as u32;
                    }
                }
            }
        }
    }
    true
}

/// languages.txt: exact tag first, then the first entry of the same
/// language ("en" finds "en-US", "de-AT" finds "de")
fn lookup_language(dirs: &str, tag: &str) -> (String, String) {
    let base: String = tag.split('-').next().unwrap_or("").chars().take(15).collect();
    let (mut best, mut best_layout, mut best_name) = (0, String::new(), String::new());
    if let Some(text) = read_data(dirs, "languages.txt") {
        for line in lines(&text) {
            if line.starts_with('#') {
                continue;
            }
            let mut rest = line.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
            let t_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let t = truncate(&rest[..t_end], 15);
            rest = rest[t_end..].trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
            let lay_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            if t.is_empty() || lay_end == 0 {
                continue;
            }
            let lay = truncate(&rest[..lay_end], 47);
            let nm = rest[lay_end..].trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
            let nm = &nm[..nm.find(['\r', '\n']).unwrap_or(nm.len())];
            let score = if t.eq_ignore_ascii_case(tag) {
                3
            } else if t.eq_ignore_ascii_case(&base) {
                2
            } else if t.len() > base.len() && t[..base.len()].eq_ignore_ascii_case(&base) && t.as_bytes()[base.len()] == b'-' {
                1
            } else {
                0
            };
            if score > best {
                best = score;
                best_layout = lay;
                best_name = truncate(if nm.is_empty() { &t } else { nm }, 47);
            }
        }
    }
    if best > 0 {
        (best_layout, best_name)
    } else {
        ("qwerty".into(), truncate(tag, 47))
    }
}

pub fn load_common(dirs: &str) -> Common {
    let mut c = Common::default();
    load_locale(dirs, "popups.txt", &mut c.common, None, false);
    load_locale(dirs, "locales/more_popups_main.txt", &mut c.accents, None, false);
    c
}

/// Load "en,fi,de:qwertz" (tag[:layout] list). Languages without a usable
/// layout are skipped with a warning.
pub fn load(list: &str, dirs: &str) -> Vec<Lang> {
    let mut out = Vec::new();
    for item in list.split(',') {
        let item = item.trim_start();
        if item.is_empty() {
            continue;
        }
        let (tag, layout_override) = match item.find(':') {
            Some(i) => (&item[..i], Some(&item[i + 1..])),
            None => (item, None),
        };
        let mut l = Lang { code: truncate(tag, 15).replace('_', "-"), ..Default::default() };
        let (mut layout, name) = lookup_language(dirs, &l.code);
        l.name = name;
        if let Some(o) = layout_override.filter(|o| !o.is_empty()) {
            layout = truncate(o, 47);
        }
        let plus = layout.ends_with('+');
        if plus {
            layout.pop();
        }
        l.layout = layout;
        if !load_layout(&mut l, dirs) {
            eprintln!("omarchy-osk: no layout {} for language {}", l.layout, l.code);
            continue;
        }
        for (i, d) in "1234567890".chars().enumerate() {
            l.number_row[i] = d as u32;
        }
        let base = l.code.split('-').next().unwrap_or("").to_string();
        let mut popups = Vec::new();
        if !load_locale(dirs, &format!("locales/{}.txt", l.code), &mut popups, Some(&mut l), plus) {
            load_locale(dirs, &format!("locales/{base}.txt"), &mut popups, Some(&mut l), plus);
        }
        l.popups = popups;
        out.push(l);
    }
    out
}

fn find_popup<'a>(list: &'a [Popup], label: &str) -> Option<&'a Popup> {
    list.iter().find(|p| p.label == label)
}

fn in_layout(l: &Lang, cp: u32) -> bool {
    l.rows.iter().flatten().any(|k| unicode::single(&k.label) == Some(cp))
}

struct AltSet<'a> {
    out: Vec<u32>,
    max: usize,
    me: u32,
    skip_keys: Option<&'a Lang>,
}

impl AltSet<'_> {
    fn add(&mut self, cp: u32) {
        if cp == 0 || cp == self.me || self.out.len() >= self.max || self.out.contains(&cp) {
            return;
        }
        if self.skip_keys.is_some_and(|l| in_layout(l, cp)) {
            return;
        }
        self.out.push(cp);
    }

    fn add_list(&mut self, p: Option<&Popup>) {
        for &cp in p.map(|p| p.cps.as_slice()).unwrap_or(&[]) {
            self.add(cp);
        }
    }
}

/// Long-press keys for a key with this label, in order: the top row digit,
/// popups from the layout, the language, popups.txt and the common accents.
/// Characters that are keys of the language's layout already are left out
/// for letter keys. row/col locate a layout key (row None for keys outside
/// the letter layout).
pub fn alternates(c: &Common, l: Option<&Lang>, label: &str, row: Option<usize>, col: usize, max: usize) -> Vec<u32> {
    let me = unicode::single(label);
    let letter = me.is_some_and(unicode::is_letter);
    let mut s = AltSet { out: Vec::new(), max, me: me.unwrap_or(0), skip_keys: if letter { l } else { None } };

    if let Some(l) = l {
        if row == Some(0) && col < 10 && l.rows[0].len() >= 10 {
            // digits go first even when the layout has them as keys
            let skip = s.skip_keys.take();
            s.add(l.number_row[col]);
            s.skip_keys = skip;
        }
        if let Some(r) = row.filter(|&r| r < l.rows.len()) {
            if col < l.rows[r].len() {
                for &cp in &l.rows[r][col].popups {
                    s.add(cp);
                }
            }
        }
        s.add_list(find_popup(&l.popups, label));
    }
    s.add_list(find_popup(&c.common, label));
    if letter {
        s.add_list(find_popup(&c.accents, label));
    }
    s.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> String {
        format!("{}/data", env!("CARGO_MANIFEST_DIR"))
    }

    struct T {
        langs: Vec<Lang>,
        common: Common,
    }

    impl T {
        fn new(list: &str) -> T {
            T { langs: load(list, &data()), common: load_common(&data()) }
        }
        fn get(&self, code: &str) -> &Lang {
            self.langs.iter().find(|l| l.code == code).unwrap_or_else(|| panic!("{code} not loaded"))
        }
        fn row(&self, code: &str, r: usize) -> String {
            self.get(code).rows[r].iter().map(|k| k.label.as_str()).collect()
        }
        fn alts(&self, code: &str, label: &str) -> String {
            let l = self.get(code);
            let pos = l.rows.iter().enumerate().find_map(|(r, row)| row.iter().position(|k| k.label == label).map(|c| (r, c)));
            let (row, col) = match pos {
                Some((r, c)) => (Some(r), c),
                None => (None, 0),
            };
            alternates(&self.common, Some(l), label, row, col, MAX_POPUPS).into_iter().map(unicode::from_cp).collect()
        }
    }

    #[test]
    fn layouts_and_alternates() {
        let t = T::new("en,fi,sv,de,fr,ru,el,de-CH:qwertz+,uk");
        assert_eq!(t.row("fi", 0), "qwertyuiopå");
        assert_eq!(t.row("fi", 1), "asdfghjklöä");
        assert_eq!(t.row("fi", 2), "zxcvbnm");
        assert_eq!(t.get("fi").name, "Suomi");
        assert_eq!(t.alts("fi", "q"), "1");
        assert_eq!(t.alts("fi", "e"), "3éèêë€ē");
        assert_eq!(t.alts("fi", "a"), "áàâãæā");
        assert_eq!(t.alts("fi", "ö"), "øœ");
        assert_eq!(t.alts("fi", "å"), "");
        assert_eq!(t.alts("fi", "s"), "šß§");
        assert_eq!(t.alts("fi", "."), "?!…");
        assert_eq!(t.get("fi").rows[1][10].shift, "Ä");
        assert_eq!(t.row("en", 0), "qwertyuiop");
        assert_eq!(t.row("en", 1), "asdfghjkl");
        assert_eq!(t.get("en").name, "English");
        assert_eq!(t.alts("en", "a"), "áàâãæäåā");
        assert_eq!(t.alts("en", "o"), "9óòôõøœöō");
        assert_eq!(t.row("de", 0), "qwertzuiop");
        assert_eq!(t.alts("de", "u"), "7üúùûū");
        assert_eq!(t.row("de-CH", 0), "qwertzuiopü");
        assert_eq!(t.row("fr", 0), "azertyuiop");
        assert_eq!(t.row("fr", 2), "wxcvbn'");
        assert_eq!(t.get("fr").rows[2][6].shift, "?");
        assert_eq!(t.alts("fr", "e"), "3éèêë€ē");
        assert_eq!(t.row("ru", 0), "йцукенгшщзх");
        assert_eq!(t.get("ru").rows[0][0].shift, "Й");
        assert_eq!(t.alts("ru", "е"), "5ёѣ");
        assert_eq!(t.get("ru").name, "Русский");
        assert_eq!(t.row("el", 0), ";ςερτυθιοπ");
    }

    #[test]
    fn tokens() {
        assert_eq!(token_text("\\!"), Some("!"));
        assert_eq!(token_text("!autoColumnOrder!4"), None);
        assert_eq!(token_text("!"), Some("!"));
        assert_eq!(token_text("%"), None);
        assert_eq!(token_text("ھ|ه"), Some("ه"));
        assert_eq!(token_text("a|!code/x"), None);
        assert_eq!(atoi("2:"), 2);
    }
}
