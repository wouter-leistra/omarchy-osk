//! Case mapping and letter tests with glibc's C.UTF-8 rules, exactly as the
//! keyboard always did, independent of the process locale.

use std::sync::OnceLock;

#[allow(non_camel_case_types)]
type locale_t = *mut libc::c_void;

extern "C" {
    fn towupper_l(wc: u32, loc: locale_t) -> u32;
    fn towlower_l(wc: u32, loc: locale_t) -> u32;
    fn iswalpha_l(wc: u32, loc: locale_t) -> libc::c_int;
}

struct Loc(locale_t);
unsafe impl Send for Loc {}
unsafe impl Sync for Loc {}

fn locale() -> Option<locale_t> {
    static LOC: OnceLock<Loc> = OnceLock::new();
    let l = LOC.get_or_init(|| unsafe {
        let mut l = libc::newlocale(libc::LC_CTYPE_MASK, c"C.UTF-8".as_ptr(), std::ptr::null_mut());
        if l.is_null() {
            l = libc::newlocale(libc::LC_CTYPE_MASK, c"en_US.UTF-8".as_ptr(), std::ptr::null_mut());
        }
        Loc(l as locale_t)
    });
    (!l.0.is_null()).then_some(l.0)
}

pub fn upper(cp: u32) -> u32 {
    match locale() {
        Some(l) => unsafe { towupper_l(cp, l) },
        None if (b'a' as u32..=b'z' as u32).contains(&cp) || ((0xE0..=0xFE).contains(&cp) && cp != 0xF7) => cp - 32,
        None => cp,
    }
}

pub fn lower(cp: u32) -> u32 {
    match locale() {
        Some(l) => unsafe { towlower_l(cp, l) },
        None if (b'A' as u32..=b'Z' as u32).contains(&cp) || ((0xC0..=0xDE).contains(&cp) && cp != 0xD7) => cp + 32,
        None => cp,
    }
}

pub fn is_letter(cp: u32) -> bool {
    match locale() {
        Some(l) => unsafe { iswalpha_l(cp, l) != 0 },
        None => upper(cp) != lower(cp),
    }
}

/// Code points of a string (what utf8_decode gives for valid UTF-8).
pub fn cps(s: &str) -> Vec<u32> {
    s.chars().map(|c| c as u32).collect()
}

/// The single code point of a one-character string.
pub fn single(s: &str) -> Option<u32> {
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => Some(c as u32),
        _ => None,
    }
}

pub fn push_cp(out: &mut String, cp: u32) {
    if let Some(c) = char::from_u32(cp) {
        out.push(c);
    }
}

pub fn from_cp(cp: u32) -> String {
    let mut s = String::new();
    push_cp(&mut s, cp);
    s
}
