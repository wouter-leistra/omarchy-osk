/* offline check of the gesture decoder on a 2160px wide fi layout */
#include "../swipe.h"
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char *rows[3] = {"qwertyuiopå", "asdfghjklöä", "zxcvbnm"};
static const double offs[3] = {0, 0, 2.0}; /* shift key is 2 units wide */

int main(int argc, char **argv)
{
	struct swipe_geom g = {0};
	double W = 2160, unit = W / 11, kh = 120;
	g.kw = unit; g.kh = kh;
	for (int r = 0; r < 3; r++) {
		uint32_t cps[16];
		int n = utf8_decode(rows[r], cps, 16);
		for (int i = 0; i < n; i++)
			g.keys[g.nkeys++] = (struct swipe_key){cps[i], (offs[r] + i + 0.5) * unit, 84 + (r + 0.5) * kh};
	}
	swipe_dict_load(argv[1]);
	srand(1);
	int ok = 0, top3 = 0, total = 0;
	for (int a = 2; a < argc; a++) {
		uint32_t cps[32];
		int n = utf8_decode(argv[a], cps, 32);
		double xs[2000], ys[2000]; int np = 0;
		double jx = 0, jy = 0;
		for (int i = 0; i < n; i++) {
			int k = -1;
			for (int j = 0; j < g.nkeys; j++) if (g.keys[j].cp == cps[i]) k = j;
			if (k < 0) continue;
			/* sloppy finger: aim up to 0.3 key off centre */
			double tx = g.keys[k].cx + ((rand() % 61) - 30) / 100.0 * unit;
			double ty = g.keys[k].cy + ((rand() % 61) - 30) / 100.0 * kh;
			if (np) {
				double px = xs[np-1], py = ys[np-1];
				for (int s = 1; s <= 12; s++) { xs[np] = px + (tx-px)*s/12; ys[np] = py + (ty-py)*s/12; np++; }
			} else { xs[np] = tx; ys[np] = ty; np++; }
		}
		struct swipe_result r;
		swipe_decode(&g, xs, ys, np, &r);
		total++;
		int hit = r.n && !strcmp(r.words[0], argv[a]);
		int hit3 = 0; for (int i = 0; i < r.n; i++) hit3 |= !strcmp(r.words[i], argv[a]);
		ok += hit; top3 += hit3;
		printf("%-12s -> %s%s | %s | %s\n", argv[a], hit ? "" : "!! ", r.n > 0 ? r.words[0] : "-", r.n > 1 ? r.words[1] : "-", r.n > 2 ? r.words[2] : "-");
	}
	printf("top1 %d/%d  top3 %d/%d\n", ok, total, top3, total);
}
