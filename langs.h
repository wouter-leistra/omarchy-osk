#ifndef __LANGS_H
#define __LANGS_H

/* omarchy-osk: keyboard layouts and long-press keys per language, read from
 * data files (see data/README.md):
 *   languages.txt      tag, default layout and display name per language
 *   layouts/<name>.txt one key per line ("label popup ..."), blank line
 *                      between rows; a layout name ending in + gets the
 *                      [extra_keys] of the language appended to its rows
 *   locales/<tag>.txt  [popup_keys], [extra_keys], [number_row] per language
 *   popups.txt         long-press keys for every language (punctuation, ...)
 * Every directory in the search path is tried in order, so files in the user
 * config directory override the installed ones. */

#include <stdbool.h>
#include <stdint.h>

#define LANG_LABEL 32
#define LANG_MAX_POPUPS 24
#define LANG_MAX_ROWS 6
#define LANG_MAX_ROW_KEYS 16

struct lang_key {
	char label[LANG_LABEL];  /* typed text */
	char shift[LANG_LABEL];  /* shifted text, upper case unless given */
	uint32_t popups[LANG_MAX_POPUPS]; /* long-press keys from the layout */
	int npopups;
};

struct lang_popup {
	char label[LANG_LABEL];
	uint32_t cps[LANG_MAX_POPUPS];
	int n;
};

struct lang {
	char code[16];       /* as configured, e.g. "fi" or "de-CH" */
	char layout[48];     /* layout file name, without + */
	char name[48];       /* shown on the space bar */
	struct lang_key rows[LANG_MAX_ROWS][LANG_MAX_ROW_KEYS];
	int nkeys[LANG_MAX_ROWS];
	int nrows;
	struct lang_popup *popups; /* [popup_keys] of the language */
	int npopups;
	uint32_t number_row[10];   /* long-press digits on the top row */
};

/* Load "en,fi,de:qwertz" (tag[:layout] list) from a ':' separated list of
 * data directories. Languages without a usable layout are skipped with a
 * warning. Returns the number of languages. */
int langs_load(const char *list, const char *dirs, struct lang **out);

/* Long-press keys for a key with this label, in order: the top row digit,
 * popups from the layout, the language, popups.txt and HeliBoard's common
 * accents. Characters that are keys of the language's layout already are
 * left out for letter keys. row/col locate a layout key (row -1 for keys
 * outside the letter layout); l may be NULL. */
int lang_alternates(const struct lang *l, const char *label, int row, int col,
                    uint32_t *out, int max);

/* Unicode helpers (C.UTF-8 rules, independent of the process locale) */
uint32_t lang_upper(uint32_t cp);
uint32_t lang_lower(uint32_t cp);
bool lang_is_letter(uint32_t cp);

#endif
