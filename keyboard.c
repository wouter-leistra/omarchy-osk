#include "proto/virtual-keyboard-unstable-v1-client-protocol.h"
#include <linux/input-event-codes.h>
#include <math.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <time.h>
#include <unistd.h>
#include "keyboard.h"

#include "drw.h"
#include "os-compatibility.h"

/* lazy die macro */
#define die(...)                                                               \
    fprintf(stderr, __VA_ARGS__);                                              \
    exit(1)

#ifndef KEYMAP
#error "make sure to define KEYMAP"
#endif
#include KEYMAP

#define SHIFT_DOUBLE_TAP_MS 400
#define SUGG_SLOTS 3

static void kbd_draw_suggestions(struct kbd *kb);
static void kbd_clear_trail(struct kbd *kb);
static void kbd_update_completions(struct kbd *kb);

/* ---------------------------------------------------------------------- */
/* typing through the xkb keymap                                           */

struct cp_entry {
	uint32_t cp;
	xkb_keycode_t kc;
	xkb_mod_mask_t mask;
	bool found;
};
static struct cp_entry cp_cache[256];

/* shift that was switched on automatically at a sentence start only applies
 * to letters, never to Enter/Backspace/arrows */
static bool autocap_latched;
static int cp_cache_n;

static bool
kbd_lookup_cp(struct kbd *kb, uint32_t cp, xkb_keycode_t *kc_out,
              xkb_mod_mask_t *mask_out)
{
	for (int i = 0; i < cp_cache_n; i++) {
		if (cp_cache[i].cp == cp) {
			*kc_out = cp_cache[i].kc;
			*mask_out = cp_cache[i].mask;
			return cp_cache[i].found;
		}
	}

	xkb_keysym_t want = xkb_utf32_to_keysym(cp);
	bool found = false;
	xkb_keycode_t best_kc = 0;
	xkb_mod_mask_t best_mask = 0;
	xkb_level_index_t best_level = 99;
	xkb_keycode_t min = xkb_keymap_min_keycode(kb->xkb_keymap);
	xkb_keycode_t max = xkb_keymap_max_keycode(kb->xkb_keymap);

	for (xkb_keycode_t kc = min; kc <= max && want != XKB_KEY_NoSymbol; kc++) {
		xkb_level_index_t levels =
		    xkb_keymap_num_levels_for_key(kb->xkb_keymap, kc, 0);
		for (xkb_level_index_t lvl = 0; lvl < levels && lvl < best_level; lvl++) {
			const xkb_keysym_t *syms;
			int n = xkb_keymap_key_get_syms_by_level(kb->xkb_keymap, kc, 0,
			                                         lvl, &syms);
			if (n != 1 || syms[0] != want)
				continue;
			xkb_mod_mask_t masks[8];
			size_t nm = xkb_keymap_key_get_mods_for_level(kb->xkb_keymap, kc,
			                                              0, lvl, masks, 8);
			if (nm == 0)
				continue;
			/* prefer the simplest modifier combination */
			xkb_mod_mask_t m = masks[0];
			for (size_t i = 1; i < nm; i++)
				if (__builtin_popcount(masks[i]) < __builtin_popcount(m))
					m = masks[i];
			found = true;
			best_kc = kc;
			best_mask = m;
			best_level = lvl;
		}
	}

	if (cp_cache_n < (int)(sizeof(cp_cache) / sizeof(*cp_cache)))
		cp_cache[cp_cache_n++] = (struct cp_entry){cp, best_kc, best_mask, found};
	*kc_out = best_kc;
	*mask_out = best_mask;
	return found;
}

#define LONGPRESS_MS 350

/* long-press alternates, Gboard style: numbers on the top row first */
static const char *alternates[][2] = {
	{"q", "1"}, {"w", "2"}, {"e", "3éèêë€"}, {"r", "4"}, {"t", "5"},
	{"y", "6ÿý"}, {"u", "7üúùû"}, {"i", "8íìîï"}, {"o", "9óòôõøœ"},
	{"p", "0"}, {"a", "áàâãæ"}, {"s", "šß§"}, {"d", "ð"}, {"g", "ğ"},
	{"l", "ł"}, {"z", "žź"}, {"c", "çč©"}, {"n", "ñń"}, {"ö", "øœ"},
	{"ä", "æ"}, {".", "?!…"}, {",", ":;'\""}, {"-", "–—_·"},
	{"$", "£€¥¢"}, {"€", "£$¥¢"}, {"?", "¿"}, {"!", "¡"}, {"'", "‘’‚"},
	{"\"", "“”„«»"}, {"%", "‰"}, {"0", "°"}, {"/", "\\"}, {"(", "[{<"},
	{")", "]}>"}, {"*", "×†"}, {"+", "±"}, {"=", "≠≈"}, {"&", "§"},
	{"1", "¹½⅓¼"}, {"2", "²⅔"}, {"3", "³¾"},
};

static const char *
key_alternates(struct key *k)
{
	if (k->type != Char)
		return NULL;
	for (size_t i = 0; i < sizeof(alternates) / sizeof(*alternates); i++)
		if (!strcmp(alternates[i][0], k->label))
			return alternates[i][1];
	return NULL;
}

static void kbd_upload_keymap(struct kbd *kb, const char *str);

/* touch $XDG_RUNTIME_DIR/omarchy-osk-typed whenever text is sent, so the
 * auto-show daemon can tell our own typing apart from a focus change */
static void
kbd_mark_typed(void)
{
	static int64_t last;
	static char path[256];
	int64_t now = monotime();
	if (now - last < 100)
		return;
	last = now;
	if (!path[0]) {
		const char *dir = getenv("XDG_RUNTIME_DIR");
		snprintf(path, sizeof(path), "%s/omarchy-osk-typed", dir ? dir : "/tmp");
	}
	int fd = open(path, O_WRONLY | O_CREAT, 0600);
	if (fd >= 0) {
		futimens(fd, NULL);
		close(fd);
	}
}

void zwp_virtual_keyboard_v1_key_mods(struct zwp_virtual_keyboard_v1 *vkbd,
                                      uint32_t time, uint32_t mods,
                                      enum wl_keyboard_key_state state);

/* Press one key with the given modifiers. Ctrl/Alt/Super are only held for
 * the duration of this key, so the compositor never sees a modifier that
 * stays down while the keyboard waits (or hides). */
static void
kbd_tap_code(struct kbd *kb, uint32_t time, uint32_t code, uint32_t mods)
{
	kbd_mark_typed();
	zwp_virtual_keyboard_v1_key_mods(kb->vkbd, time, mods,
	                                 WL_KEYBOARD_KEY_STATE_PRESSED);
	zwp_virtual_keyboard_v1_modifiers(kb->vkbd, mods, 0, 0, 0);
	zwp_virtual_keyboard_v1_key(kb->vkbd, time, code,
	                            WL_KEYBOARD_KEY_STATE_PRESSED);
	zwp_virtual_keyboard_v1_key(kb->vkbd, time, code,
	                            WL_KEYBOARD_KEY_STATE_RELEASED);
	zwp_virtual_keyboard_v1_key_mods(kb->vkbd, time, mods,
	                                 WL_KEYBOARD_KEY_STATE_RELEASED);
	zwp_virtual_keyboard_v1_modifiers(kb->vkbd, 0, 0, 0, 0);
}

static void
kbd_type_cp(struct kbd *kb, uint32_t time, uint32_t cp, uint32_t extra_mods)
{
	xkb_keycode_t kc;
	xkb_mod_mask_t mask;
	if (!kbd_lookup_cp(kb, cp, &kc, &mask)) {
		fprintf(stderr, "omarchy-osk: U+%04X is not in the keymap\n", cp);
		return;
	}
	kbd_tap_code(kb, time, kc - 8, mask | extra_mods);
}

static void
kbd_type_utf8(struct kbd *kb, uint32_t time, const char *s)
{
	uint32_t cps[SWIPE_MAX_CPS * 2];
	int n = utf8_decode(s, cps, SWIPE_MAX_CPS * 2);
	for (int i = 0; i < n; i++)
		kbd_type_cp(kb, time, cps[i], 0);
}

static void
kbd_backspace(struct kbd *kb, uint32_t time, int count)
{
	for (int i = 0; i < count; i++)
		kbd_tap_code(kb, time, KEY_BACKSPACE, 0);
}

static uint32_t
cp_upper(uint32_t cp)
{
	if ((cp >= 'a' && cp <= 'z') || (cp >= 0xE0 && cp <= 0xFE && cp != 0xF7))
		return cp - 32;
	return cp;
}

static void
utf8_capitalize(char *s, bool all)
{
	uint32_t cps[SWIPE_MAX_CPS];
	int n = utf8_decode(s, cps, SWIPE_MAX_CPS);
	char out[SWIPE_MAX_WORD];
	int o = 0;
	for (int i = 0; i < n && o < SWIPE_MAX_WORD - 5; i++)
		o += utf8_encode((all || i == 0) ? cp_upper(cps[i]) : cps[i], out + o);
	out[o] = 0;
	strcpy(s, out);
}

static bool
utf8_starts_upper(const char *s)
{
	uint32_t cp;
	return utf8_decode(s, &cp, 1) == 1 && cp_upper(cp) == cp &&
	       cp_upper(cp + 32) == cp;
}

/* ---------------------------------------------------------------------- */
/* layouts                                                                 */

void
kbd_switch_layout(struct kbd *kb, struct layout *l, size_t layer_index)
{
	kb->prevlayout = kb->layout;
	if ((kb->layer_index != kb->last_abc_index) && (kb->layout->abc)) {
		kb->last_abc_layout = kb->layout;
		kb->last_abc_index = kb->layer_index;
	}
	kb->layer_index = layer_index;
	kb->layout = l;
	if (kb->debug)
		fprintf(stderr, "Switching to layout %s, layer_index %ld\n",
		        kb->layout->name, layer_index);
	kbd_draw_layout(kb);
}

void
kbd_next_layer(struct kbd *kb, struct key *k, bool invert)
{
	size_t layercount = 0;
	enum layout_id *layers = kb->landscape ? kb->landscape_layers : kb->layers;
	while (layers[layercount] != NumLayouts)
		layercount++;
	size_t layer_index = kb->layer_index;
	if (invert)
		layer_index = layer_index ? layer_index - 1 : layercount - 1;
	else
		layer_index = (layer_index + 1) % layercount;
	kbd_switch_layout(kb, &kb->layouts[layers[layer_index]], layer_index);
}

uint8_t
kbd_get_rows(struct layout *l)
{
	uint8_t rows = 0;
	struct key *k = l->keys;
	while (k->type != Last) {
		if (k->type == EndRow) {
			rows++;
		}
		k++;
	}
	return rows + 1;
}

enum layout_id *
kbd_init_layers(char *layer_names_list)
{
	enum layout_id *layers;
	uint8_t numlayers = 0;
	bool found;
	char *s;
	int i;

	layers = malloc(MAX_LAYERS * sizeof(enum layout_id));
	s = strtok(layer_names_list, ",");
	while (s != NULL) {
		if (numlayers + 1 == MAX_LAYERS) {
			fprintf(stderr, "too many layers specified");
			exit(3);
		}
		found = false;
		for (i = 0; i < NumLayouts; i++) {
			if (layouts[i].name && strcmp(layouts[i].name, s) == 0) {
				layers[numlayers++] = i;
				found = true;
				break;
			}
		}
		if (!found) {
			fprintf(stderr, "No such layer: %s\n", s);
			exit(3);
		}
		s = strtok(NULL, ",");
	}
	layers[numlayers] = NumLayouts; // mark the end of the sequence
	if (numlayers == 0) {
		fprintf(stderr, "No layers defined\n");
		exit(3);
	}

	return layers;
}

void
kbd_init(struct kbd *kb, struct layout *layouts, char *layer_names_list,
         char *landscape_layer_names_list)
{
	kb->layouts = layouts;
	kb->layer_index = 0;
	kb->last_abc_index = 0;
	kb->sugg_pressed = -1;

	if (layer_names_list)
		kb->layers = kbd_init_layers(layer_names_list);
	if (landscape_layer_names_list)
		kb->landscape_layers = kbd_init_layers(landscape_layer_names_list);

	enum layout_id layer =
	    kb->landscape ? kb->landscape_layers[0] : kb->layers[0];
	kb->layout = &kb->layouts[layer];
	kb->last_abc_layout = &kb->layouts[layer];

	create_and_upload_keymap(kb, kb->layout->keymap_name, 0);
}

void
kbd_init_layout(struct layout *l, uint32_t width, uint32_t height,
                uint32_t yoff)
{
	uint32_t x = 0, y = yoff;
	uint8_t rows = kbd_get_rows(l);

	l->keyheight = (height - yoff) / rows;

	struct key *k = l->keys;
	double rowlength = kbd_get_row_length(k);
	double rowwidth = 0.0;
	while (k->type != Last) {
		if (k->type == EndRow) {
			y += l->keyheight;
			x = 0;
			rowwidth = 0.0;
			rowlength = kbd_get_row_length(k + 1);
		} else if (k->width > 0) {
			k->x = x;
			k->y = y;
			k->w = ((double)width / rowlength) * k->width;
			x += k->w;
			rowwidth += k->width;
			if (x < (rowwidth / rowlength) * (double)width) {
				k->w++;
				x++;
			}
		}
		k->h = l->keyheight;
		k++;
	}
}

double
kbd_get_row_length(struct key *k)
{
	double l = 0.0;
	while ((k->type != Last) && (k->type != EndRow)) {
		l += k->width;
		k++;
	}
	return l;
}

struct key *
kbd_get_key(struct kbd *kb, uint32_t x, uint32_t y)
{
	struct key *k = kb->layout->keys;
	while (k->type != Last) {
		if ((k->type != EndRow) && (k->type != Pad) && (x >= k->x) &&
		    (y >= k->y) && (x < k->x + k->w) && (y < k->y + k->h)) {
			return k;
		}
		k++;
	}
	return NULL;
}

size_t
kbd_get_layer_index(struct kbd *kb, struct layout *l)
{
	for (size_t i = 0; i < NumLayouts; i++) {
		if (l == &kb->layouts[i]) {
			return i;
		}
	}
	return 0;
}

/* ---------------------------------------------------------------------- */
/* classic key handling (modifiers, special keys)                          */

void
zwp_virtual_keyboard_v1_key_mods(struct zwp_virtual_keyboard_v1 *vkbd,
                                 uint32_t time, uint32_t mods,
                                 enum wl_keyboard_key_state state)
{
	if ((mods & Ctrl) == Ctrl)
		zwp_virtual_keyboard_v1_key(vkbd, time, KEY_LEFTCTRL, state);
	if ((mods & Alt) == Alt)
		zwp_virtual_keyboard_v1_key(vkbd, time, KEY_LEFTALT, state);
	if ((mods & Super) == Super)
		zwp_virtual_keyboard_v1_key(vkbd, time, KEY_LEFTMETA, state);
}

static bool
kbd_shifted(struct kbd *kb)
{
	return (kb->mods & Shift) || (kb->mods & CapsLock);
}

/* drop one-shot modifiers after a key was typed */
static void
kbd_unlatch(struct kbd *kb, uint32_t time)
{
	uint8_t oneshot = kb->mods & (Shift | Ctrl | Alt | Super);
	if (!oneshot)
		return;
	kb->mods &= ~oneshot;
	autocap_latched = false;
	kbd_draw_layout(kb);
}

static void
kbd_autocap(struct kbd *kb)
{
	if (!(kb->mods & CapsLock) && kb->layout->abc) {
		kb->mods |= Shift;
		autocap_latched = true;
		kbd_draw_layout(kb);
	}
}

/* forget latched modifiers (caps lock stays) */
void
kbd_clear_mods(struct kbd *kb)
{
	kb->mods &= CapsLock;
	autocap_latched = false;
	zwp_virtual_keyboard_v1_modifiers(kb->vkbd, 0, 0, 0, 0);
}

void
kbd_unpress_key(struct kbd *kb, uint32_t time)
{
	if (!kb->last_press)
		return;
	struct key *k = kb->last_press;
	kb->last_press = NULL;
	if (k->type == Code) {
		zwp_virtual_keyboard_v1_key(kb->vkbd, time, k->code,
		                            WL_KEYBOARD_KEY_STATE_RELEASED);
		zwp_virtual_keyboard_v1_key_mods(kb->vkbd, time, kb->held_mods,
		                                 WL_KEYBOARD_KEY_STATE_RELEASED);
		zwp_virtual_keyboard_v1_modifiers(kb->vkbd, 0, 0, 0, 0);
		kb->held_mods = 0;
		if (kb->mods & (Shift | Ctrl | Alt | Super)) {
			kbd_unlatch(kb, time);
			return;
		}
	}
	kbd_draw_key(kb, k, Unpress);
}

void
kbd_release_key(struct kbd *kb, uint32_t time)
{
	kbd_unpress_key(kb, time);
	kbd_clear_last_popup(kb);
}

void
kbd_motion_key(struct kbd *kb, uint32_t time, uint32_t x, uint32_t y)
{
	struct key *k = kbd_get_key(kb, x, y);
	if (kb->last_press && kb->last_press != k) {
		kbd_unpress_key(kb, time);
		kbd_clear_last_popup(kb);
	}
}

static void
kbd_set_suggestions(struct kbd *kb, struct swipe_result *r, bool replace)
{
	if (r)
		kb->sugg = *r;
	else
		kb->sugg.n = 0;
	kb->sugg_replace = replace;
	kbd_draw_suggestions(kb);
}

void
kbd_reset_text(struct kbd *kb)
{
	kb->word[0] = 0;
	kb->committed[0] = 0;
	kb->after_commit = false;
	kb->after_space = false;
	if (kb->sugg.n)
		kbd_set_suggestions(kb, NULL, false);
}

void
kbd_press_key(struct kbd *kb, struct key *k, uint32_t time)
{
	switch (k->type) {
	case Code:
		if (k->code == KEY_BACKSPACE) {
			if (kb->after_commit && kb->committed[0]) {
				if (kb->commit_swiped) {
					/* like Gboard: backspace right after a
					 * gesture removes the whole word */
					kbd_backspace(kb, time, utf8_len(kb->committed) + 1);
					kbd_reset_text(kb);
				} else {
					/* after a picked suggestion only the added
					 * space goes, so the word can be continued */
					char w[SWIPE_MAX_WORD];
					snprintf(w, sizeof(w), "%s", kb->committed);
					kbd_backspace(kb, time, 1);
					kbd_reset_text(kb);
					snprintf(kb->word, sizeof(kb->word), "%s", w);
					kbd_update_completions(kb);
				}
				kbd_draw_key(kb, k, Press);
				kb->last_press = NULL;
				kbd_draw_key(kb, k, Unpress);
				return;
			}
			size_t len = strlen(kb->word);
			if (len) {
				while (len && (kb->word[len - 1] & 0xC0) == 0x80)
					len--;
				kb->word[len ? len - 1 : 0] = 0;
				kbd_update_completions(kb);
			} else {
				kbd_reset_text(kb);
			}
			kb->after_commit = kb->after_space = false;
		} else {
			kbd_reset_text(kb);
		}
		if (autocap_latched) {
			kb->mods &= ~Shift;
			autocap_latched = false;
			kbd_draw_layout(kb);
		}
		/* shift only modifies navigation keys, never Enter/Backspace */
		uint8_t code_mods = kb->mods & ~CapsLock;
		if (k->code == KEY_ENTER || k->code == KEY_BACKSPACE)
			code_mods &= ~Shift;
		kb->held_mods = code_mods;
		zwp_virtual_keyboard_v1_key_mods(kb->vkbd, time, code_mods,
		                                 WL_KEYBOARD_KEY_STATE_PRESSED);
		zwp_virtual_keyboard_v1_modifiers(kb->vkbd, code_mods, 0, 0, 0);
		kb->last_press = k;
		kb->last_press_offset = time - monotime();
		kbd_draw_key(kb, k, Press);
		kbd_mark_typed();
		zwp_virtual_keyboard_v1_key(kb->vkbd, time, k->code,
		                            WL_KEYBOARD_KEY_STATE_PRESSED);
		break;
	case Mod:
		if (k->code == Shift) {
			int64_t now = monotime();
			if (kb->mods & CapsLock) {
				kb->mods &= ~(CapsLock | Shift);
			} else if ((kb->mods & Shift) &&
			           now - kb->last_shift_tap < SHIFT_DOUBLE_TAP_MS) {
				kb->mods = (kb->mods & ~Shift) | CapsLock;
			} else {
				kb->mods ^= Shift;
			}
			kb->last_shift_tap = now;
			autocap_latched = false;
			kbd_draw_layout(kb);
			break;
		}
		/* Ctrl/Alt/Super latch for the next key, nothing is sent yet */
		kb->mods ^= k->code;
		kbd_draw_key(kb, k, (kb->mods & k->code) ? Press : Unpress);
		break;
	case Layout:
		kbd_switch_layout(kb, k->layout, kbd_get_layer_index(kb, k->layout));
		break;
	case NextLayer:
		kbd_next_layer(kb, k, false);
		break;
	case BackLayer:
		kbd_switch_layout(kb, kb->last_abc_layout, kb->last_abc_index);
		break;
	case Hide:
		kbd_reset_text(kb);
		if (kb->hide_cb)
			kb->hide_cb();
		break;
	default:
		break;
	}
}

/* ---------------------------------------------------------------------- */
/* text input: taps, gestures and suggestions                              */

static bool
key_is_letter(struct key *k)
{
	return k->type == Char && strcmp(k->label, k->shift_label) != 0;
}

static bool
key_is_space(struct key *k)
{
	return k->type == Code && k->code == KEY_SPACE;
}

static void
kbd_commit_word(struct kbd *kb, uint32_t time, const char *w, bool replace_word)
{
	if (replace_word)
		kbd_backspace(kb, time, utf8_len(kb->word));
	kbd_type_utf8(kb, time, w);
	kbd_type_utf8(kb, time, " ");
	snprintf(kb->committed, sizeof(kb->committed), "%s", w);
	kb->word[0] = 0;
	kb->after_commit = true;
	kb->after_space = false;
}

static const char *
lang_display_name(const char *code)
{
	static const char *names[][2] = {
		{"en", "English"}, {"fi", "Suomi"},     {"sv", "Svenska"},
		{"nb", "Norsk"},   {"da", "Dansk"},     {"de", "Deutsch"},
		{"nl", "Nederlands"}, {"fr", "Français"}, {"es", "Español"},
		{"it", "Italiano"}, {"pt", "Português"}, {"et", "Eesti"},
		{"pl", "Polski"},
	};
	for (size_t i = 0; i < sizeof(names) / sizeof(*names); i++)
		if (!strcmp(names[i][0], code))
			return names[i][1];
	return code;
}

static void
lang_state_path(char *buf, size_t len)
{
	const char *state = getenv("XDG_STATE_HOME");
	if (state && *state)
		snprintf(buf, len, "%s/omarchy-osk/language", state);
	else
		snprintf(buf, len, "%s/.local/state/omarchy-osk/language",
		         getenv("HOME") ? getenv("HOME") : "");
}

/* pick the remembered language, or the first dictionary */
void
kbd_load_lang(struct kbd *kb)
{
	int n = swipe_lang_count();
	if (n < 2)
		return;
	swipe_set_lang(0);
	char path[512], code[16] = {0};
	lang_state_path(path, sizeof(path));
	FILE *f = fopen(path, "r");
	if (!f)
		return;
	if (fscanf(f, "%15s", code) == 1)
		for (int i = 0; i < n; i++)
			if (!strcmp(swipe_lang_code(i), code))
				swipe_set_lang(i);
	fclose(f);
}

static void
kbd_switch_lang(struct kbd *kb, int dir)
{
	int n = swipe_lang_count();
	if (n < 2)
		return;
	int cur = swipe_get_lang() < 0 ? 0 : swipe_get_lang();
	swipe_set_lang((cur + dir + n) % n);
	kbd_reset_text(kb);
	kbd_draw_layout(kb);

	char path[512];
	lang_state_path(path, sizeof(path));
	char *slash = strrchr(path, '/');
	*slash = 0;
	char cmd[600];
	snprintf(cmd, sizeof(cmd), "%s", path);
	mkdir(cmd, 0755);
	*slash = '/';
	FILE *f = fopen(path, "w");
	if (f) {
		fprintf(f, "%s\n", swipe_lang_code(swipe_get_lang()));
		fclose(f);
	}
}

static void
kbd_tap_space(struct kbd *kb, uint32_t time)
{
	if (kb->after_commit) {
		/* the gesture already added a space */
		kb->after_commit = false;
		kb->after_space = true;
		kbd_set_suggestions(kb, NULL, false);
		return;
	}
	if (kb->after_space) {
		/* double space: end the sentence */
		kbd_backspace(kb, time, 1);
		kbd_type_utf8(kb, time, ". ");
		kb->after_space = false;
		kb->word[0] = 0;
		kbd_autocap(kb);
		return;
	}
	kb->after_space = kb->word[0] != 0;
	kb->word[0] = 0;
	kbd_type_cp(kb, time, ' ', kb->mods & (Ctrl | Alt | Super));
	kbd_set_suggestions(kb, NULL, false);
	if (kb->mods & (Ctrl | Alt | Super))
		kbd_unlatch(kb, time);
}

/* completions for the letters typed so far, keeping their capitalisation */
static void
kbd_update_completions(struct kbd *kb)
{
	if (!kb->word[0]) {
		kbd_set_suggestions(kb, NULL, false);
		return;
	}
	char lower[SWIPE_MAX_WORD];
	uint32_t cps[SWIPE_MAX_CPS];
	int n = utf8_decode(kb->word, cps, SWIPE_MAX_CPS), o = 0;
	for (int i = 0; i < n; i++) {
		uint32_t c = cps[i];
		if ((c >= 'A' && c <= 'Z') || (c >= 0xC0 && c <= 0xDE && c != 0xD7))
			c += 32;
		o += utf8_encode(c, lower + o);
	}
	lower[o] = 0;
	struct swipe_result r;
	swipe_complete(lower, &r);
	bool upper = utf8_starts_upper(kb->word);
	for (int i = 0; i < r.n; i++)
		if (upper)
			utf8_capitalize(r.words[i], false);
	kbd_set_suggestions(kb, &r, false);
}

static void
kbd_tap_cp(struct kbd *kb, uint32_t time, uint32_t cp, bool letter)
{
	bool sentence_end = cp == '.' || cp == '?' || cp == '!';
	if ((kb->after_commit || kb->after_space) && !letter &&
	    strchr(".,?!:;", (int)cp)) {
		/* punctuation hugs the previous word */
		kbd_backspace(kb, time, 1);
		kbd_type_cp(kb, time, cp, 0);
		kbd_type_utf8(kb, time, " ");
		kbd_reset_text(kb);
		if (sentence_end)
			kbd_autocap(kb);
		return;
	}

	kbd_type_cp(kb, time, cp, kb->mods & (Ctrl | Alt | Super));
	bool had_ctrl = kb->mods & (Ctrl | Alt | Super);
	kb->after_commit = kb->after_space = false;

	if (letter && !had_ctrl) {
		size_t len = strlen(kb->word);
		if (len + 5 < sizeof(kb->word)) {
			char buf[5];
			int n = utf8_encode(cp, buf);
			memcpy(kb->word + len, buf, n);
			kb->word[len + n] = 0;
		}
		kbd_update_completions(kb);
	} else {
		kbd_reset_text(kb);
	}

	if (kb->mods & (Shift | Ctrl | Alt | Super))
		kbd_unlatch(kb, time);
	if (sentence_end)
		kbd_autocap(kb);
}

static void
kbd_tap_char(struct kbd *kb, struct key *k, uint32_t time)
{
	bool letter = key_is_letter(k);
	const char *text = (letter && kbd_shifted(kb)) ? k->shift_label : k->label;
	uint32_t cp;
	if (utf8_decode(text, &cp, 1) == 1)
		kbd_tap_cp(kb, time, cp, letter);
}

static void
kbd_swipe_geom(struct kbd *kb, struct swipe_geom *g)
{
	g->nkeys = 0;
	g->kw = g->kh = 0;
	for (struct key *k = kb->layout->keys; k->type != Last; k++) {
		if (!key_is_letter(k) || g->nkeys >= SWIPE_MAX_KEYS)
			continue;
		uint32_t cp;
		if (utf8_decode(k->label, &cp, 1) != 1)
			continue;
		g->keys[g->nkeys++] =
		    (struct swipe_key){cp, k->x + k->w / 2.0, k->y + k->h / 2.0};
		if (!g->kw) {
			g->kw = k->w;
			g->kh = k->h;
		}
	}
}

static void
kbd_finish_swipe(struct kbd *kb, uint32_t time)
{
	struct swipe_geom g;
	struct swipe_result r;
	kbd_swipe_geom(kb, &g);
	swipe_decode(&g, kb->pts_x, kb->pts_y, kb->npts, &r);
	if (kb->debug)
		for (int i = 0; i < r.n; i++)
			fprintf(stderr, "swipe candidate %d: %s\n", i, r.words[i]);
	if (r.n == 0)
		return;

	bool caps = kb->mods & CapsLock, shift = kb->mods & Shift;
	for (int i = 0; i < r.n; i++) {
		if (caps)
			utf8_capitalize(r.words[i], true);
		else if (shift || !strcmp(r.words[i], "i") ||
		         !strncmp(r.words[i], "i'", 2))
			utf8_capitalize(r.words[i], false);
	}

	/* a gesture right after tapped letters starts a new word */
	if (kb->word[0])
		kbd_type_utf8(kb, time, " ");
	kbd_commit_word(kb, time, r.words[0], false);
	kb->commit_swiped = true;
	kbd_set_suggestions(kb, &r, true);
	if (shift)
		kbd_unlatch(kb, time);
}

/* suggestion slots are drawn left, middle, right; the middle one is the
 * primary result (index 0) */
static const int slot_to_index[SUGG_SLOTS] = {1, 0, 2};

static void
kbd_apply_suggestion(struct kbd *kb, uint32_t time, int slot)
{
	int idx = slot_to_index[slot];
	if (idx >= kb->sugg.n)
		return;
	char chosen[SWIPE_MAX_WORD];
	snprintf(chosen, sizeof(chosen), "%s", kb->sugg.words[idx]);

	if (kb->sugg_replace) {
		if (!kb->after_commit || !strcmp(chosen, kb->committed))
			return;
		kbd_backspace(kb, time, utf8_len(kb->committed) + 1);
		kbd_commit_word(kb, time, chosen, false);
		/* keep the alternatives, with the chosen one as primary */
		memcpy(kb->sugg.words[idx], kb->sugg.words[0], SWIPE_MAX_WORD);
		memcpy(kb->sugg.words[0], chosen, SWIPE_MAX_WORD);
		kbd_draw_suggestions(kb);
	} else {
		kbd_commit_word(kb, time, chosen, true);
		kb->commit_swiped = false;
		struct swipe_result r = {.n = 1};
		memcpy(r.words[0], chosen, SWIPE_MAX_WORD);
		kbd_set_suggestions(kb, &r, true);
	}
	if (kb->mods & Shift)
		kbd_unlatch(kb, time);
}

static void
kbd_add_point(struct kbd *kb, double x, double y)
{
	if (kb->npts == kb->pts_cap) {
		kb->pts_cap = kb->pts_cap ? kb->pts_cap * 2 : 256;
		kb->pts_x = realloc(kb->pts_x, kb->pts_cap * sizeof(double));
		kb->pts_y = realloc(kb->pts_y, kb->pts_cap * sizeof(double));
	}
	kb->pts_x[kb->npts] = x;
	kb->pts_y[kb->npts] = y;
	kb->npts++;
}

static double
kbd_trail_width(struct kbd *kb)
{
	return kb->down_key ? kb->down_key->w * 0.14 : 8;
}

static void
kbd_draw_trail_segment(struct kbd *kb, int i)
{
	if (i < 1)
		return;
	drw_line(kb->popup_surf, kb->trail, kb->pts_x[i - 1],
	         kb->pts_y[i - 1] + kb->h, kb->pts_x[i], kb->pts_y[i] + kb->h,
	         kbd_trail_width(kb));
}

static void
kbd_clear_trail(struct kbd *kb)
{
	drw_do_clear(kb->popup_surf, 0, kb->h, kb->w, kb->h);
}

static void
kbd_draw_alternates(struct kbd *kb)
{
	struct clr_scheme *s0 = &kb->schemes[0], *s2 = &kb->schemes[2];
	uint32_t pad = KBD_KEY_BORDER;
	drw_fill_rectangle(kb->popup_surf, s0->bg, kb->alt_x - pad, kb->alt_y - pad,
	                   kb->alt_w * kb->alt_n + pad * 2, kb->alt_h + pad * 2,
	                   s0->rounding);
	for (int i = 0; i < kb->alt_n; i++) {
		char label[8] = {0};
		utf8_encode(kb->alt_cps[i], label);
		bool sel = i == kb->alt_sel;
		uint32_t x = kb->alt_x + i * kb->alt_w;
		draw_inset(kb->popup_surf, x, kb->alt_y, kb->alt_w, kb->alt_h,
		           KBD_KEY_BORDER, sel ? s2->fg : s0->fg, s0->rounding);
		drw_draw_text(kb->popup_surf, sel ? s2->text : s0->text, x, kb->alt_y,
		              kb->alt_w, kb->alt_h, KBD_KEY_BORDER, label,
		              s0->font_description);
	}
}

static void
kbd_close_alternates(struct kbd *kb)
{
	uint32_t pad = KBD_KEY_BORDER;
	drw_do_clear(kb->popup_surf, kb->alt_x - pad, kb->alt_y - pad,
	             kb->alt_w * kb->alt_n + pad * 2, kb->alt_h + pad * 2);
	kb->alt_open = false;
}

void
kbd_longpress(struct kbd *kb)
{
	kb->longpress_at = 0;
	struct key *k = kb->down_key;
	if (k && key_is_space(k) && kb->space_mode == SpaceTap) {
		kb->space_mode = SpaceCursor;
		kb->pts_x[0] = kb->pts_x[kb->npts - 1];
		kbd_clear_last_popup(kb);
		return;
	}
	const char *alts = k ? key_alternates(k) : NULL;
	if (kb->debug)
		fprintf(stderr, "%ld longpress key=%s swiping=%d\n", (long)monotime(), k ? k->label : "-", kb->swiping);
	if (!alts || kb->swiping)
		return;

	bool upper = key_is_letter(k) && kbd_shifted(kb);
	kb->alt_n = utf8_decode(alts, kb->alt_cps, 16);
	for (int i = 0; i < kb->alt_n && upper; i++)
		kb->alt_cps[i] = cp_upper(kb->alt_cps[i]);
	kb->alt_sel = 0;
	kb->alt_w = k->w;
	kb->alt_h = k->h;
	int x = k->x;
	if (x + kb->alt_w * kb->alt_n > kb->w)
		x = kb->w - kb->alt_w * kb->alt_n;
	kb->alt_x = x < (int)KBD_KEY_BORDER ? KBD_KEY_BORDER : x;
	kb->alt_y = kb->h + k->y - k->h;
	kb->alt_open = true;

	kbd_clear_last_popup(kb);
	kbd_draw_alternates(kb);
}

void
kbd_touch_down(struct kbd *kb, uint32_t time, int32_t x, int32_t y)
{
	if (kb->debug)
		fprintf(stderr, "%ld down %u,%u\n", (long)monotime(), x, y);
	kbd_unpress_key(kb, time);
	if (kb->alt_open)
		kbd_close_alternates(kb);
	if (x < 0 || y < 0)
		return;

	if ((uint32_t)y < kb->suggest_h) {
		kb->sugg_pressed = x * SUGG_SLOTS / kb->w;
		kbd_draw_suggestions(kb);
		return;
	}

	struct key *k = kbd_get_key(kb, x, y);
	if (!k)
		return;

	if (k->type == Char || key_is_space(k)) {
		/* typed on release, so the touch can still become a gesture */
		kb->down_key = k;
		kb->swiping = false;
		kb->npts = 0;
		kbd_add_point(kb, x, y);
		kbd_draw_key(kb, k, Press);
		kb->space_mode = SpaceTap;
		kb->lang_dir = 1;
		if (key_alternates(k) || key_is_space(k))
			kb->longpress_at = monotime() + LONGPRESS_MS;
		return;
	}
	kbd_press_key(kb, k, time);
}

void
kbd_touch_motion(struct kbd *kb, uint32_t time, int32_t x, int32_t y)
{
	if (kb->sugg_pressed >= 0)
		return;
	if (!kb->down_key) {
		if (x >= 0 && y >= 0)
			kbd_motion_key(kb, time, x, y);
		else
			kbd_motion_key(kb, time, UINT32_MAX, UINT32_MAX);
		return;
	}

	struct key *dk = kb->down_key;
	if (kb->alt_open) {
		int sel = ((int)x - (int)kb->alt_x) / (int)kb->alt_w;
		sel = sel < 0 ? 0 : sel >= kb->alt_n ? kb->alt_n - 1 : sel;
		if (sel != kb->alt_sel) {
			kb->alt_sel = sel;
			kbd_draw_alternates(kb);
		}
		return;
	}
	double dx = (double)x - kb->pts_x[kb->npts - 1];
	double dy = (double)y - kb->pts_y[kb->npts - 1];
	if (dx * dx + dy * dy < 4)
		return;
	kbd_add_point(kb, x, y);

	if (key_is_space(dk)) {
		double moved = (double)x - kb->pts_x[0];
		if (kb->space_mode == SpaceTap) {
			/* a quick swipe across the space bar switches language */
			if (fabs(moved) > dk->w / 6.0) {
				kb->space_mode = SpaceLang;
				kb->longpress_at = 0;
				kb->lang_dir = moved < 0 ? -1 : 1;
			}
			return;
		}
		if (kb->space_mode == SpaceLang) {
			kb->lang_dir = moved < 0 ? -1 : 1;
			return;
		}
		/* after a long press, sliding moves the cursor */
		double step = dk->w / 14.0;
		if (fabs(moved) >= step) {
			int n = (int)(moved / step);
			for (int i = 0; i < abs(n); i++)
				kbd_tap_code(kb, time, n > 0 ? KEY_RIGHT : KEY_LEFT,
				             kb->mods & ~(CapsLock | (autocap_latched ? Shift : 0)));
			kb->pts_x[0] += n * step;
			kbd_reset_text(kb);
		}
		return;
	}

	if (!kb->swiping) {
		struct key *over = (x < 0 || y < 0) ? NULL : kbd_get_key(kb, x, y);
		double fx = (double)x - kb->pts_x[0], fy = (double)y - kb->pts_y[0];
		if (kb->swipe_enabled && kb->layout->abc && key_is_letter(dk) &&
		    over != dk && fx * fx + fy * fy > dk->w * dk->w * 0.36) {
			kb->swiping = true;
			kb->longpress_at = 0;
			kbd_clear_last_popup(kb);
			kbd_draw_key(kb, dk, Unpress);
			for (int i = 1; i < kb->npts; i++)
				kbd_draw_trail_segment(kb, i);
		}
		return;
	}
	kbd_draw_trail_segment(kb, kb->npts - 1);
}

void
kbd_touch_up(struct kbd *kb, uint32_t time)
{
	if (kb->debug)
		fprintf(stderr, "%ld up alt_open=%d swiping=%d\n", (long)monotime(), kb->alt_open, kb->swiping);
	if (kb->sugg_pressed >= 0) {
		int slot = kb->sugg_pressed;
		kb->sugg_pressed = -1;
		kbd_apply_suggestion(kb, time, slot);
		kbd_draw_suggestions(kb);
		return;
	}

	struct key *dk = kb->down_key;
	if (!dk) {
		kbd_release_key(kb, time);
		return;
	}
	kb->down_key = NULL;
	kb->longpress_at = 0;
	kbd_clear_last_popup(kb);

	if (kb->alt_open) {
		uint32_t cp = kb->alt_cps[kb->alt_sel];
		kbd_close_alternates(kb);
		kbd_draw_key(kb, dk, Unpress);
		/* accented letters continue the current word */
		bool letter = (cp >= 'a' && cp <= 'z') || (cp >= 'A' && cp <= 'Z') ||
		              (cp >= 0xC0 && cp <= 0x17F && cp != 0xD7 && cp != 0xF7);
		kbd_tap_cp(kb, time, cp, letter);
		kb->swiping = false;
		return;
	}

	if (key_is_space(dk)) {
		if (kb->space_mode == SpaceLang)
			kbd_switch_lang(kb, kb->lang_dir);
		else if (kb->space_mode == SpaceTap)
			kbd_tap_space(kb, time);
		kb->space_mode = SpaceTap;
		kbd_draw_key(kb, dk, Unpress);
	} else if (kb->swiping) {
		kbd_clear_trail(kb);
		kbd_finish_swipe(kb, time);
	} else {
		kbd_draw_key(kb, dk, Unpress);
		kbd_tap_char(kb, dk, time);
	}
	kb->swiping = false;
}

/* the compositor took the touch away (e.g. a gesture): drop everything
 * without typing */
void
kbd_touch_cancel(struct kbd *kb, uint32_t time)
{
	kbd_unpress_key(kb, time);
	if (kb->alt_open)
		kbd_close_alternates(kb);
	if (kb->swiping && kb->down_key && !key_is_space(kb->down_key))
		kbd_clear_trail(kb);
	if (kb->down_key)
		kbd_draw_key(kb, kb->down_key, Unpress);
	kb->down_key = NULL;
	kb->swiping = false;
	kb->longpress_at = 0;
	kb->space_mode = SpaceTap;
	kb->sugg_pressed = -1;
	kbd_clear_last_popup(kb);
	kbd_draw_suggestions(kb);
}

/* ---------------------------------------------------------------------- */
/* drawing                                                                 */

void
kbd_clear_last_popup(struct kbd *kb)
{
	if (kb->last_popup_w && kb->last_popup_h) {
		drw_do_clear(kb->popup_surf, kb->last_popup_x, kb->last_popup_y,
		             kb->last_popup_w, kb->last_popup_h);
		kb->last_popup_w = kb->last_popup_h = 0;
	}
}

static const char *
kbd_key_label(struct kbd *kb, struct key *k)
{
	if (k->type == Mod && k->code == Shift)
		return (kb->mods & CapsLock) ? k->shift_label : k->label;
	if (k->type == Char)
		return (key_is_letter(k) && kbd_shifted(kb)) ? k->shift_label : k->label;
	if (key_is_space(k) && swipe_lang_count() > 1) {
		static char buf[64];
		snprintf(buf, sizeof(buf), "‹  %s  ›",
		         lang_display_name(swipe_lang_code(swipe_get_lang())));
		return buf;
	}
	return k->label;
}

void
kbd_draw_key(struct kbd *kb, struct key *k, enum key_draw_type type)
{
	const char *label = kbd_key_label(kb, k);
	struct clr_scheme *scheme = &kb->schemes[k->scheme];
	Color fill = scheme->fg, text = scheme->text;

	if (type == Press && kb->show_highlight) {
		fill = scheme->high;
		text = scheme->text_press;
	}

	drw_fill_rectangle(kb->surf, scheme->bg, k->x, k->y, k->w, k->h, 0);
	draw_inset(kb->surf, k->x, k->y, k->w, k->h, KBD_KEY_BORDER, fill,
	           scheme->rounding);
	bool space_label = key_is_space(k) && kb->hint_font;
	if (space_label)
		text.bgra[3] = 170;
	drw_draw_text(kb->surf, text, k->x, k->y, k->w, k->h, KBD_KEY_BORDER,
	              label, space_label ? kb->hint_font : scheme->font_description);

	const char *alts = key_alternates(k);
	if (alts && kb->hint_font && key_is_letter(k) && alts[0] >= '0' &&
	    alts[0] <= '9') {
		char hint[2] = {alts[0], 0};
		Color dim = text;
		dim.bgra[3] = 140;
		drw_draw_text(kb->surf, dim, k->x + k->w * 0.62, k->y + KBD_KEY_BORDER,
		              k->w * 0.38 - KBD_KEY_BORDER, k->h * 0.4, 0, hint,
		              kb->hint_font);
	}

	/* magnified preview above the finger for character keys */
	if (kb->show_popup && k->type == Char && (type == Press || type == Unpress)) {
		kbd_clear_last_popup(kb);
		if (type == Unpress)
			return;
		kb->last_popup_x = k->x;
		kb->last_popup_y = kb->h + k->y - k->h;
		kb->last_popup_w = k->w;
		kb->last_popup_h = k->h;
		draw_inset(kb->popup_surf, k->x, kb->last_popup_y, k->w, k->h,
		           KBD_KEY_BORDER, scheme->high, scheme->rounding);
		drw_draw_text(kb->popup_surf, scheme->text_press, k->x,
		              kb->last_popup_y, k->w, k->h, KBD_KEY_BORDER, label,
		              scheme->font_description);
	}
}

static void
kbd_draw_suggestions(struct kbd *kb)
{
	if (!kb->suggest_h || !kb->surf->back_buffer || !kb->w)
		return;
	struct clr_scheme *s0 = &kb->schemes[0], *s1 = &kb->schemes[1];
	uint32_t h = kb->suggest_h, slot_w = kb->w / SUGG_SLOTS;
	drw_fill_rectangle(kb->surf, s0->bg, 0, 0, kb->w, h, 0);

	for (int slot = 0; slot < SUGG_SLOTS; slot++) {
		int idx = slot_to_index[slot];
		uint32_t x = slot * slot_w;
		if (idx >= kb->sugg.n)
			continue;
		if (kb->sugg_pressed == slot)
			draw_inset(kb->surf, x, 0, slot_w, h, KBD_KEY_BORDER, s0->fg,
			           s0->rounding);
		bool primary = idx == 0 && !(kb->sugg_replace && !kb->after_commit);
		drw_draw_text(kb->surf, primary ? kb->schemes[2].fg : s0->text, x, 0,
		              slot_w, h, KBD_KEY_BORDER, kb->sugg.words[idx],
		              s0->font_description);
		if (slot > 0)
			drw_fill_rectangle(kb->surf, s1->fg, x, h / 4, 2, h / 2, 0);
	}
}

void
kbd_draw_layout(struct kbd *kb)
{
	struct drwsurf *d = kb->surf;
	struct key *next_key = kb->layout->keys;

	if (kb->corner_radius > 0) {
		drw_do_clear(d, 0, 0, kb->w, kb->h);
		drw_fill_rectangle(d, kb->schemes[0].bg, 0, 0, kb->w, kb->h,
		                   kb->corner_radius);
	} else {
		drw_fill_rectangle(d, kb->schemes[0].bg, 0, 0, kb->w, kb->h, 0);
	}
	kbd_draw_suggestions(kb);

	while (next_key->type != Last) {
		if ((next_key->type == Pad) || (next_key->type == EndRow)) {
			next_key++;
			continue;
		}
		bool active = next_key->type == Mod &&
		              ((kb->mods & next_key->code) ||
		               (next_key->code == Shift && (kb->mods & CapsLock)));
		kbd_draw_key(kb, next_key, active ? Press : None);
		next_key++;
	}
}

void
kbd_resize(struct kbd *kb, struct layout *layouts, uint8_t layoutcount)
{
	fprintf(stderr, "Resize %dx%d %f, %d layouts\n", kb->w, kb->h, kb->scale,
	        layoutcount);

	drwsurf_resize(kb->surf, kb->w, kb->h, kb->scale);
	drwsurf_resize(kb->popup_surf, kb->w, kb->h * 2, kb->scale);
	if (!kb->hint_font) {
		kb->hint_font = pango_font_description_copy(kb->schemes[0].font_description);
		pango_font_description_set_size(
		    kb->hint_font,
		    pango_font_description_get_size(kb->hint_font) * 0.5);
	}
	uint8_t rows = kbd_get_rows(&layouts[0]);
	kb->suggest_h = kb->swipe_enabled ? (uint32_t)(kb->h / (rows + 0.7) * 0.7) : 0;
	for (int i = 0; i < layoutcount; i++)
		kbd_init_layout(&layouts[i], kb->w, kb->h, kb->suggest_h);
	kbd_draw_layout(kb);
}

void
draw_inset(struct drwsurf *ds, uint32_t x, uint32_t y, uint32_t width,
           uint32_t height, uint32_t border, Color color, int rounding)
{
	drw_fill_rectangle(ds, color, x + border, y + border, width - (border * 2),
	                   height - (border * 2), rounding);
}
void
draw_over_inset(struct drwsurf *ds, uint32_t x, uint32_t y, uint32_t width,
                uint32_t height, uint32_t border, Color color, int rounding)
{
	drw_over_rectangle(ds, color, x + border, y + border, width - (border * 2),
	                   height - (border * 2), rounding);
}

static void
kbd_collect_cps(struct kbd *kb, const char *s, uint32_t *cps, int *n, int max)
{
	uint32_t buf[32];
	int m = utf8_decode(s, buf, 32);
	for (int i = 0; i < m; i++) {
		bool dup = false;
		for (int j = 0; j < *n; j++)
			dup |= cps[j] == buf[i];
		if (!dup && *n < max)
			cps[(*n)++] = buf[i];
	}
}

/* keycodes the on-screen keyboard never sends: unused ones and the keypad
 * (this keymap only belongs to the virtual keyboard, not the Type Cover) */
static bool
kbd_spare_keycode(struct kbd *kb, xkb_keycode_t kc)
{
	const char *name = xkb_keymap_key_get_name(kb->xkb_keymap, kc);
	if (!name)
		return false;
	if (xkb_keymap_num_layouts_for_key(kb->xkb_keymap, kc) == 0)
		return true;
	return !strncmp(name, "KP", 2) && strcmp(name, "KPEN") != 0;
}

/* Characters on the keyboard that the xkb layout cannot produce (é, ñ, —,
 * …) are bound to keycodes that carry no symbols, so every character can be
 * typed with a plain key press and the keymap never has to be swapped. */
static void
kbd_extend_keymap(struct kbd *kb)
{
	uint32_t cps[256];
	int n = 0;
	for (int l = 0; l < NumLayouts; l++)
		for (struct key *k = layouts[l].keys; k->type != Last; k++)
			if (k->type == Char) {
				kbd_collect_cps(kb, k->label, cps, &n, 256);
				kbd_collect_cps(kb, k->shift_label, cps, &n, 256);
			}
	for (size_t i = 0; i < sizeof(alternates) / sizeof(*alternates); i++) {
		kbd_collect_cps(kb, alternates[i][1], cps, &n, 256);
		/* uppercase variants of accented letters */
		uint32_t buf[32];
		int m = utf8_decode(alternates[i][1], buf, 32);
		for (int j = 0; j < m; j++) {
			char up[8] = {0};
			utf8_encode(cp_upper(buf[j]), up);
			kbd_collect_cps(kb, up, cps, &n, 256);
		}
	}

	size_t extra_cap = 8192, extra_len = 0;
	char *extra = calloc(1, extra_cap);
	xkb_keycode_t kc = 9, max = xkb_keymap_max_keycode(kb->xkb_keymap);
	int added = 0;
	uint32_t missing[256];
	int nmissing = 0;
	for (int i = 0; i < n; i++) {
		xkb_keycode_t dummy_kc;
		xkb_mod_mask_t dummy_mask;
		if (!kbd_lookup_cp(kb, cps[i], &dummy_kc, &dummy_mask))
			missing[nmissing++] = cps[i];
	}
	if (kb->debug)
		fprintf(stderr, "omarchy-osk: %d characters missing from the keymap\n", nmissing);
	/* four characters per free key: plain, Shift, AltGr, Shift+AltGr */
	for (int i = 0; i < nmissing; i += 4) {
		while (kc <= max && kc < 256 && !kbd_spare_keycode(kb, kc))
			kc++;
		if (kc > max || kc >= 256) {
			fprintf(stderr, "omarchy-osk: no free keycodes left\n");
			break;
		}
		char syms[4][64];
		for (int j = 0; j < 4; j++) {
			if (i + j < nmissing)
				xkb_keysym_get_name(xkb_utf32_to_keysym(missing[i + j]),
				                    syms[j], sizeof(syms[j]));
			else
				snprintf(syms[j], sizeof(syms[j]), "NoSymbol");
		}
		const char *fmt = "\toverride key <%s> { type= \"FOUR_LEVEL\", "
		                  "[ %s, %s, %s, %s ] };\n";
		const char *kname = xkb_keymap_key_get_name(kb->xkb_keymap, kc);
		int need = snprintf(NULL, 0, fmt, kname, syms[0], syms[1], syms[2], syms[3]);
		if (extra_len + need + 1 > extra_cap) {
			extra_cap = (extra_len + need + 1) * 2;
			extra = realloc(extra, extra_cap);
		}
		extra_len += snprintf(extra + extra_len, extra_cap - extra_len, fmt,
		                      kname, syms[0], syms[1], syms[2], syms[3]);
		kc++;
		added += i + 4 <= nmissing ? 4 : nmissing - i;
	}
	cp_cache_n = 0;
	if (!added) {
		free(extra);
		return;
	}

	/* the symbols section is the last one in the keymap */
	char *end = strstr(kb->keymap_str, "\n};\n\n};");
	if (!end) {
		free(extra);
		return;
	}
	size_t head = end - kb->keymap_str + 1;
	size_t len = strlen(kb->keymap_str) + extra_len + 1;
	char *str = malloc(len);
	snprintf(str, len, "%.*s%s%s", (int)head, kb->keymap_str, extra,
	         kb->keymap_str + head);
	free(extra);

	struct xkb_keymap *km = xkb_keymap_new_from_string(
	    kb->xkb_ctx, str, XKB_KEYMAP_FORMAT_TEXT_V1, XKB_KEYMAP_COMPILE_NO_FLAGS);
	if (!km) {
		fprintf(stderr, "omarchy-osk: extended keymap failed to compile\n");
		free(str);
		return;
	}
	xkb_keymap_unref(kb->xkb_keymap);
	kb->xkb_keymap = km;
	free(kb->keymap_str);
	kb->keymap_str = xkb_keymap_get_as_string(km, XKB_KEYMAP_FORMAT_TEXT_V1);
	free(str);
	fprintf(stderr, "omarchy-osk: added %d extra characters to the keymap\n", added);
}

/* compile the keymap from xkb names (e.g. "fi") and hand it to the
 * compositor; Char keys look their keycodes up in it */
void
create_and_upload_keymap(struct kbd *kb, const char *name, uint32_t comp_unichr)
{
	if (kb->xkb_keymap)
		return;
	kb->xkb_ctx = xkb_context_new(XKB_CONTEXT_NO_FLAGS);
	struct xkb_rule_names names = {
	    .layout = kb->xkb_layout ? kb->xkb_layout : "fi",
	    .variant = kb->xkb_variant,
	};
	kb->xkb_keymap = xkb_keymap_new_from_names(kb->xkb_ctx, &names,
	                                           XKB_KEYMAP_COMPILE_NO_FLAGS);
	if (!kb->xkb_keymap) {
		die("could not compile xkb keymap for layout %s\n", names.layout);
	}
	kb->keymap_str =
	    xkb_keymap_get_as_string(kb->xkb_keymap, XKB_KEYMAP_FORMAT_TEXT_V1);
	kbd_extend_keymap(kb);
	kbd_upload_keymap(kb, kb->keymap_str);
	fprintf(stderr, "omarchy-osk: using xkb layout %s\n", names.layout);
}

static void
kbd_upload_keymap(struct kbd *kb, const char *keymap_str)
{
	size_t keymap_size = strlen(keymap_str) + 1;
	int keymap_fd = os_create_anonymous_file(keymap_size);
	if (keymap_fd < 0) {
		die("could not create keymap fd\n");
	}
	void *ptr = mmap(NULL, keymap_size, PROT_READ | PROT_WRITE, MAP_SHARED,
	                 keymap_fd, 0);
	if (ptr == (void *)-1) {
		die("could not map keymap data\n");
	}
	memcpy(ptr, keymap_str, keymap_size);
	munmap(ptr, keymap_size);
	zwp_virtual_keyboard_v1_keymap(kb->vkbd, WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1,
	                               keymap_fd, keymap_size);
	close(keymap_fd);
}

// Get a monotonic millisecond timer.
int64_t
monotime()
{
	struct timespec spec;
	clock_gettime(CLOCK_MONOTONIC, &spec);

	return (spec.tv_sec * 1000 + spec.tv_nsec / 1000000);
}
