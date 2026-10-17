//! Word lists for gesture typing, made from frequency lists ("word count"
//! per line, most frequent first, as FrequencyWords publishes them): only
//! words whose letters are all keys of the language's layout can be swiped.

use crate::langs::Lang;
use crate::unicode;
use std::collections::HashSet;
use std::io::{BufRead, Write};

/// lowest count kept: rarer words are mostly noise
pub const MIN_COUNT: f64 = 5.0;

/// the letters on the keys of a language's layout, lower case
pub fn letters(lang: &Lang) -> HashSet<u32> {
    lang.rows
        .iter()
        .flatten()
        .filter_map(|k| unicode::single(&k.label))
        .filter(|&cp| unicode::is_letter(cp))
        .map(unicode::lower)
        .collect()
}

/// copy the swipeable words of input to out, lower case, first (most
/// frequent) occurrence only; returns how many
pub fn filter(lang: &Lang, input: impl BufRead, mut out: impl Write) -> std::io::Result<usize> {
    let keys = letters(lang);
    let mut seen = HashSet::new();
    let mut n = 0;
    for line in input.split(b'\n') {
        let line = line?;
        let line = String::from_utf8_lossy(&line);
        let mut it = line.split_whitespace();
        let (Some(word), Some(count)) = (it.next(), it.next()) else { continue };
        let Ok(count) = count.parse::<f64>() else { continue };
        if count < MIN_COUNT {
            continue;
        }
        let lower: String = word.chars().filter_map(|c| char::from_u32(unicode::lower(c as u32))).collect();
        let ok = lower.len() < crate::swipe::MAX_WORD
            && lower.chars().any(|c| keys.contains(&(c as u32)))
            && lower.chars().all(|c| c == '\'' || keys.contains(&(c as u32)));
        if ok && seen.insert(lower.clone()) {
            writeln!(out, "{lower} {count}")?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lang(tag: &str) -> Lang {
        crate::langs::load(tag, &format!("{}/data", env!("CARGO_MANIFEST_DIR"))).remove(0)
    }

    #[test]
    fn keeps_what_the_layout_can_swipe() {
        let input = "Der 900\nund 800\nfür 700\nstraße 600\nçava 500\nl'eau 400\n123 300\nüber 200\nund 100\nselten 3\n";
        let mut out = Vec::new();
        // de-DE: qwertz with ü ö ä ß keys
        let n = filter(&lang("de-DE"), input.as_bytes(), &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "der 900\nund 800\nfür 700\nstraße 600\nl'eau 400\nüber 200\n");
        assert_eq!(n, 6);
        // plain de has ü only on long press: words with it can't be swiped
        let mut out = Vec::new();
        filter(&lang("de"), input.as_bytes(), &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "der 900\nund 800\nl'eau 400\n");
    }

    #[test]
    fn other_scripts() {
        let mut out = Vec::new();
        filter(&lang("ru"), "Привет 50\nhello 40\nёлка 30\n".as_bytes(), &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "привет 50\n");
    }
}
