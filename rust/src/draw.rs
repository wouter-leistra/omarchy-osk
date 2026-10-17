//! Drawing with cairo and pango, the same operations as drw.c, into an
//! image the Wayland side copies to shared-memory buffers.

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

pub struct Canvas {
    pub surf: cairo::ImageSurface,
    cr: cairo::Context,
    layout: pango::Layout,
    pub scale: f64,
    /// changed areas in surface coordinates since the last take_damage
    damage: Vec<Rect>,
}

fn set_source(cr: &cairo::Context, c: Color) {
    cr.set_source_rgba(c[2] as f64 / 255.0, c[1] as f64 / 255.0, c[0] as f64 / 255.0, c[3] as f64 / 255.0);
}

impl Canvas {
    /// a w x h surface (in surface coordinates) drawn at scale s
    pub fn new(w: u32, h: u32, s: f64) -> Canvas {
        let pw = (w as f64 * s).ceil() as i32;
        let ph = (h as f64 * s).ceil() as i32;
        let surf = cairo::ImageSurface::create(cairo::Format::ARgb32, pw.max(1), ph.max(1)).expect("cairo surface");
        let cr = cairo::Context::new(&surf).expect("cairo context");
        cr.scale(s, s);
        cr.set_antialias(cairo::Antialias::None);
        let layout = pangocairo::functions::create_layout(&cr);
        layout.set_auto_dir(false);
        cr.save().ok();
        Canvas { surf, cr, layout, scale: s, damage: Vec::new() }
    }

    pub fn pixel_width(&self) -> i32 {
        self.surf.width()
    }

    pub fn pixel_height(&self) -> i32 {
        self.surf.height()
    }

    fn damage(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.damage.push(Rect { x: x as i32, y: y as i32, w: w as i32, h: h as i32 });
    }

    pub fn take_damage(&mut self) -> Vec<Rect> {
        std::mem::take(&mut self.damage)
    }

    pub fn has_damage(&self) -> bool {
        !self.damage.is_empty()
    }

    pub fn damage_all(&mut self) {
        let (w, h) = ((self.surf.width() as f64 / self.scale).ceil() as u32, (self.surf.height() as f64 / self.scale).ceil() as u32);
        self.damage(0, 0, w, h);
    }

    pub fn draw_text(&mut self, color: Color, x: u32, y: u32, w: u32, h: u32, b: u32, label: &str, font: &pango::FontDescription) {
        self.damage(x, y, w, h);
        let cr = &self.cr;
        cr.save().ok();
        self.layout.set_font_description(Some(font));
        set_source(cr, color);
        cr.move_to(x.wrapping_add(w / 2) as f64, y.wrapping_add(h / 2) as f64);
        self.layout.set_text(label);
        self.layout.set_width((w.wrapping_sub(b * 2) as i32).wrapping_mul(pango::SCALE));
        self.layout.set_height((h.wrapping_sub(b * 2) as i32).wrapping_mul(pango::SCALE));
        let (width, height) = self.layout.pixel_size();
        cr.rel_move_to((-width / 2) as f64, (-height / 2) as f64);
        pangocairo::functions::show_layout(cr, &self.layout);
        cr.restore().ok();
    }

    pub fn clear(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.damage(x, y, w, h);
        let cr = &self.cr;
        cr.save().ok();
        cr.set_operator(cairo::Operator::Clear);
        cr.rectangle(x as f64, y as f64, w as f64, h as f64);
        cr.fill().ok();
        cr.restore().ok();
    }

    fn rectangle(&mut self, color: Color, x: u32, y: u32, w: u32, h: u32, over: bool, rounding: i32) {
        self.damage(x, y, w, h);
        let cr = &self.cr;
        cr.save().ok();
        cr.set_operator(if over { cairo::Operator::Over } else { cairo::Operator::Source });
        let (xf, yf, wf, hf) = (x as f64, y as f64, w as f64, h as f64);
        if rounding > 0 {
            let mut radius = rounding as f64;
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
            set_source(cr, color);
            cr.fill().ok();
        } else {
            cr.rectangle(xf, yf, wf, hf);
            set_source(cr, color);
            cr.fill().ok();
        }
        cr.restore().ok();
    }

    pub fn fill_rectangle(&mut self, color: Color, x: u32, y: u32, w: u32, h: u32, rounding: i32) {
        self.rectangle(color, x, y, w, h, false, rounding);
    }

    pub fn over_rectangle(&mut self, color: Color, x: u32, y: u32, w: u32, h: u32, rounding: i32) {
        self.rectangle(color, x, y, w, h, true, rounding);
    }

    pub fn line(&mut self, color: Color, x1: f64, y1: f64, x2: f64, y2: f64, width: f64) {
        let pad = width / 2.0 + 2.0;
        let minx = (x1.min(x2) - pad).max(0.0);
        let miny = (y1.min(y2) - pad).max(0.0);
        self.damage(minx as u32, miny as u32, ((x2 - x1).abs() + pad * 2.0) as u32, ((y2 - y1).abs() + pad * 2.0) as u32);
        let cr = &self.cr;
        cr.save().ok();
        cr.set_antialias(cairo::Antialias::Default);
        cr.set_operator(cairo::Operator::Source);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_width(width);
        set_source(cr, color);
        cr.move_to(x1, y1);
        cr.line_to(x2, y2);
        cr.stroke().ok();
        cr.restore().ok();
    }

    pub fn write_png(&mut self, path: &str) -> std::io::Result<()> {
        self.surf.flush();
        let mut f = std::fs::File::create(path)?;
        self.surf.write_to_png(&mut f).map_err(std::io::Error::other)
    }
}
