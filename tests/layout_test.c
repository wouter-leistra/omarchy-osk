/* offline check of the layout and long-press data (langs.c)
 *   cc -o /tmp/layout_test tests/layout_test.c langs.c swipe.c -lm
 *   /tmp/layout_test data [lang,lang,...]   prints the layouts
 *   /tmp/layout_test data --check           runs the assertions */
#include "../langs.h"
#include "../swipe.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int failures;

static void
expect(const char *what, const char *got, const char *want)
{
	if (strcmp(got, want)) {
		printf("FAIL %s: got \"%s\", want \"%s\"\n", what, got, want);
		failures++;
	}
}

static const struct lang_key *
find_key(const struct lang *l, const char *label, int *row, int *col)
{
	for (int r = 0; r < l->nrows; r++)
		for (int i = 0; i < l->nkeys[r]; i++)
			if (!strcmp(l->rows[r][i].label, label)) {
				*row = r;
				*col = i;
				return &l->rows[r][i];
			}
	*row = -1;
	*col = 0;
	return NULL;
}

/* utf-8 string of the long-press keys of a key */
static const char *
alts(const struct lang *l, const char *label)
{
	static char buf[256];
	uint32_t cps[LANG_MAX_POPUPS];
	int row, col;
	find_key(l, label, &row, &col);
	int n = lang_alternates(l, label, row, col, cps, LANG_MAX_POPUPS), o = 0;
	for (int i = 0; i < n; i++)
		o += utf8_encode(cps[i], buf + o);
	buf[o] = 0;
	return buf;
}

static const char *
row(const struct lang *l, int r)
{
	static char buf[256];
	buf[0] = 0;
	for (int i = 0; i < l->nkeys[r]; i++)
		strcat(buf, l->rows[r][i].label);
	return buf;
}

static void
print(const struct lang *l)
{
	printf("%s (%s, layout %s)\n", l->name, l->code, l->layout);
	for (int r = 0; r < l->nrows; r++) {
		printf("  ");
		for (int i = 0; i < l->nkeys[r]; i++) {
			const struct lang_key *k = &l->rows[r][i];
			printf("%s", k->label);
			if (strcmp(k->label, k->shift))
				printf("/%s", k->shift);
			const char *a = alts(l, k->label);
			printf(*a ? "[%s] " : " ", a);
		}
		printf("\n");
	}
}

static const struct lang *
get(struct lang *ls, int n, const char *code)
{
	for (int i = 0; i < n; i++)
		if (!strcmp(ls[i].code, code))
			return &ls[i];
	printf("FAIL language %s not loaded\n", code);
	exit(1);
}

int
main(int argc, char **argv)
{
	const char *dirs = argc > 1 ? argv[1] : "data";
	bool check = argc > 2 && !strcmp(argv[2], "--check");
	const char *list = argc > 2 && !check ? argv[2] : "en,fi,sv,de,fr,ru,el,de-CH:qwertz+,uk";
	struct lang *ls;
	int n = langs_load(list, dirs, &ls);
	if (!check) {
		for (int i = 0; i < n; i++)
			print(&ls[i]);
		return 0;
	}

	const struct lang *fi = get(ls, n, "fi"), *en = get(ls, n, "en");
	/* Finnish is qwerty with å, ö, ä: the same as the old built-in layout */
	expect("fi row 1", row(fi, 0), "qwertyuiopå");
	expect("fi row 2", row(fi, 1), "asdfghjklöä");
	expect("fi row 3", row(fi, 2), "zxcvbnm");
	expect("fi name", fi->name, "Suomi");
	expect("fi q", alts(fi, "q"), "1");
	expect("fi e", alts(fi, "e"), "3éèêë€ē");
	expect("fi a", alts(fi, "a"), "áàâãæā"); /* ä å are keys already */
	expect("fi ö", alts(fi, "ö"), "øœ");
	expect("fi å", alts(fi, "å"), "");
	expect("fi s", alts(fi, "s"), "šß§");
	expect("fi .", alts(fi, "."), "?!…");
	expect("fi Ä", fi->rows[1][10].shift, "Ä");

	expect("en row 1", row(en, 0), "qwertyuiop");
	expect("en row 2", row(en, 1), "asdfghjkl");
	expect("en name", en->name, "English");
	expect("en a", alts(en, "a"), "áàâãæäåā");
	expect("en o", alts(en, "o"), "9óòôõøœöō");

	const struct lang *de = get(ls, n, "de");
	expect("de row 1", row(de, 0), "qwertzuiop");
	expect("de u", alts(de, "u"), "7üúùûū");

	const struct lang *ch = get(ls, n, "de-CH");
	expect("de-CH row 1", row(ch, 0), "qwertzuiopü");

	const struct lang *fr = get(ls, n, "fr");
	expect("fr row 1", row(fr, 0), "azertyuiop");
	expect("fr row 3", row(fr, 2), "wxcvbn'");
	expect("fr ' shift", fr->rows[2][6].shift, "?");
	expect("fr e", alts(fr, "e"), "3éèêë€ē");

	const struct lang *ru = get(ls, n, "ru");
	expect("ru row 1", row(ru, 0), "йцукенгшщзх");
	expect("ru Й", ru->rows[0][0].shift, "Й");
	expect("ru е", alts(ru, "е"), "5ёѣ");
	expect("ru name", ru->name, "Русский");

	const struct lang *el = get(ls, n, "el");
	expect("el row 1", row(el, 0), ";ςερτυθιοπ");

	if (failures)
		printf("%d failures\n", failures);
	else
		printf("all layout checks passed\n");
	return failures != 0;
}
