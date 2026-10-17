#!/usr/bin/python3 -I
"""Import keyboard layouts and per-language key data from HeliBoard.

usage: tools/import-heliboard.py HELIBOARD_CHECKOUT [OUT_DIR]

Writes OUT_DIR (default: data/ next to this script's parent):
  layouts/<name>.txt   letter layouts in HeliBoard's simple format: one key
                       per line ("label popup popup ..."), a blank line between
                       rows. JSON layouts are flattened to the same format;
                       a shifted label that is not just the upper case is
                       kept as a "!shift!X" token.
  locales/<tag>.txt    HeliBoard's locale_key_texts ([popup_keys],
                       [extra_keys], [number_row], ...), unchanged.
  languages.txt        "<tag> <layout> <name>": the default layout per
                       language, from HeliBoard's method.xml.

A checkout only needs the data, e.g.:
  git clone --depth 1 --filter=blob:none --sparse \
      https://github.com/HeliBorg/HeliBoard.git hb
  git -C hb sparse-checkout set --no-cone /app/src/main/assets/layouts/main/ \
      /app/src/main/assets/locale_key_texts/ /app/src/main/res/xml/method.xml
"""

import json
import os
import re
import subprocess
import sys

ASSETS = "app/src/main/assets"

# names shown on the space bar, in the language itself
NAMES = {
    "af": "Afrikaans", "ar": "العربية", "az": "Azərbaycan", "be": "Беларуская",
    "bg": "Български", "bn": "বাংলা", "ca": "Català", "cs": "Čeština",
    "cy": "Cymraeg", "da": "Dansk", "de": "Deutsch", "el": "Ελληνικά",
    "en": "English", "eo": "Esperanto", "es": "Español", "et": "Eesti",
    "eu": "Euskara", "fa": "فارسی", "fi": "Suomi", "fr": "Français",
    "ga": "Gaeilge", "gl": "Galego", "gu": "ગુજરાતી", "hi": "हिन्दी",
    "hr": "Hrvatski", "hu": "Magyar", "hy": "Հայերեն", "id": "Indonesia",
    "in": "Indonesia", "is": "Íslenska", "it": "Italiano", "iw": "עברית",
    "he": "עברית", "ka": "ქართული", "kk": "Қазақ", "km": "ខ្មែរ",
    "kn": "ಕನ್ನಡ", "ko": "한국어", "ky": "Кыргызча", "la": "Latina",
    "lb": "Lëtzebuergesch", "lo": "ລາວ", "lt": "Lietuvių", "lv": "Latviešu",
    "mk": "Македонски", "ml": "മലയാളം", "mn": "Монгол", "mr": "मराठी",
    "ms": "Melayu", "nb": "Norsk", "ne": "नेपाली", "nl": "Nederlands",
    "pl": "Polski", "pt": "Português", "ro": "Română", "ru": "Русский",
    "si": "සිංහල", "sk": "Slovenčina", "sl": "Slovenščina", "sr": "Српски",
    "sv": "Svenska", "sw": "Kiswahili", "ta": "தமிழ்", "te": "తెలుగు",
    "th": "ไทย", "tl": "Tagalog", "tr": "Türkçe", "uk": "Українська",
    "ur": "اردو", "uz": "Oʻzbek", "vi": "Tiếng Việt", "zu": "isiZulu",
}


def pick(key):
    """Resolve HeliBoard/FlorisBoard selector keys to (normal, shifted)."""
    kind = key.get("$", "")
    if kind == "case_selector":
        return pick(key["lower"])[0], pick(key["upper"])[0]
    if kind == "shift_state_selector":
        normal = key.get("unshifted") or key.get("default")
        shifted = (key.get("manualOrLocked") or key.get("shifted")
                   or key.get("shiftedManual") or key.get("capsLock"))
        n = pick(normal)[0] if normal else None
        s = pick(shifted)[0] if shifted else None
        return n or s, s
    if kind == "variation_selector":
        return pick(key.get("normal") or key.get("default"))
    if kind == "keyboard_state_selector":
        return pick(key.get("alphabet") or key.get("default"))
    if kind == "layout_direction_selector":
        return pick(key["ltr"])
    return key, None


def label_of(key):
    if key is None:
        return None
    if key.get("type") == "placeholder":
        return None
    label = key.get("label")
    if not label and "code" in key and key["code"] > 0:
        label = chr(key["code"])
    if not label and "codePoints" in key:
        label = "".join(chr(c) for c in key["codePoints"])
    return label


def convert_json(path):
    text = open(path, encoding="utf-8").read()
    text = "\n".join(l for l in text.splitlines() if not l.strip().startswith("//"))
    rows = json.loads(text)
    out = []
    for row in rows:
        keys = []
        for raw in row:
            normal, shifted = pick(raw)
            label = label_of(normal)
            if not label:
                continue
            tokens = [label]
            slabel = label_of(shifted)
            if slabel and slabel != label.upper():
                tokens.append("!shift!" + slabel)
            popup = normal.get("popup", {})
            for group in ("main", "relevant"):
                entries = popup.get(group, [])
                if isinstance(entries, dict):
                    entries = [entries]
                for p in entries:
                    pl = label_of(pick(p)[0])
                    if pl and " " not in pl:
                        tokens.append(pl)
            keys.append(" ".join(t.replace("\n", "") for t in tokens))
        if keys:
            out.append(keys)
    return out


def convert_txt(path):
    rows, cur = [], []
    for line in open(path, encoding="utf-8").read().splitlines():
        if line.strip():
            cur.append(line.strip())
        elif cur:
            rows.append(cur)
            cur = []
    if cur:
        rows.append(cur)
    return rows


def languages(method_xml):
    xml = open(method_xml, encoding="utf-8").read()
    seen = {}
    for sub in re.findall(r"<subtype(.*?)/>", xml, re.S):
        loc = re.search(r'imeSubtypeLocale="([^"]+)"', sub)
        extra = re.search(r'imeSubtypeExtraValue="([^"]+)"', sub)
        if not loc or not extra:
            continue
        tag = loc.group(1).replace("_", "-")
        layout = re.search(r"MAIN:([^,|\"]+)", extra.group(1))
        layout = layout.group(1) if layout else "qwerty"
        seen.setdefault(tag, layout)  # first subtype is the default
    return seen


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    src = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "..", "data")
    rev = subprocess.run(["git", "-C", src, "rev-parse", "HEAD"],
                         capture_output=True, text=True).stdout.strip()

    os.makedirs(os.path.join(out, "layouts"), exist_ok=True)
    os.makedirs(os.path.join(out, "locales"), exist_ok=True)

    lay_dir = os.path.join(src, ASSETS, "layouts", "main")
    layouts = set()
    for name in sorted(os.listdir(lay_dir)):
        base, ext = os.path.splitext(name)
        path = os.path.join(lay_dir, name)
        rows = convert_json(path) if ext == ".json" else convert_txt(path)
        with open(os.path.join(out, "layouts", base + ".txt"), "w",
                  encoding="utf-8") as f:
            f.write("\n\n".join("\n".join(r) for r in rows) + "\n")
        layouts.add(base)

    loc_dir = os.path.join(src, ASSETS, "locale_key_texts")
    for name in sorted(os.listdir(loc_dir)):
        with open(os.path.join(loc_dir, name), encoding="utf-8") as f:
            data = f.read()
        with open(os.path.join(out, "locales", name), "w", encoding="utf-8") as f:
            f.write(data)

    langs = languages(os.path.join(src, "app/src/main/res/xml/method.xml"))
    with open(os.path.join(out, "languages.txt"), "w", encoding="utf-8") as f:
        f.write(f"# tag layout name -- default layouts from HeliBoard {rev}\n")
        f.write("# a layout ending in + gets the [extra_keys] of the language\n")
        for tag, layout in sorted(langs.items()):
            if layout.rstrip("+") not in layouts:
                continue
            base = tag.split("-")[0]
            f.write(f"{tag} {layout} {NAMES.get(base, tag)}\n")
    print(f"{len(layouts)} layouts, {len(langs)} languages from HeliBoard {rev}")


if __name__ == "__main__":
    main()
