//! Drawing with cairo and pango.
//!
//! A Canvas records drawing operations and plays them onto its target: an
//! image of its own (tests) or the shared-memory buffer the compositor shows.
//! The buffer may only be written while the compositor doesn't hold it, so
//! operations wait in the list until then; one buffer per surface is enough,
//! with no copy of the surface anywhere else.

use std::f64::consts::PI;

/// blue, green, red, alpha (as wvkbd stores colours)
pub type Color = [u8; 4];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

enum Op {
    Text { color: Color, x: u32, y: u32, w: u32, h: u32, b: u32, label: String, font: pango::FontDescription },
    Clear { x: u32, y: u32, w: u32, h: u32 },
    Rect { color: Color, x: u32, y: u32, w: u32, h: u32, over: bool, rounding: i32 },
    Line { color: Color, x1: f64, y1: f64, x2: f64, y2: f64, width: f64 },
}

struct Target {
    surf: cairo::ImageSurface,
    cr: cairo::Context,
    /// laid out labels: shaping text is the costly part of drawing a key,
    /// and the same labels come back all the time
    layouts: std::cell::RefCell<LayoutCache>,
}

#[derive(Default)]
struct LayoutCache {
    map: std::collections::HashMap<(String, u64, i32, i32), Vec<(pango::FontDescription, pango::Layout, (i32, i32))>>,
    entries: usize,
}

const LAYOUT_CACHE: usize = 512;

impl Target {
    /// a layout of label in font, wrapped to w x h (pango units), and its
    /// pixel size
    fn layout(&self, label: &str, font: &pango::FontDescription, w: i32, h: i32) -> (pango::Layout, (i32, i32)) {
        let mut cache = self.layouts.borrow_mut();
        let fh = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            font.hash(&mut h);
            h.finish()
        };
        let key = (label.to_string(), fh, w, h);
        if let Some(hit) = cache.map.get(&key).and_then(|v| v.iter().find(|(f, _, _)| f == font)) {
            return (hit.1.clone(), hit.2);
        }
        let layout = pangocairo::functions::create_layout(&self.cr);
        layout.set_auto_dir(false);
        layout.set_font_description(Some(font));
        layout.set_text(label);
        layout.set_width(w);
        layout.set_height(h);
        let size = layout.pixel_size();
        if cache.entries >= LAYOUT_CACHE {
            cache.map.clear();
            cache.entries = 0;
        }
        cache.map.entry(key).or_default().push((font.clone(), layout.clone(), size));
        cache.entries += 1;
        (layout, size)
    }
}

/// memory cairo draws into, e.g. a mapped shared-memory buffer
pub struct Memory {
    ptr: *mut u8,
    len: usize,
    release: Option<Box<dyn FnOnce()>>,
}

impl Memory {
    /// # Safety
    /// ptr must be valid for len bytes until release is called
    pub unsafe fn new(ptr: *mut u8, len: usize, release: impl FnOnce() + 'static) -> Memory {
        Memory { ptr, len, release: Some(Box::new(release)) }
    }
}

impl AsMut<[u8]> for Memory {
    fn as_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for Memory {
    fn drop(&mut self) {
        if let Some(r) = self.release.take() {
            r();
        }
    }
}

pub struct Canvas {
    ops: Vec<Op>,
    target: Option<Target>,
    pub scale: f64,
    pw: i32,
    ph: i32,
    /// changed areas in surface coordinates since the last take_damage
    damage: Vec<Rect>,
}

fn set_source(cr: &cairo::Context, c: Color) {
    cr.set_source_rgba(c[2] as f64 / 255.0, c[1] as f64 / 255.0, c[0] as f64 / 255.0, c[3] as f64 / 255.0);
}

fn target(surf: cairo::ImageSurface, s: f64) -> Target {
    let cr = cairo::Context::new(&surf).expect("cairo context");
    cr.scale(s, s);
    cr.set_antialias(cairo::Antialias::None);
    cr.save().ok();
    Target { surf, cr, layouts: Default::default() }
}

impl Op {
    fn play(&self, t: &Target) {
        let cr = &t.cr;
        cr.save().ok();
        match self {
            Op::Text { color, x, y, w, h, b, label, font } => {
                set_source(cr, *color);
                cr.move_to(x.wrapping_add(w / 2) as f64, y.wrapping_add(h / 2) as f64);
                let (layout, (width, height)) = t.layout(
                    label,
                    font,
                    (w.wrapping_sub(b * 2) as i32).wrapping_mul(pango::SCALE),
                    (h.wrapping_sub(b * 2) as i32).wrapping_mul(pango::SCALE),
                );
                cr.rel_move_to((-width / 2) as f64, (-height / 2) as f64);
                pangocairo::functions::show_layout(cr, &layout);
            }
            Op::Clear { x, y, w, h } => {
                cr.set_operator(cairo::Operator::Clear);
                cr.rectangle(*x as f64, *y as f64, *w as f64, *h as f64);
                cr.fill().ok();
            }
            Op::Rect { color, x, y, w, h, over, rounding } => {
                cr.set_operator(if *over { cairo::Operator::Over } else { cairo::Operator::Source });
                let (xf, yf, wf, hf) = (*x as f64, *y as f64, *w as f64, *h as f64);
                if *rounding > 0 {
                    let mut radius = *rounding as f64;
                    if radius * 2.0 > wf {
                        radius = wf / 2.0;
                    }
                    if radius * 2.0 > hf {
                        radius = hf / 2.0;
                    }
                    cr.set_antialias(cairo::Antialias::Default);
                    let deg = PI / 180.0;
                    cr.new_sub_path();
                    cr.arc(xf + wf - radius, yf + radius, radius, -90.0 * deg, 0.0 * deg);
                    cr.arc(xf + wf - radius, yf + hf - radius, radius, 0.0 * deg, 90.0 * deg);
                    cr.arc(xf + radius, yf + hf - radius, radius, 90.0 * deg, 180.0 * deg);
                    cr.arc(xf + radius, yf + radius, radius, 180.0 * deg, 270.0 * deg);
                    cr.close_path();
                } else {
                    cr.rectangle(xf, yf, wf, hf);
                }
                set_source(cr, *color);
                cr.fill().ok();
            }
            Op::Line { color, x1, y1, x2, y2, width } => {
                cr.set_antialias(cairo::Antialias::Default);
                cr.set_operator(cairo::Operator::Source);
                cr.set_line_cap(cairo::LineCap::Round);
                cr.set_line_width(*width);
                set_source(cr, *color);
                cr.move_to(*x1, *y1);
                cr.line_to(*x2, *y2);
                cr.stroke().ok();
            }
        }
        cr.restore().ok();
    }
}

impl Canvas {
    /// a w x h surface (in surface coordinates) drawn at scale s; without a
    /// target the operations wait for one (set_memory)
    pub fn new(w: u32, h: u32, s: f64, own_image: bool) -> Canvas {
        let pw = ((w as f64 * s).ceil() as i32).max(1);
        let ph = ((h as f64 * s).ceil() as i32).max(1);
        let target = own_image.then(|| {
            target(cairo::ImageSurface::create(cairo::Format::ARgb32, pw, ph).expect("cairo surface"), s)
        });
        Canvas { ops: Vec::new(), target, scale: s, pw, ph, damage: Vec::new() }
    }

    pub fn pixel_width(&self) -> i32 {
        self.pw
    }

    pub fn pixel_height(&self) -> i32 {
        self.ph
    }

    /// draw into this memory from now on (ARGB32, pixel_width x
    /// pixel_height, zeroed or holding what was drawn so far)
    pub fn set_memory(&mut self, mem: Memory) {
        let stride = self.pw * 4;
        let surf = cairo::ImageSurface::create_for_data(mem, cairo::Format::ARgb32, self.pw, self.ph, stride).expect("cairo surface");
        self.target = Some(target(surf, self.scale));
    }

    /// play the waiting operations onto the target
    pub fn flush(&mut self) {
        if let Some(t) = &self.target {
            for op in self.ops.drain(..) {
                op.play(t);
            }
            t.surf.flush();
        }
    }

    pub fn has_pending(&self) -> bool {
        !self.ops.is_empty()
    }

    fn push(&mut self, op: Op, x: u32, y: u32, w: u32, h: u32) {
        self.damage.push(Rect { x: x as i32, y: y as i32, w: w as i32, h: h as i32 });
        self.ops.push(op);
    }

    pub fn take_damage(&mut self) -> Vec<Rect> {
        std::mem::take(&mut self.damage)
    }

    pub fn has_damage(&self) -> bool {
        !self.damage.is_empty()
    }

    pub fn draw_text(&mut self, color: Color, x: u32, y: u32, w: u32, h: u32, b: u32, label: &str, font: &pango::FontDescription) {
        self.push(Op::Text { color, x, y, w, h, b, label: label.into(), font: font.clone() }, x, y, w, h);
    }

    pub fn clear(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.push(Op::Clear { x, y, w, h }, x, y, w, h);
    }

    pub fn fill_rectangle(&mut self, color: Color, x: u32, y: u32, w: u32, h: u32, rounding: i32) {
        self.push(Op::Rect { color, x, y, w, h, over: false, rounding }, x, y, w, h);
    }

    pub fn over_rectangle(&mut self, color: Color, x: u32, y: u32, w: u32, h: u32, rounding: i32) {
        self.push(Op::Rect { color, x, y, w, h, over: true, rounding }, x, y, w, h);
    }

    pub fn line(&mut self, color: Color, x1: f64, y1: f64, x2: f64, y2: f64, width: f64) {
        let pad = width / 2.0 + 2.0;
        let minx = (x1.min(x2) - pad).max(0.0);
        let miny = (y1.min(y2) - pad).max(0.0);
        let (w, h) = (((x2 - x1).abs() + pad * 2.0) as u32, ((y2 - y1).abs() + pad * 2.0) as u32);
        self.push(Op::Line { color, x1, y1, x2, y2, width }, minx as u32, miny as u32, w, h);
    }

    pub fn write_png(&mut self, path: &str) -> std::io::Result<()> {
        self.flush();
        let t = self.target.as_ref().ok_or_else(|| std::io::Error::other("no target"))?;
        let mut f = std::fs::File::create(path)?;
        t.surf.write_to_png(&mut f).map_err(std::io::Error::other)
    }
}
