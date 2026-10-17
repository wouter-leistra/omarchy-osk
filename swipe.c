/* Gesture ("swipe") typing decoder for omarchy-osk.
 *
 * The finger path is resampled and compared against the ideal path through
 * the key centres of every plausible dictionary word (same start and end
 * key region). Shape distance is combined with word frequency, which is
 * roughly how SHARK2-style decoders used by Gboard/SwiftKey started out.
 */
#include "swipe.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define RESAMPLE 48
#define SHAPE_WEIGHT 9.0  /* nats per key-width of mean path deviation */
#define LEN_WEIGHT 2.5    /* penalty for path length mismatch */
#define END_RADIUS 1.15   /* start/end key must be within this many keys */
#define PASS_RADIUS 0.9   /* every letter must be passed this closely */

struct word {
	char utf8[SWIPE_MAX_WORD];
	uint32_t cps[SWIPE_MAX_CPS];
	uint8_t ncps;
	float lp; /* log relative frequency, <= 0 */
	uint8_t lang;
};

static struct word *words;
static int nwords, capwords;

#define MAX_LANGS 8
static char lang_codes[MAX_LANGS][16];
static int nlangs;
#define NO_LANG -2
static int active_lang = -1; /* -1: all languages, NO_LANG: none */

int
swipe_lang_count(void)
{
	return nlangs;
}

const char *
swipe_lang_code(int i)
{
	return i >= 0 && i < nlangs ? lang_codes[i] : "";
}

void
swipe_set_lang(int i)
{
	active_lang = i >= 0 && i < nlangs ? i : -1;
}

int
swipe_get_lang(void)
{
	return active_lang;
}

bool
swipe_set_lang_code(const char *code)
{
	/* "fi" for fi.txt; "de-CH" falls back to de.txt */
	size_t base = strcspn(code, "-_");
	for (int pass = 0; pass < 2; pass++)
		for (int i = 0; i < nlangs; i++)
			if (pass ? !strncmp(lang_codes[i], code, base) && !lang_codes[i][base]
			         : !strcmp(lang_codes[i], code)) {
				active_lang = i;
				return true;
			}
	active_lang = nlangs ? NO_LANG : -1;
	return !nlangs;
}

bool
swipe_lang_active(void)
{
	return nwords && active_lang != NO_LANG;
}

static bool
lang_ok(const struct word *w)
{
	return active_lang == -1 || w->lang == active_lang;
}

int
utf8_decode(const char *s, uint32_t *cps, int max)
{
	const unsigned char *p = (const unsigned char *)s;
	int n = 0;
	while (*p && n < max) {
		uint32_t c;
		int len;
		if (*p < 0x80) {
			c = *p;
			len = 1;
		} else if ((*p & 0xE0) == 0xC0) {
			c = *p & 0x1F;
			len = 2;
		} else if ((*p & 0xF0) == 0xE0) {
			c = *p & 0x0F;
			len = 3;
		} else {
			c = *p & 0x07;
			len = 4;
		}
		for (int i = 1; i < len; i++) {
			if (!p[i])
				return n;
			c = (c << 6) | (p[i] & 0x3F);
		}
		p += len;
		cps[n++] = c;
	}
	return n;
}

int
utf8_encode(uint32_t cp, char *out)
{
	if (cp < 0x80) {
		out[0] = cp;
		return 1;
	} else if (cp < 0x800) {
		out[0] = 0xC0 | (cp >> 6);
		out[1] = 0x80 | (cp & 0x3F);
		return 2;
	} else if (cp < 0x10000) {
		out[0] = 0xE0 | (cp >> 12);
		out[1] = 0x80 | ((cp >> 6) & 0x3F);
		out[2] = 0x80 | (cp & 0x3F);
		return 3;
	}
	out[0] = 0xF0 | (cp >> 18);
	out[1] = 0x80 | ((cp >> 12) & 0x3F);
	out[2] = 0x80 | ((cp >> 6) & 0x3F);
	out[3] = 0x80 | (cp & 0x3F);
	return 4;
}

int
utf8_len(const char *s)
{
	int n = 0;
	for (; *s; s++)
		if ((*s & 0xC0) != 0x80)
			n++;
	return n;
}

static int
load_one(const char *path, uint8_t lang)
{
	FILE *f = fopen(path, "r");
	if (!f) {
		fprintf(stderr, "omarchy-osk: cannot open dictionary %s\n", path);
		return 0;
	}
	char line[256];
	double maxcount = 0;
	int loaded = 0;
	while (fgets(line, sizeof(line), f)) {
		char w[SWIPE_MAX_WORD];
		double count;
		if (sscanf(line, "%47s %lf", w, &count) != 2)
			continue;
		if (maxcount == 0)
			maxcount = count;
		if (nwords == capwords) {
			capwords = capwords ? capwords * 2 : 65536;
			words = realloc(words, capwords * sizeof(*words));
		}
		struct word *wd = &words[nwords];
		snprintf(wd->utf8, sizeof(wd->utf8), "%s", w);
		wd->ncps = utf8_decode(w, wd->cps, SWIPE_MAX_CPS);
		wd->lp = (float)log(count / maxcount);
		wd->lang = lang;
		nwords++;
		loaded++;
	}
	fclose(f);
	return loaded;
}

static int
cmp_alpha(const void *a, const void *b)
{
	const struct word *x = a, *y = b;
	int c = strcmp(x->utf8, y->utf8);
	if (c)
		return c;
	if (x->lang != y->lang)
		return x->lang - y->lang;
	return (x->lp < y->lp) - (x->lp > y->lp);
}

static int
cmp_freq(const void *a, const void *b)
{
	const struct word *x = a, *y = b;
	return (x->lp < y->lp) - (x->lp > y->lp);
}

int
swipe_dict_load(const char *paths)
{
	char *copy = strdup(paths), *save = NULL;
	for (char *p = strtok_r(copy, ",", &save); p && nlangs < MAX_LANGS;
	     p = strtok_r(NULL, ",", &save)) {
		/* language code from the file name, e.g. ".../fi.txt" -> "fi" */
		const char *base = strrchr(p, '/');
		base = base ? base + 1 : p;
		snprintf(lang_codes[nlangs], sizeof(lang_codes[nlangs]), "%.*s",
		         (int)strcspn(base, "."), base);
		load_one(p, nlangs);
		nlangs++;
	}
	free(copy);

	/* words shared between languages keep their best frequency, and the
	 * merged list is ordered by frequency for completions */
	qsort(words, nwords, sizeof(*words), cmp_alpha);
	int out = 0;
	for (int i = 0; i < nwords; i++)
		if (!out || strcmp(words[out - 1].utf8, words[i].utf8) ||
		    words[out - 1].lang != words[i].lang)
			words[out++] = words[i];
	nwords = out;
	qsort(words, nwords, sizeof(*words), cmp_freq);

	fprintf(stderr, "omarchy-osk: %d dictionary words\n", nwords);
	return nwords;
}

bool
swipe_dict_loaded(void)
{
	return nwords > 0;
}

static void
insert_result(struct swipe_result *out, double *costs, const char *w,
              double cost)
{
	for (int i = 0; i < out->n; i++)
		if (!strcmp(out->words[i], w))
			return;
	int pos = out->n;
	while (pos > 0 && costs[pos - 1] > cost)
		pos--;
	if (pos >= SWIPE_MAX_RESULTS)
		return;
	int last = out->n < SWIPE_MAX_RESULTS ? out->n : SWIPE_MAX_RESULTS - 1;
	for (int i = last; i > pos; i--) {
		memcpy(out->words[i], out->words[i - 1], SWIPE_MAX_WORD);
		costs[i] = costs[i - 1];
	}
	snprintf(out->words[pos], SWIPE_MAX_WORD, "%s", w);
	costs[pos] = cost;
	if (out->n < SWIPE_MAX_RESULTS)
		out->n++;
}

/* resample a polyline to RESAMPLE evenly spaced points, returns length */
static double
resample(const double *xs, const double *ys, int n, double *ox, double *oy)
{
	double len = 0;
	for (int i = 1; i < n; i++)
		len += hypot(xs[i] - xs[i - 1], ys[i] - ys[i - 1]);
	if (n == 1 || len == 0) {
		for (int i = 0; i < RESAMPLE; i++) {
			ox[i] = xs[0];
			oy[i] = ys[0];
		}
		return 0;
	}
	double step = len / (RESAMPLE - 1), acc = 0;
	int seg = 1, k = 1;
	ox[0] = xs[0];
	oy[0] = ys[0];
	double px = xs[0], py = ys[0];
	while (k < RESAMPLE - 1 && seg < n) {
		double d = hypot(xs[seg] - px, ys[seg] - py);
		if (acc + d >= step && d > 0) {
			double t = (step - acc) / d;
			px = px + t * (xs[seg] - px);
			py = py + t * (ys[seg] - py);
			ox[k] = px;
			oy[k] = py;
			k++;
			acc = 0;
		} else {
			acc += d;
			px = xs[seg];
			py = ys[seg];
			seg++;
		}
	}
	for (; k < RESAMPLE; k++) {
		ox[k] = xs[n - 1];
		oy[k] = ys[n - 1];
	}
	return len;
}

static int
key_index(const struct swipe_geom *g, uint32_t cp)
{
	for (int i = 0; i < g->nkeys; i++)
		if (g->keys[i].cp == cp)
			return i;
	return -1;
}

void
swipe_decode(const struct swipe_geom *g, const double *xs, const double *ys,
             int npts, struct swipe_result *out)
{
	out->n = 0;
	if (!nwords || npts < 2 || g->nkeys == 0)
		return;

	/* work in key units so that horizontal and vertical error weigh alike */
	double *nx = malloc(npts * sizeof(double)), *ny = malloc(npts * sizeof(double));
	for (int i = 0; i < npts; i++) {
		nx[i] = xs[i] / g->kw;
		ny[i] = ys[i] / g->kh;
	}
	double px[RESAMPLE], py[RESAMPLE];
	double plen = resample(nx, ny, npts, px, py);

	bool start_ok[SWIPE_MAX_KEYS], end_ok[SWIPE_MAX_KEYS];
	double kx[SWIPE_MAX_KEYS], ky[SWIPE_MAX_KEYS];
	for (int i = 0; i < g->nkeys; i++) {
		kx[i] = g->keys[i].cx / g->kw;
		ky[i] = g->keys[i].cy / g->kh;
		start_ok[i] = hypot(kx[i] - nx[0], ky[i] - ny[0]) <= END_RADIUS;
		end_ok[i] = hypot(kx[i] - nx[npts - 1], ky[i] - ny[npts - 1]) <= END_RADIUS;
	}

	double costs[SWIPE_MAX_RESULTS];
	double tx[SWIPE_MAX_CPS], ty[SWIPE_MAX_CPS];
	double rx[RESAMPLE], ry[RESAMPLE];

	for (int w = 0; w < nwords; w++) {
		struct word *wd = &words[w];
		if (!lang_ok(wd))
			continue;
		/* template: key centres of the letters, repeats collapsed,
		 * apostrophes/hyphens are not swiped */
		int nt = 0, first = -1, last = -1;
		bool bad = false;
		for (int c = 0; c < wd->ncps; c++) {
			if (wd->cps[c] == '\'' || wd->cps[c] == '-')
				continue;
			int ki = key_index(g, wd->cps[c]);
			if (ki < 0) {
				bad = true;
				break;
			}
			if (first < 0) {
				first = ki;
				if (!start_ok[ki]) {
					bad = true;
					break;
				}
			}
			if (last == ki)
				continue;
			last = ki;
			tx[nt] = kx[ki];
			ty[nt] = ky[ki];
			nt++;
		}
		if (bad || nt < 1 || !end_ok[last])
			continue;
		if (nt == 1 && plen > 1.2)
			continue;

		/* every letter has to be passed, in order */
		int pi = 0;
		for (int t = 0; t < nt && !bad; t++) {
			while (pi < npts && hypot(nx[pi] - tx[t], ny[pi] - ty[t]) > PASS_RADIUS)
				pi++;
			if (pi >= npts)
				bad = true;
		}
		if (bad)
			continue;

		double tlen = resample(tx, ty, nt, rx, ry);
		double d = 0;
		for (int i = 0; i < RESAMPLE; i++)
			d += hypot(rx[i] - px[i], ry[i] - py[i]);
		d /= RESAMPLE;
		double lenpen = fabs(plen - tlen) / (plen > tlen ? plen : tlen + 1e-6);

		double cost = d * SHAPE_WEIGHT + lenpen * LEN_WEIGHT - wd->lp;
		insert_result(out, costs, wd->utf8, cost);
	}
	free(nx);
	free(ny);
}

void
swipe_complete(const char *prefix, struct swipe_result *out)
{
	out->n = 0;
	size_t plen = strlen(prefix);
	if (!plen)
		return;
	/* the lists are sorted by frequency, so the first hits are the best */
	for (int i = 0; i < nwords && out->n < SWIPE_MAX_RESULTS; i++) {
		if (!lang_ok(&words[i]) || strncmp(words[i].utf8, prefix, plen) ||
		    !words[i].utf8[plen])
			continue;
		bool dup = false;
		for (int j = 0; j < out->n; j++)
			dup |= !strcmp(out->words[j], words[i].utf8);
		if (!dup)
			snprintf(out->words[out->n++], SWIPE_MAX_WORD, "%s", words[i].utf8);
	}
}
