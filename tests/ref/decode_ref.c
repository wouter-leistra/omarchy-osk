/* decoder reference: geometry file + paths on stdin -> top results
 *   decode_ref DICTS GEOMETRY < paths
 * GEOMETRY: "kw kh" then "cp cx cy" per key; a path is one line of
 * "x,y" points, or "?prefix" for completions; LANG code switches language */
#include "../../swipe.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int
main(int argc, char **argv)
{
	if (argc < 3)
		return 2;
	swipe_dict_load(argv[1]);
	struct swipe_geom g = {0};
	FILE *f = fopen(argv[2], "r");
	if (!f || fscanf(f, "%lf %lf", &g.kw, &g.kh) != 2)
		return 2;
	unsigned cp;
	double x, y;
	while (g.nkeys < SWIPE_MAX_KEYS && fscanf(f, "%u %lf %lf", &cp, &x, &y) == 3)
		g.keys[g.nkeys++] = (struct swipe_key){cp, x, y};
	fclose(f);
	static char line[1 << 16];
	static double xs[4096], ys[4096];
	while (fgets(line, sizeof(line), stdin)) {
		line[strcspn(line, "\n")] = 0;
		struct swipe_result r;
		if (!strncmp(line, "LANG ", 5)) {
			swipe_set_lang_code(line + 5);
			printf("lang %s\n", line + 5);
			continue;
		}
		if (line[0] == '?') {
			swipe_complete(line + 1, &r);
		} else {
			int n = 0;
			for (char *p = strtok(line, " "); p && n < 4096; p = strtok(NULL, " "))
				if (sscanf(p, "%lf,%lf", &xs[n], &ys[n]) == 2)
					n++;
			swipe_decode(&g, xs, ys, n, &r);
		}
		printf("%d", r.n);
		for (int i = 0; i < r.n; i++)
			printf(" %s", r.words[i]);
		printf("\n");
	}
	return 0;
}
