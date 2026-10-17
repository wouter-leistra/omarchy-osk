/* omarchy-osk: layouts and long-press keys per language, see langs.h */
#define _GNU_SOURCE
#include "langs.h"
#include "swipe.h"

#include <ctype.h>
#include <locale.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <wctype.h>

/* ---------------------------------------------------------------------- */
/* unicode                                                                 */

static locale_t
utf8_locale(void)
{
	static locale_t loc;
	static bool tried;
	if (!tried) {
		tried = true;
		loc = newlocale(LC_CTYPE_MASK, "C.UTF-8", (locale_t)0);
		if (!loc)
			loc = newlocale(LC_CTYPE_MASK, "en_US.UTF-8", (locale_t)0);
	}
	return loc;
}

uint32_t
lang_upper(uint32_t cp)
{
	locale_t loc = utf8_locale();
	if (loc)
		return towupper_l(cp, loc);
	if ((cp >= 'a' && cp <= 'z') || (cp >= 0xE0 && cp <= 0xFE && cp != 0xF7))
		return cp - 32;
	return cp;
}

uint32_t
lang_lower(uint32_t cp)
{
	locale_t loc = utf8_locale();
	if (loc)
		return towlower_l(cp, loc);
	if ((cp >= 'A' && cp <= 'Z') || (cp >= 0xC0 && cp <= 0xDE && cp != 0xD7))
		return cp + 32;
	return cp;
}

bool
lang_is_letter(uint32_t cp)
{
	locale_t loc = utf8_locale();
	if (loc)
		return iswalpha_l(cp, loc);
	return lang_upper(cp) != lang_lower(cp);
}

/* ---------------------------------------------------------------------- */
/* files                                                                   */

/* first existing dir/name in the ':' separated search path */
static FILE *
open_data(const char *dirs, const char *name)
{
	char path[1024];
	const char *p = dirs;
	while (p && *p) {
		const char *end = strchr(p, ':');
		int len = end ? (int)(end - p) : (int)strlen(p);
		if (len) {
			snprintf(path, sizeof(path), "%.*s/%s", len, p, name);
			FILE *f = fopen(path, "r");
			if (f)
				return f;
		}
		p = end ? end + 1 : NULL;
	}
	return NULL;
}

/* split a line into whitespace separated tokens, in place */
static int
tokenize(char *line, char **tok, int max)
{
	int n = 0;
	for (char *s = strtok(line, " \t\r\n"); s && n < max; s = strtok(NULL, " \t\r\n"))
		tok[n++] = s;
	return n;
}

/* The text a HeliBoard token types, or NULL for tokens that are not plain
 * text (!icon/..., !autoColumnOrder!4, %, $$$ currency placeholders). */
static const char *
token_text(char *t)
{
	if (t[0] == '\\' && t[1])
		return t + 1;
	if (!strcmp(t, "%") || strstr(t, "$$$"))
		return NULL;
	if (t[0] == '!' && t[1])
		return NULL;
	char *bar = strchr(t, '|');
	if (bar && bar != t && bar[1])
		return bar[1] == '!' ? NULL : bar + 1;
	return t;
}

/* append the token as a long-press key if it is a single character */
static void
add_popup(uint32_t *cps, int *n, const char *text)
{
	uint32_t buf[2];
	if (!text || *n >= LANG_MAX_POPUPS || utf8_decode(text, buf, 2) != 1)
		return;
	for (int i = 0; i < *n; i++)
		if (cps[i] == buf[0])
			return;
	cps[(*n)++] = buf[0];
}

/* fill a key from the tokens of one layout line ("label popup ...") */
static bool
parse_key(struct lang_key *k, char **tok, int ntok)
{
	const char *label = ntok ? token_text(tok[0]) : NULL;
	if (!label)
		return false;
	memset(k, 0, sizeof(*k));
	snprintf(k->label, sizeof(k->label), "%s", label);
	for (int i = 1; i < ntok; i++) {
		if (!strncmp(tok[i], "!shift!", 7) && tok[i][7])
			snprintf(k->shift, sizeof(k->shift), "%s", tok[i] + 7);
		else
			add_popup(k->popups, &k->npopups, token_text(tok[i]));
	}
	if (!k->shift[0]) {
		/* upper case of every character */
		uint32_t cps[LANG_LABEL];
		int n = utf8_decode(k->label, cps, LANG_LABEL), o = 0;
		for (int i = 0; i < n && o < LANG_LABEL - 5; i++)
			o += utf8_encode(lang_upper(cps[i]), k->shift + o);
		k->shift[o] = 0;
	}
	return true;
}

static bool
load_layout(struct lang *l, const char *dirs)
{
	char name[80], line[512];
	snprintf(name, sizeof(name), "layouts/%s.txt", l->layout);
	FILE *f = open_data(dirs, name);
	if (!f)
		return false;
	l->nrows = 0;
	memset(l->nkeys, 0, sizeof(l->nkeys));
	bool in_row = false;
	while (fgets(line, sizeof(line), f)) {
		char *tok[LANG_MAX_POPUPS + 4];
		int ntok = tokenize(line, tok, LANG_MAX_POPUPS + 4);
		if (!ntok) {
			in_row = false;
			continue;
		}
		if (!in_row) {
			if (l->nrows == LANG_MAX_ROWS)
				break;
			l->nrows++;
			in_row = true;
		}
		int r = l->nrows - 1;
		if (l->nkeys[r] < LANG_MAX_ROW_KEYS &&
		    parse_key(&l->rows[r][l->nkeys[r]], tok, ntok))
			l->nkeys[r]++;
	}
	fclose(f);
	return l->nrows > 0;
}

static void
add_lang_popup(struct lang_popup **list, int *n, char **tok, int ntok)
{
	const char *label = token_text(tok[0]);
	if (!label || ntok < 2)
		return;
	if (!strcmp(label, "punctuation"))
		label = ".";
	struct lang_popup *p = NULL;
	for (int i = 0; i < *n; i++)
		if (!strcmp((*list)[i].label, label))
			p = &(*list)[i];
	if (!p) {
		*list = realloc(*list, (*n + 1) * sizeof(**list));
		p = &(*list)[(*n)++];
		memset(p, 0, sizeof(*p));
		snprintf(p->label, sizeof(p->label), "%s", label);
	}
	for (int i = 1; i < ntok; i++)
		add_popup(p->cps, &p->n, token_text(tok[i]));
}

/* read a locale_key_texts style file; extra keys only for "+" layouts */
static bool
load_locale(const char *dirs, const char *name, struct lang_popup **popups,
            int *npopups, struct lang *l, bool extra_keys)
{
	char path[80], line[512];
	snprintf(path, sizeof(path), "%s", name);
	FILE *f = open_data(dirs, path);
	if (!f)
		return false;
	char section[32] = "";
	while (fgets(line, sizeof(line), f)) {
		char *tok[LANG_MAX_POPUPS + 4];
		if (line[0] == '[') {
			sscanf(line, "[%31[^]]", section);
			continue;
		}
		int ntok = tokenize(line, tok, LANG_MAX_POPUPS + 4);
		if (!ntok)
			continue;
		if (!strcmp(section, "popup_keys")) {
			add_lang_popup(popups, npopups, tok, ntok);
		} else if (!strcmp(section, "extra_keys") && l && extra_keys) {
			/* "2: ö ø" appends ö (long press ø) to the second row */
			int row = atoi(tok[0]) - 1;
			if (row < 0 || row >= l->nrows || ntok < 2 ||
			    l->nkeys[row] >= LANG_MAX_ROW_KEYS)
				continue;
			if (parse_key(&l->rows[row][l->nkeys[row]], tok + 1, ntok - 1))
				l->nkeys[row]++;
		} else if (!strcmp(section, "number_row") && l && ntok == 10) {
			for (int i = 0; i < 10; i++) {
				uint32_t cp;
				if (utf8_decode(tok[i], &cp, 1) == 1)
					l->number_row[i] = cp;
			}
		}
	}
	fclose(f);
	return true;
}

/* languages.txt: exact tag first, then the first entry of the same
 * language ("en" finds "en-US", "de-AT" finds "de") */
static void
lookup_language(const char *dirs, const char *tag, char *layout, size_t llen,
                char *name, size_t nlen)
{
	char base[16], line[256], best_layout[48] = "", best_name[48] = "";
	int best = 0;
	snprintf(base, sizeof(base), "%.*s", (int)strcspn(tag, "-"), tag);
	FILE *f = open_data(dirs, "languages.txt");
	while (f && fgets(line, sizeof(line), f)) {
		char t[16], lay[48];
		int off = 0;
		if (line[0] == '#' || sscanf(line, "%15s %47s %n", t, lay, &off) < 2)
			continue;
		char *nm = line + off;
		nm[strcspn(nm, "\r\n")] = 0;
		int score = !strcasecmp(t, tag)                          ? 3
		            : !strcasecmp(t, base)                       ? 2
		            : !strncasecmp(t, base, strlen(base)) &&
		                      t[strlen(base)] == '-'             ? 1
		                                                         : 0;
		if (score > best) {
			best = score;
			snprintf(best_layout, sizeof(best_layout), "%s", lay);
			snprintf(best_name, sizeof(best_name), "%s", *nm ? nm : t);
		}
	}
	if (f)
		fclose(f);
	snprintf(layout, llen, "%s", best ? best_layout : "qwerty");
	snprintf(name, nlen, "%s", best ? best_name : tag);
}

/* popups.txt and HeliBoard's common accents, for every language */
static struct lang_popup *common, *accents;
static int ncommon, naccents;

int
langs_load(const char *list, const char *dirs, struct lang **out)
{
	*out = NULL;
	if (!dirs)
		return 0;
	if (!common)
		load_locale(dirs, "popups.txt", &common, &ncommon, NULL, false);
	if (!accents)
		load_locale(dirs, "locales/more_popups_main.txt", &accents, &naccents,
		            NULL, false);
	if (!list)
		return 0;

	char *copy = strdup(list), *save = NULL;
	int n = 0;
	for (char *item = strtok_r(copy, ",", &save); item;
	     item = strtok_r(NULL, ",", &save)) {
		while (isspace((unsigned char)*item))
			item++;
		if (!*item)
			continue;
		struct lang *l = calloc(1, sizeof(*l));
		char *colon = strchr(item, ':');
		if (colon)
			*colon = 0;
		snprintf(l->code, sizeof(l->code), "%s", item);
		for (char *c = l->code; *c; c++)
			if (*c == '_')
				*c = '-';

		char layout[48];
		lookup_language(dirs, l->code, layout, sizeof(layout), l->name,
		                sizeof(l->name));
		if (colon && colon[1])
			snprintf(layout, sizeof(layout), "%s", colon + 1);
		bool plus = layout[0] && layout[strlen(layout) - 1] == '+';
		if (plus)
			layout[strlen(layout) - 1] = 0;
		snprintf(l->layout, sizeof(l->layout), "%s", layout);

		if (!load_layout(l, dirs)) {
			fprintf(stderr, "omarchy-osk: no layout %s for language %s\n",
			        l->layout, l->code);
			free(l);
			continue;
		}
		static const char digits[] = "1234567890";
		for (int i = 0; i < 10; i++)
			l->number_row[i] = digits[i];

		char name[64], base[16];
		snprintf(name, sizeof(name), "locales/%s.txt", l->code);
		snprintf(base, sizeof(base), "%.*s", (int)strcspn(l->code, "-"), l->code);
		if (!load_locale(dirs, name, &l->popups, &l->npopups, l, plus)) {
			snprintf(name, sizeof(name), "locales/%s.txt", base);
			load_locale(dirs, name, &l->popups, &l->npopups, l, plus);
		}

		*out = realloc(*out, (n + 1) * sizeof(**out));
		(*out)[n++] = *l;
		free(l);
	}
	free(copy);
	return n;
}

/* ---------------------------------------------------------------------- */
/* long-press keys                                                         */

static const struct lang_popup *
find_popup(const struct lang_popup *list, int n, const char *label)
{
	for (int i = 0; i < n; i++)
		if (!strcmp(list[i].label, label))
			return &list[i];
	return NULL;
}

static bool
in_layout(const struct lang *l, uint32_t cp)
{
	for (int r = 0; r < l->nrows; r++)
		for (int i = 0; i < l->nkeys[r]; i++) {
			uint32_t k[2];
			if (utf8_decode(l->rows[r][i].label, k, 2) == 1 && k[0] == cp)
				return true;
		}
	return false;
}

struct alt_set {
	uint32_t *out;
	int n, max;
	uint32_t self;
	const struct lang *skip_keys; /* leave out keys of this layout */
};

static void
alt_add(struct alt_set *s, uint32_t cp)
{
	if (!cp || cp == s->self || s->n >= s->max)
		return;
	for (int i = 0; i < s->n; i++)
		if (s->out[i] == cp)
			return;
	if (s->skip_keys && in_layout(s->skip_keys, cp))
		return;
	s->out[s->n++] = cp;
}

static void
alt_add_list(struct alt_set *s, const struct lang_popup *p)
{
	for (int i = 0; p && i < p->n; i++)
		alt_add(s, p->cps[i]);
}

int
lang_alternates(const struct lang *l, const char *label, int row, int col,
                uint32_t *out, int max)
{
	uint32_t self[2];
	bool single = utf8_decode(label, self, 2) == 1;
	bool letter = single && lang_is_letter(self[0]);
	struct alt_set s = {out, 0, max, single ? self[0] : 0,
	                    letter ? l : NULL};

	if (l && row == 0 && col < 10 && l->nkeys[0] >= 10) {
		/* digits go first even when the layout has them as keys */
		const struct lang *skip = s.skip_keys;
		s.skip_keys = NULL;
		alt_add(&s, l->number_row[col]);
		s.skip_keys = skip;
	}
	if (l && row >= 0 && row < l->nrows && col < l->nkeys[row])
		for (int i = 0; i < l->rows[row][col].npopups; i++)
			alt_add(&s, l->rows[row][col].popups[i]);
	if (l)
		alt_add_list(&s, find_popup(l->popups, l->npopups, label));
	alt_add_list(&s, find_popup(common, ncommon, label));
	if (letter)
		alt_add_list(&s, find_popup(accents, naccents, label));
	return s.n;
}
