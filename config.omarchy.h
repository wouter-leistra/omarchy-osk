#ifndef config_h_INCLUDED
#define config_h_INCLUDED

#define DEFAULT_FONT "Sans 20"
#define DEFAULT_ROUNDING 10
static const int transparency = 255;

/* 0: letters, 1: modifiers/specials, 2: accent (enter)
 * colours are normally passed in from the omarchy theme by omarchy-osk */
struct clr_scheme schemes[] = {
{
  .bg = {.bgra = {0x2e, 0x1e, 0x1e, transparency}},
  .fg = {.bgra = {0x44, 0x32, 0x31, transparency}},
  .high = {.bgra = {0x70, 0x5b, 0x58, transparency}},
  .swipe = {.bgra = {0xfa, 0xb4, 0x89, transparency}},
  .text = {.bgra = {0xf4, 0xd6, 0xcd, transparency}},
  .text_press = {.bgra = {0xf4, 0xd6, 0xcd, transparency}},
  .text_swipe = {.bgra = {0xf4, 0xd6, 0xcd, transparency}},
  .font = DEFAULT_FONT,
  .rounding = DEFAULT_ROUNDING,
},
{
  .bg = {.bgra = {0x2e, 0x1e, 0x1e, transparency}},
  .fg = {.bgra = {0x22, 0x16, 0x16, transparency}},
  .high = {.bgra = {0x70, 0x5b, 0x58, transparency}},
  .swipe = {.bgra = {0xfa, 0xb4, 0x89, transparency}},
  .text = {.bgra = {0xf4, 0xd6, 0xcd, transparency}},
  .text_press = {.bgra = {0xf4, 0xd6, 0xcd, transparency}},
  .text_swipe = {.bgra = {0xf4, 0xd6, 0xcd, transparency}},
  .font = DEFAULT_FONT,
  .rounding = DEFAULT_ROUNDING,
},
{
  .bg = {.bgra = {0x2e, 0x1e, 0x1e, transparency}},
  .fg = {.bgra = {0xfa, 0xb4, 0x89, transparency}},
  .high = {.bgra = {0xfd, 0xd4, 0xb8, transparency}},
  .swipe = {.bgra = {0xfa, 0xb4, 0x89, transparency}},
  .text = {.bgra = {0x2e, 0x1e, 0x1e, transparency}},
  .text_press = {.bgra = {0x2e, 0x1e, 0x1e, transparency}},
  .text_swipe = {.bgra = {0x2e, 0x1e, 0x1e, transparency}},
  .font = DEFAULT_FONT,
  .rounding = DEFAULT_ROUNDING,
},
};

static enum layout_id layers[] = {
  Abc,
  Sym,
  NumLayouts
};

static enum layout_id landscape_layers[] = {
  Abc,
  Sym,
  NumLayouts
};

#endif
