/* omarchy-osk: Gboard-style Finnish/Swedish layout.
 *
 * Char keys type their label (or shift_label) through the active xkb keymap,
 * so they follow whatever xkb layout the keyboard was started with (fi by
 * default). Letter Char keys take part in gesture typing.
 */

/* default landscape height (overridden by -L) */
#define KBD_PIXEL_HEIGHT 560
#define KBD_PIXEL_LANDSCAPE_HEIGHT 560

/* spacing around each key */
#define KBD_KEY_BORDER 5

enum layout_id {
	Abc = 0,
	Sym,
	Sym2,
	NumLayouts,
};

static struct key keys_abc[], keys_sym[], keys_sym2[];

static struct layout layouts[NumLayouts] = {
  [Abc] = {keys_abc, "xkb", "abc", true},
  [Sym] = {keys_sym, "xkb", "sym", false},
  [Sym2] = {keys_sym2, "xkb", "sym2", false},
};

#define CH(l, s) {l, s, 1.0, Char}
#define HIDE {"\xf3\xb0\x8c\x8f", "\xf3\xb0\x8c\x8f", 1.0, Hide, .scheme = 1}
#define BOTTOM_ROW(layer_label, layer)                                       \
  {layer_label, layer_label, 1.5, Layout, 0, &layouts[layer], .scheme = 1},  \
  {",", ",", 1.0, Char},                                                     \
  {"", "", 5.0, Code, KEY_SPACE},                                            \
  {".", ".", 1.0, Char},                                                     \
  {"\xe2\x8f\x8e", "\xe2\x8f\x8e", 1.5, Code, KEY_ENTER, .scheme = 2},       \
  HIDE,                                                                      \
  {"", "", 0.0, Last}

static struct key keys_abc[] = {
  CH("q", "Q"), CH("w", "W"), CH("e", "E"), CH("r", "R"), CH("t", "T"),
  CH("y", "Y"), CH("u", "U"), CH("i", "I"), CH("o", "O"), CH("p", "P"),
  CH("å", "Å"),
  {"", "", 0.0, EndRow},

  CH("a", "A"), CH("s", "S"), CH("d", "D"), CH("f", "F"), CH("g", "G"),
  CH("h", "H"), CH("j", "J"), CH("k", "K"), CH("l", "L"), CH("ö", "Ö"),
  CH("ä", "Ä"),
  {"", "", 0.0, EndRow},

  {"⇧", "⇪", 2.0, Mod, Shift, .scheme = 1},
  CH("z", "Z"), CH("x", "X"), CH("c", "C"), CH("v", "V"), CH("b", "B"),
  CH("n", "N"), CH("m", "M"),
  {"⌫", "⌫", 2.0, Code, KEY_BACKSPACE, .scheme = 1},
  {"", "", 0.0, EndRow},

  BOTTOM_ROW("?123", Sym),
};

static struct key keys_sym[] = {
  CH("1", "1"), CH("2", "2"), CH("3", "3"), CH("4", "4"), CH("5", "5"),
  CH("6", "6"), CH("7", "7"), CH("8", "8"), CH("9", "9"), CH("0", "0"),
  CH("%", "%"),
  {"", "", 0.0, EndRow},

  CH("@", "@"), CH("#", "#"), CH("€", "€"), CH("$", "$"), CH("_", "_"),
  CH("&", "&"), CH("-", "-"), CH("+", "+"), CH("(", "("), CH(")", ")"),
  CH("/", "/"),
  {"", "", 0.0, EndRow},

  {"=\\<", "=\\<", 2.0, Layout, 0, &layouts[Sym2], .scheme = 1},
  CH("*", "*"), CH("\"", "\""), CH("'", "'"), CH(":", ":"), CH(";", ";"),
  CH("!", "!"), CH("?", "?"),
  {"⌫", "⌫", 2.0, Code, KEY_BACKSPACE, .scheme = 1},
  {"", "", 0.0, EndRow},

  BOTTOM_ROW("ABC", Abc),
};

static struct key keys_sym2[] = {
  {"Esc", "Esc", 1.0, Code, KEY_ESC, .scheme = 1},
  {"Tab", "Tab", 1.0, Code, KEY_TAB, .scheme = 1},
  CH("~", "~"), CH("`", "`"), CH("|", "|"), CH("\\", "\\"), CH("{", "{"),
  CH("}", "}"), CH("[", "["), CH("]", "]"), CH("^", "^"),
  {"", "", 0.0, EndRow},

  {"Ctrl", "Ctrl", 1.0, Mod, Ctrl, .scheme = 1},
  {"Alt", "Alt", 1.0, Mod, Alt, .scheme = 1},
  {"Super", "Super", 1.0, Mod, Super, .scheme = 1},
  {"←", "←", 1.0, Code, KEY_LEFT, .scheme = 1},
  {"↓", "↓", 1.0, Code, KEY_DOWN, .scheme = 1},
  {"↑", "↑", 1.0, Code, KEY_UP, .scheme = 1},
  {"→", "→", 1.0, Code, KEY_RIGHT, .scheme = 1},
  {"Home", "Home", 1.0, Code, KEY_HOME, .scheme = 1},
  {"End", "End", 1.0, Code, KEY_END, .scheme = 1},
  {"PgUp", "PgUp", 1.0, Code, KEY_PAGEUP, .scheme = 1},
  {"PgDn", "PgDn", 1.0, Code, KEY_PAGEDOWN, .scheme = 1},
  {"", "", 0.0, EndRow},

  {"?123", "?123", 2.0, Layout, 0, &layouts[Sym], .scheme = 1},
  CH("<", "<"), CH(">", ">"), CH("=", "="), CH("£", "£"), CH("§", "§"),
  CH("½", "½"), CH("¤", "¤"),
  {"⌫", "⌫", 2.0, Code, KEY_BACKSPACE, .scheme = 1},
  {"", "", 0.0, EndRow},

  BOTTOM_ROW("ABC", Abc),
};
