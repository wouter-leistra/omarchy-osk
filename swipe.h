#ifndef __SWIPE_H
#define __SWIPE_H

#include <stdbool.h>
#include <stdint.h>

#define SWIPE_MAX_WORD 48   /* bytes of utf-8 per word */
#define SWIPE_MAX_CPS 32    /* codepoints per word */
#define SWIPE_MAX_KEYS 64
#define SWIPE_MAX_RESULTS 3

/* geometry of a letter key, in surface pixels */
struct swipe_key {
	uint32_t cp; /* lowercase codepoint the key types */
	double cx, cy;
};

struct swipe_geom {
	struct swipe_key keys[SWIPE_MAX_KEYS];
	int nkeys;
	double kw, kh; /* typical letter key width/height */
};

struct swipe_result {
	char words[SWIPE_MAX_RESULTS][SWIPE_MAX_WORD];
	int n;
};

/* Load one or more "word count" frequency lists (comma separated paths).
 * Returns number of words loaded. */
int swipe_dict_load(const char *paths);
bool swipe_dict_loaded(void);

/* languages are the dictionary files, in the order given */
int swipe_lang_count(void);
const char *swipe_lang_code(int i);
void swipe_set_lang(int i); /* -1 for all */
int swipe_get_lang(void);

/* Decode a gesture path into the most likely words. */
void swipe_decode(const struct swipe_geom *g, const double *xs,
                  const double *ys, int npts, struct swipe_result *out);

/* Most frequent dictionary words starting with prefix (lowercase utf-8),
 * excluding the prefix itself. */
void swipe_complete(const char *prefix, struct swipe_result *out);

/* utf-8 helpers */
int utf8_decode(const char *s, uint32_t *cps, int max);
int utf8_encode(uint32_t cp, char *out);
int utf8_len(const char *s);

#endif
