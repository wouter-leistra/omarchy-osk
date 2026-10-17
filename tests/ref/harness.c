/* Reference harness: runs the C keyboard logic and drawing headless.
 *
 * libwayland is replaced by the stubs below, which print the virtual
 * keyboard requests, so a script of touches gives an exact log of what the
 * keyboard sends, and the drawing can be dumped to PNG. The Rust port has a
 * trace mode with the same arguments and script language (see
 * tests/ref/README.md), and both are compared by tests/ref/compare.py.
 *
 * usage: osk-ref [options] < script
 *   options as omarchy-oskd: --langs --data --dict --xkb --fn -R and the
 *   colour options, plus -W width -H height (default 2160x576)
 * script, one command per line:
 *   down X Y | move X Y | up | cancel   touch events
 *   longpress                           the long-press timer fires
 *   sleep MS                            real sleep (double taps, throttles)
 *   hide | show | reset | private on|off  what the signals do
 *   png FILE                            keyboard and popup surfaces as
 *                                       FILE.kbd.png and FILE.popup.png
 *   keymap FILE                         write the uploaded keymap
 *   # comment
 */
#define _GNU_SOURCE
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/mman.h>
#include <wayland-util.h>
#include <linux/input-event-codes.h>

#include "keyboard.h"
#include "config.omarchy.h"

/* ---------------------------------------------------------------------- */
/* fake libwayland                                                         */

const struct wl_interface wl_shm_pool_interface, wl_buffer_interface,
    wl_callback_interface, wl_region_interface, wl_surface_interface;

struct wl_proxy {
	int kind;
};

static struct wl_proxy vkbd_proxy = {1}, other_proxy = {2};
static char *keymap_text;

struct wl_proxy *
wl_proxy_marshal_flags(struct wl_proxy *proxy, uint32_t opcode,
                       const struct wl_interface *interface, uint32_t version,
                       uint32_t flags, ...)
{
	va_list ap;
	va_start(ap, flags);
	if (proxy == &vkbd_proxy) {
		if (opcode == 0) { /* keymap(format, fd, size) */
			uint32_t format = va_arg(ap, uint32_t);
			int fd = va_arg(ap, int);
			uint32_t size = va_arg(ap, uint32_t);
			free(keymap_text);
			keymap_text = calloc(1, size + 1);
			char *p = mmap(NULL, size, PROT_READ, MAP_SHARED, fd, 0);
			if (p != MAP_FAILED) {
				memcpy(keymap_text, p, size);
				munmap(p, size);
			}
			printf("keymap %u %u\n", format, size);
		} else if (opcode == 1) { /* key(time, key, state) */
			va_arg(ap, uint32_t);
			uint32_t key = va_arg(ap, uint32_t);
			uint32_t state = va_arg(ap, uint32_t);
			printf("key %u %u\n", key, state);
		} else if (opcode == 2) { /* modifiers(dep, lat, lock, group) */
			uint32_t d = va_arg(ap, uint32_t), l = va_arg(ap, uint32_t);
			uint32_t k = va_arg(ap, uint32_t), g = va_arg(ap, uint32_t);
			printf("mods %u %u %u %u\n", d, l, k, g);
		}
	}
	va_end(ap);
	return interface ? &other_proxy : NULL;
}

int wl_proxy_add_listener(struct wl_proxy *p, void (**impl)(void), void *data) { return 0; }
void wl_proxy_destroy(struct wl_proxy *p) {}
uint32_t wl_proxy_get_version(struct wl_proxy *p) { return 1; }
void wl_proxy_set_user_data(struct wl_proxy *p, void *d) {}
void *wl_proxy_get_user_data(struct wl_proxy *p) { return NULL; }

/* ---------------------------------------------------------------------- */

static struct kbd kb;
static struct drw ctx = {.shm = (void *)&other_proxy};
static struct drwbuf bufs[4];
static struct drwsurf surf, popup;

static void
set_color(uint8_t *bgra, const char *hex)
{
	size_t len = strlen(hex);
	if (len != 6 && len != 8)
		return;
	char sub[3] = {0};
	for (int i = 0; i < (int)len / 2; i++) {
		memcpy(sub, hex + i * 2, 2);
		int v = (int)strtol(sub, NULL, 16);
		bgra[i < 3 ? 2 - i : 3] = v;
	}
}

static void
dump(struct drwsurf *ds, const char *path)
{
	cairo_surface_flush(ds->back_buffer->cairo_surf);
	cairo_surface_write_to_png(ds->back_buffer->cairo_surf, path);
}

static void
do_hide(void)
{
	/* what main.c hide() does to the keyboard state */
	kbd_unpress_key(&kb, 0);
	kb.down_key = NULL;
	kb.swiping = false;
	kb.space_mode = SpaceTap;
	kb.longpress_at = 0;
	kb.alt_open = false;
	kbd_clear_mods(&kb);
	kb.sugg_pressed = -1;
	kbd_reset_text(&kb);
}

int
main(int argc, char **argv)
{
	uint32_t width = 2160, height = 576;
	double scale = 1;
	char *font = NULL, *dicts = NULL;
	int rounding = DEFAULT_ROUNDING;

	kb.schemes = schemes;
	kb.layers = (enum layout_id *)&layers;
	kb.landscape_layers = (enum layout_id *)&landscape_layers;
	kb.landscape = true;
	kb.show_popup = true;
	kb.show_highlight = true;
	kb.trail = schemes[0].swipe;
	kb.vkbd = (void *)&vkbd_proxy;

	for (int i = 1; i < argc; i++) {
		const char *a = argv[i], *v = i + 1 < argc ? argv[i + 1] : "";
		if (!strcmp(a, "-W")) width = atoi(argv[++i]);
		else if (!strcmp(a, "-S")) scale = atof(argv[++i]);
		else if (!strcmp(a, "-H")) height = atoi(argv[++i]);
		else if (!strcmp(a, "-R")) rounding = atoi(argv[++i]);
		else if (!strcmp(a, "--fn")) font = argv[++i];
		else if (!strcmp(a, "--langs")) kb.lang_list = argv[++i];
		else if (!strcmp(a, "--data")) kb.data_dirs = argv[++i];
		else if (!strcmp(a, "--dict")) dicts = argv[++i];
		else if (!strcmp(a, "--xkb")) {
			kb.xkb_layout = argv[++i];
			char *c = strchr(kb.xkb_layout, ':');
			if (c) { *c = 0; kb.xkb_variant = c + 1; }
		}
		else if (!strcmp(a, "--bg")) set_color(schemes[0].bg.bgra, v), i++;
		else if (!strcmp(a, "--fg")) set_color(schemes[0].fg.bgra, v), i++;
		else if (!strcmp(a, "--fg-sp")) set_color(schemes[1].fg.bgra, v), i++;
		else if (!strcmp(a, "--press")) set_color(schemes[0].high.bgra, v), i++;
		else if (!strcmp(a, "--press-sp")) set_color(schemes[1].high.bgra, v), i++;
		else if (!strcmp(a, "--text")) set_color(schemes[0].text.bgra, v), i++;
		else if (!strcmp(a, "--text-sp")) set_color(schemes[1].text.bgra, v), i++;
		else if (!strcmp(a, "--text-press")) set_color(schemes[0].text_press.bgra, v), i++;
		else if (!strcmp(a, "--text-press-sp")) set_color(schemes[1].text_press.bgra, v), i++;
		else if (!strcmp(a, "--fg-accent")) set_color(schemes[2].fg.bgra, v), i++;
		else if (!strcmp(a, "--press-accent")) set_color(schemes[2].high.bgra, v), i++;
		else if (!strcmp(a, "--text-accent")) {
			set_color(schemes[2].text.bgra, v);
			set_color(schemes[2].text_press.bgra, v);
			i++;
		}
		else if (!strcmp(a, "--trail")) set_color(kb.trail.bgra, v), i++;
		else { fprintf(stderr, "unknown option %s\n", a); return 2; }
	}
	for (size_t i = 1; i < sizeof(schemes) / sizeof(*schemes); i++)
		schemes[i].bg = schemes[0].bg;
	for (size_t i = 0; i < sizeof(schemes) / sizeof(*schemes); i++) {
		if (font)
			schemes[i].font = font;
		if (rounding != DEFAULT_ROUNDING)
			schemes[i].rounding = rounding;
	}

	if (dicts)
		swipe_dict_load(dicts);
	kb.swipe_enabled = swipe_dict_loaded();
	if (!kb.lang_list && swipe_lang_count()) {
		static char list[256];
		for (int i = 0; i < swipe_lang_count(); i++)
			snprintf(list + strlen(list), sizeof(list) - strlen(list), "%s%s",
			         i ? "," : "", swipe_lang_code(i));
		kb.lang_list = list;
	}

	kbd_init(&kb, (struct layout *)&layouts, NULL, NULL);
	for (size_t i = 0; i < sizeof(schemes) / sizeof(*schemes); i++)
		schemes[i].font_description =
		    pango_font_description_from_string(schemes[i].font);

	surf.ctx = popup.ctx = &ctx;
	surf.back_buffer = &bufs[0];
	surf.display_buffer = &bufs[1];
	popup.back_buffer = &bufs[2];
	popup.display_buffer = &bufs[3];
	kb.surf = &surf;
	kb.popup_surf = &popup;

	/* the layer surface configure of main.c */
	kb.w = width;
	kb.h = height;
	kb.scale = scale;
	kb.layout = &kb.layouts[kb.layers[0]];
	kb.prevlayout = kb.last_abc_layout = kb.layout;
	kbd_resize(&kb, layouts, NumLayouts);

	char line[512];
	uint32_t t = 1000;
	while (fgets(line, sizeof(line), stdin)) {
		char cmd[32] = "", arg[400] = "";
		int x = 0, y = 0;
		if (sscanf(line, "%31s", cmd) != 1 || cmd[0] == '#')
			continue;
		t += 10;
		printf("> %s", line);
		if (!strcmp(cmd, "down") && sscanf(line, "%*s %d %d", &x, &y) == 2)
			kbd_touch_down(&kb, t, x, y);
		else if (!strcmp(cmd, "move") && sscanf(line, "%*s %d %d", &x, &y) == 2)
			kbd_touch_motion(&kb, t, x, y);
		else if (!strcmp(cmd, "up"))
			kbd_touch_up(&kb, t);
		else if (!strcmp(cmd, "cancel"))
			kbd_touch_cancel(&kb, t);
		else if (!strcmp(cmd, "longpress")) {
			if (kb.longpress_at)
				kbd_longpress(&kb);
		} else if (!strcmp(cmd, "sleep") && sscanf(line, "%*s %d", &x) == 1)
			usleep(x * 1000);
		else if (!strcmp(cmd, "hide"))
			do_hide();
		else if (!strcmp(cmd, "show"))
			kbd_resize(&kb, layouts, NumLayouts);
		else if (!strcmp(cmd, "reset"))
			kbd_reset_text(&kb);
		else if (!strcmp(cmd, "private") && sscanf(line, "%*s %399s", arg) == 1)
			kbd_set_private(&kb, !strcmp(arg, "on"));
		else if (!strcmp(cmd, "png") && sscanf(line, "%*s %399s", arg) == 1) {
			char p[512];
			snprintf(p, sizeof(p), "%s.kbd.png", arg);
			dump(&surf, p);
			snprintf(p, sizeof(p), "%s.popup.png", arg);
			dump(&popup, p);
		} else if (!strcmp(cmd, "keymap") && sscanf(line, "%*s %399s", arg) == 1) {
			FILE *f = fopen(arg, "w");
			if (f) {
				fputs(keymap_text ? keymap_text : "", f);
				fclose(f);
			}
		} else
			printf("? unknown command\n");
		/* the state a script can check */
		printf("= layout=%s lang=%s mods=%u word=\"%s\" sugg=%d", kb.layout->name,
		       kb.langs[kb.cur_lang].code, kb.mods, kb.word, kb.sugg.n);
		for (int i = 0; i < kb.sugg.n; i++)
			printf(" \"%s\"", kb.sugg.words[i]);
		printf(" alt=%d", kb.alt_open ? kb.alt_n : 0);
		printf("\n");
		fflush(stdout);
	}
	return 0;
}
