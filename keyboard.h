#ifndef __KEYBOARD_H
#define __KEYBOARD_H

#include "drw.h"
#include "swipe.h"
#include <xkbcommon/xkbcommon.h>

#define MAX_LAYERS 25

enum key_type;
enum key_modifier_type;
struct clr_scheme;
struct key;
struct layout;
struct kbd;

enum key_type {
	Pad = 0, // Padding, not a pressable key
	Code,    // A normal key emitting a keycode
	Mod,     // A modifier key
	Copy,    // Copy key, copies the unicode value specified in code (creates and
	         // activates temporary keymap)
	         // used for keys that are not part of the keymap
	Layout,  // Layout switch to a specific layout
	BackLayer, // Layout switch to the layout that was previously active
	NextLayer, // Layout switch to the next layout in the layers sequence
	Compose,   // Compose modifier key, switches to a specific associated layout
	           // upon next keypress
	Char,      // Types its label through the xkb keymap (letters swipe)
	Hide,      // Hides the keyboard
	EndRow,    // Incidates the end of a key row
	Last,      // Indicated the end of a layout
};

/* Modifiers passed to the virtual_keyboard protocol. They are based on
 * wayland's wl_keyboard, which doesn't document them.
 */
enum key_modifier_type {
	NoMod = 0,
	Shift = 1,
	CapsLock = 2,
	Ctrl = 4,
	Alt = 8,
	Super = 64,
	AltGr = 128,
};

enum key_draw_type {
	None = 0,
	Unpress,
	Press,
	Swipe,
};

struct clr_scheme {
	Color fg;
	Color bg;
	Color high;
	Color swipe;
	Color text;
	Color text_press;
	Color text_swipe;
	char *font;
	int rounding;
	PangoFontDescription *font_description;
};

struct key {
	const char *label;       // primary label
	const char *shift_label; // secondary label
	const double width;      // relative width (1.0)
	const enum key_type type;

	const uint32_t
	  code;                  /* code: key scancode or modifier name (see
	                          *   `/usr/include/linux/input-event-codes.h` for scancode names, and
	                          *   `keyboard.h` for modifiers)
	                          *   XKB keycodes are +8 */
	struct layout *layout;   // pointer back to the parent layout that holds this
	                         // key
	const uint32_t code_mod; /* modifier to force when this key is pressed */
	const uint32_t
	  shift_code;            /* shift_code: secondary scancode */
	uint8_t scheme;          // index of the scheme to use
	bool reset_mod;          /* reset modifiers when clicked */

	// actual coordinates on the surface (pixels), will be computed automatically
	// for all keys
	uint32_t x, y, w, h;
};

struct layout {
	struct key *keys;
	const char *keymap_name;
	const char *name;
	bool abc; //is this an alphabetical/abjad layout or not? (i.e. something that is a primary input layout)
	uint32_t keyheight; // absolute height (pixels)
};

struct kbd {
	bool debug;
	bool show_popup;
	bool show_highlight;

	struct layout *layout;
	struct clr_scheme *schemes;

	bool print;
	bool print_intersect;
	uint32_t w, h;
	int corner_radius;
	double scale;
	double preferred_scale, preferred_fractional_scale;
	bool landscape;
	bool shift_space_is_tab;
	bool exclusive;
	uint8_t mods;
	uint8_t compose;
	struct key *last_press;
	struct key *last_swipe;
	struct layout *prevlayout; //the previous layout, needed to keep track of keymap changes
	size_t layer_index;
	struct layout *last_abc_layout; //the last alphabetical layout to fall back to (may be further away than prevlayout)
	size_t last_abc_index; //the layer index of the last alphabetical layout
	int64_t last_press_offset; // Delta between processor and compositor time during `last_press`

	struct layout *layouts;
	struct Output *output; //only used to keep track of landscape flipping, never dereferenced
	enum layout_id *layers;
	enum layout_id *landscape_layers;

	struct drwsurf *surf;
	struct drwsurf *popup_surf;
	struct zwp_virtual_keyboard_v1 *vkbd;

	uint32_t last_popup_x, last_popup_y, last_popup_w, last_popup_h;

	/* omarchy-osk: xkb keymap used for typing Char keys */
	struct xkb_context *xkb_ctx;
	struct xkb_keymap *xkb_keymap;
	char *xkb_layout, *xkb_variant;

	/* omarchy-osk: gesture typing */
	bool swipe_enabled;
	struct key *down_key;   // Char key under the finger, typed on release
	bool swiping;
	int npts;
	double *pts_x, *pts_y;
	int pts_cap;
	Color trail;

	/* omarchy-osk: suggestion strip and text tracking */
	uint32_t suggest_h;
	int sugg_pressed;
	struct swipe_result sugg;
	bool sugg_replace;      // suggestions replace the last committed word
	char word[SWIPE_MAX_WORD];      // letters tapped in the current word
	char committed[SWIPE_MAX_WORD]; // last word committed with a trailing space
	bool after_commit;
	bool after_space;       // last key was a tapped space after a word
	int64_t last_shift_tap;
	void (*hide_cb)(void);

	/* omarchy-osk: long-press alternates */
	int64_t longpress_at;   // monotonic ms deadline, 0 when not armed
	bool alt_open;
	uint32_t alt_cps[16];
	int alt_n, alt_sel;
	uint32_t alt_x, alt_y, alt_w, alt_h; // first cell, in popup coordinates
	PangoFontDescription *hint_font;
	char *keymap_str;       // keymap as uploaded, for temporary extra keys

	/* omarchy-osk: space bar gestures */
	enum { SpaceTap = 0, SpaceCursor, SpaceLang } space_mode;
	int lang_dir;
};

void draw_inset(struct drwsurf *ds, uint32_t x, uint32_t y, uint32_t width,
                uint32_t height, uint32_t border, Color color, int rounding);
void draw_over_inset(struct drwsurf *ds, uint32_t x, uint32_t y, uint32_t width,
                     uint32_t height, uint32_t border, Color color, int rounding);

void kbd_init(struct kbd *kb, struct layout *layouts,
              char *layer_names_list, char *landscape_layer_names_list);
void kbd_init_layout(struct layout *l, uint32_t width, uint32_t height,
                     uint32_t yoff);
void kbd_touch_down(struct kbd *kb, uint32_t time, uint32_t x, uint32_t y);
void kbd_touch_motion(struct kbd *kb, uint32_t time, uint32_t x, uint32_t y);
void kbd_touch_up(struct kbd *kb, uint32_t time);
void kbd_reset_text(struct kbd *kb);
void kbd_longpress(struct kbd *kb);
void kbd_load_lang(struct kbd *kb);
struct key *kbd_get_key(struct kbd *kb, uint32_t x, uint32_t y);
size_t kbd_get_layer_index(struct kbd *kb, struct layout *l);
void kbd_unpress_key(struct kbd *kb, uint32_t time);
void kbd_release_key(struct kbd *kb, uint32_t time);
void kbd_motion_key(struct kbd *kb, uint32_t time, uint32_t x, uint32_t y);
void kbd_press_key(struct kbd *kb, struct key *k, uint32_t time);
void kbd_print_key_stdout(struct kbd *kb, struct key *k);
void kbd_print_first_utf8_char_stdout(const char *str);
void kbd_clear_last_popup(struct kbd *kb);
void kbd_draw_key(struct kbd *kb, struct key *k, enum key_draw_type);
void kbd_draw_layout(struct kbd *kb);
void kbd_resize(struct kbd *kb, struct layout *layouts, uint8_t layoutcount);
uint8_t kbd_get_rows(struct layout *l);
double kbd_get_row_length(struct key *k);
void kbd_next_layer(struct kbd *kb, struct key *k, bool invert);
void kbd_switch_layout(struct kbd *kb, struct layout *l, size_t layer_index);
void create_and_upload_keymap(struct kbd *kb, const char *name, uint32_t comp_unichr);
int64_t monotime();

#ifndef LAYOUT
#error "make sure to define LAYOUT"
#endif
#include LAYOUT
#endif
