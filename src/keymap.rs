//! The xkb keymap the keyboard types through. Characters the layout can't
//! produce are added to keycodes the on-screen keyboard never sends, so
//! every character is a plain key press and the keymap never has to be
//! swapped (which races in Hyprland).

use xkbcommon::xkb;

pub struct Xkb {
    keymap: xkb::Keymap,
    ctx: xkb::Context,
    /// keymap as uploaded
    pub text: String,
    cache: Vec<(u32, u32, u32, bool)>, // cp, keycode, mask, found
}

const CACHE_SIZE: usize = 256;

fn kc(k: u32) -> xkb::Keycode {
    xkb::Keycode::new(k)
}

impl Xkb {
    /// compile the keymap from xkb names (e.g. "fi")
    pub fn new(layout: &str, variant: &str) -> Xkb {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let Some(keymap) = xkb::Keymap::new_from_names(&ctx, "", "", layout, variant, None, xkb::KEYMAP_COMPILE_NO_FLAGS) else {
            eprintln!("could not compile xkb keymap for layout {layout}");
            std::process::exit(1);
        };
        let text = keymap.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
        Xkb { keymap, ctx, text, cache: Vec::new() }
    }

    /// keycode and modifiers that type this character
    pub fn lookup(&mut self, cp: u32) -> Option<(u32, u32)> {
        if let Some(&(_, k, m, found)) = self.cache.iter().find(|e| e.0 == cp) {
            return found.then_some((k, m));
        }
        let want = xkb::utf32_to_keysym(cp);
        let (mut found, mut best_kc, mut best_mask, mut best_level) = (false, 0, 0, 99);
        let (min, max) = (self.keymap.min_keycode().raw(), self.keymap.max_keycode().raw());
        if want.raw() != 0 {
            for k in min..=max {
                let levels = self.keymap.num_levels_for_key(kc(k), 0);
                let mut lvl = 0;
                while lvl < levels && lvl < best_level {
                    let syms = self.keymap.key_get_syms_by_level(kc(k), 0, lvl);
                    if syms.len() == 1 && syms[0] == want {
                        let mut masks = [0u32; 8];
                        let nm = self.keymap.key_get_mods_for_level(kc(k), 0, lvl, &mut masks);
                        if nm > 0 {
                            // prefer the simplest modifier combination
                            let mut m = masks[0];
                            for &mk in &masks[1..nm] {
                                if mk.count_ones() < m.count_ones() {
                                    m = mk;
                                }
                            }
                            found = true;
                            best_kc = k;
                            best_mask = m;
                            best_level = lvl;
                        }
                    }
                    lvl += 1;
                }
            }
        }
        if self.cache.len() < CACHE_SIZE {
            self.cache.push((cp, best_kc, best_mask, found));
        }
        found.then_some((best_kc, best_mask))
    }

    fn modifier_key(&self, k: u32) -> bool {
        let levels = self.keymap.num_levels_for_key(kc(k), 0);
        for lvl in 0..levels {
            for s in self.keymap.key_get_syms_by_level(kc(k), 0, lvl) {
                let s = s.raw();
                if (xkb::keysyms::KEY_ISO_Lock..=xkb::keysyms::KEY_ISO_Level5_Lock).contains(&s)
                    || (xkb::keysyms::KEY_Shift_L..=xkb::keysyms::KEY_Hyper_R).contains(&s)
                    || s == xkb::keysyms::KEY_Mode_switch
                    || s == xkb::keysyms::KEY_Num_Lock
                {
                    return true;
                }
            }
        }
        false
    }

    /// Keycodes the on-screen keyboard never sends, best first: unused
    /// ones, the keypad, then keys such as F1-F24 and media keys. This
    /// keymap only belongs to the virtual keyboard. Keys it does send and
    /// modifiers stay as they are.
    fn spare_keycodes(&self, used: &[bool; 256]) -> Vec<u32> {
        let mut out = Vec::new();
        let last = self.keymap.max_keycode().raw();
        for pass in 0..3 {
            let mut k = 9;
            while k <= last && k < 256 && out.len() < 256 {
                if let Some(name) = self.keymap.key_get_name(kc(k)) {
                    if !used[k as usize] {
                        let empty = self.keymap.num_layouts_for_key(kc(k)) == 0;
                        let keypad = name.starts_with("KP") && name != "KPEN";
                        let take = if empty {
                            pass == 0
                        } else if keypad {
                            pass == 1
                        } else {
                            pass == 2 && !self.modifier_key(k)
                        };
                        if take {
                            out.push(k);
                        }
                    }
                }
                k += 1;
            }
        }
        out
    }

    /// Add the characters the keymap can't type. cps: every character the
    /// keyboard can type; codes: evdev codes of its Code keys.
    pub fn extend(&mut self, cps: &[u32], codes: &[u32], debug: bool) {
        let mut used = [false; 256];
        let mut missing = Vec::new();
        for &cp in cps {
            match self.lookup(cp) {
                None => missing.push(cp),
                Some((k, _)) if k < 256 => used[k as usize] = true,
                _ => {}
            }
        }
        // space and the digits type through the keymap too, as do shortcuts
        for c in " 0123456789abcdefghijklmnopqrstuvwxyz".bytes() {
            if let Some((k, _)) = self.lookup(c as u32) {
                if k < 256 {
                    used[k as usize] = true;
                }
            }
        }
        for &code in codes {
            if code + 8 < 256 {
                used[(code + 8) as usize] = true;
            }
        }
        use crate::keyboard::keys::*;
        for code in [KEY_LEFTCTRL, KEY_LEFTALT, KEY_LEFTMETA, KEY_LEFTSHIFT, KEY_LEFT, KEY_RIGHT, KEY_BACKSPACE] {
            used[(code + 8) as usize] = true;
        }
        let spare = self.spare_keycodes(&used);
        if debug {
            eprintln!("omarchy-osk: room for {} extra characters", spare.len() * 4);
            eprintln!("omarchy-osk: {} of {} characters missing from the keymap", missing.len(), cps.len());
        }
        // four characters per free key: plain, Shift, AltGr, Shift+AltGr
        let mut extra = String::new();
        let mut added = 0;
        let mut next = 0;
        let mut i = 0;
        while i < missing.len() {
            if next == spare.len() {
                eprintln!("omarchy-osk: no free keycodes left for {} characters", missing.len() - i);
                break;
            }
            let k = spare[next];
            next += 1;
            let syms: Vec<String> = (0..4)
                .map(|j| match missing.get(i + j) {
                    Some(&cp) => xkb::keysym_get_name(xkb::utf32_to_keysym(cp)),
                    None => "NoSymbol".into(),
                })
                .collect();
            let kname = self.keymap.key_get_name(kc(k)).unwrap_or("");
            extra += &format!(
                "\toverride key <{kname}> {{ type= \"FOUR_LEVEL\", [ {}, {}, {}, {} ] }};\n",
                syms[0], syms[1], syms[2], syms[3]
            );
            added += (missing.len() - i).min(4);
            i += 4;
        }
        self.cache.clear();
        if added == 0 {
            return;
        }
        // the symbols section is the last one in the keymap
        let Some(end) = self.text.find("\n};\n\n};") else { return };
        let head = end + 1;
        let s = format!("{}{}{}", &self.text[..head], extra, &self.text[head..]);
        let Some(km) = xkb::Keymap::new_from_string(&self.ctx, s, xkb::KEYMAP_FORMAT_TEXT_V1, xkb::KEYMAP_COMPILE_NO_FLAGS) else {
            eprintln!("omarchy-osk: extended keymap failed to compile");
            return;
        };
        self.text = km.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
        self.keymap = km;
        eprintln!("omarchy-osk: added {added} extra characters to the keymap");
    }
}
