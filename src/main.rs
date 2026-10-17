//! omarchy-oskd: the Wayland side of the keyboard (main.c of the C version).
//!
//! A layer surface at the bottom of the screen holds the keys, an xdg popup
//! twice its height (no input) shows key previews, long-press keys and the
//! gesture trail. Keys are typed with a virtual keyboard; touch, pointer and
//! tablet pens drive it. Signals: USR1 hide, USR2 show, RTMIN toggle,
//! RTMIN+1 text field changed, RTMIN+2/+3 private mode on/off.

use omarchy_osk::draw::Memory;
use omarchy_osk::keyboard::{monotime, Kbd, Output};
use omarchy_osk::options::{self, Dock, Options, WlLayer};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{
    wl_buffer::{self, WlBuffer},
    wl_callback::{self, WlCallback},
    wl_compositor::WlCompositor,
    wl_pointer::{self, WlPointer},
    wl_region::WlRegion,
    wl_registry::WlRegistry,
    wl_seat::{self, WlSeat},
    wl_shm::{self, WlShm},
    wl_shm_pool::WlShmPool,
    wl_surface::{self, WlSurface},
    wl_touch::{self, WlTouch},
};
use wayland_client::{event_created_child, Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
use wayland_protocols::wp::tablet::zv2::client::{
    zwp_tablet_manager_v2::ZwpTabletManagerV2,
    zwp_tablet_pad_dial_v2::ZwpTabletPadDialV2,
    zwp_tablet_pad_group_v2::{self, ZwpTabletPadGroupV2},
    zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2,
    zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2,
    zwp_tablet_pad_v2::{self, ZwpTabletPadV2},
    zwp_tablet_seat_v2::{self, ZwpTabletSeatV2},
    zwp_tablet_tool_v2::{self, ZwpTabletToolV2},
    zwp_tablet_v2::ZwpTabletV2,
};
use wayland_protocols::wp::viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter};
use wayland_protocols::xdg::shell::client::{
    xdg_popup::{self, XdgPopup},
    xdg_positioner::XdgPositioner,
    xdg_surface::{self, XdgSurface},
    xdg_wm_base::{self, XdgWmBase},
};
use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
    zwp_input_method_v2::{self, ZwpInputMethodV2},
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, Anchor, ZwlrLayerSurfaceV1},
};

const NAMESPACE: &str = "omarchy-osk";

macro_rules! die {
    ($($t:tt)*) => {{
        eprint!($($t)*);
        std::process::exit(1)
    }};
}

/* ---------------------------------------------------------------------- */
/* shared memory buffers                                                   */

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Which {
    Keys,
    Popup,
}

/// the one shared-memory buffer of a surface; the canvas draws into it
/// while the compositor doesn't hold it. Buffers outlive the surfaces: what
/// the next show needs is drawn into them while the keyboard is hidden.
struct ShmBuf {
    id: u64,
    buffer: WlBuffer,
    busy: bool,
}

impl Drop for ShmBuf {
    fn drop(&mut self) {
        self.buffer.destroy();
    }
}

struct Surf {
    surface: WlSurface,
    frame_pending: bool,
    /// attach even without changes (a freshly configured popup)
    force: bool,
    /// a new generation for every surface, so events of old ones are ignored
    gen: u32,
}

fn anon_file(size: usize) -> OwnedFd {
    let fd = unsafe { libc::memfd_create(c"omarchy-osk".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 || unsafe { libc::ftruncate(fd, size as libc::off_t) } < 0 {
        die!("could not create shared memory file\n");
    }
    unsafe { OwnedFd::from_raw_fd(fd) }
}

/* ---------------------------------------------------------------------- */
/* the virtual keyboard as Output                                          */

struct Vk<'a> {
    vk: &'a ZwpVirtualKeyboardV1,
    hide: &'a mut bool,
}

impl Output for Vk<'_> {
    fn key(&mut self, time: u32, key: u32, state: u32) {
        self.vk.key(time, key, state);
    }
    fn modifiers(&mut self, d: u32, l: u32, k: u32, g: u32) {
        self.vk.modifiers(d, l, k, g);
    }
    fn keymap(&mut self, text: &str) {
        let size = text.len() + 1;
        let fd = anon_file(size);
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        let n = unsafe { libc::pwrite(fd.as_raw_fd(), bytes.as_ptr() as *const libc::c_void, size, 0) };
        if n as usize != size {
            die!("could not write keymap\n");
        }
        self.vk.keymap(1, fd.as_fd(), size as u32);
    }
    fn hide(&mut self) {
        *self.hide = true;
    }
}

/* ---------------------------------------------------------------------- */

#[derive(Default)]
struct Pen {
    on_keyboard: bool,
    down: bool,
    pending_down: bool,
    pending_up: bool,
    moved: bool,
    x: i32,
    y: i32,
}

struct App {
    kb: Kbd,
    opts: Options,
    qh: QueueHandle<App>,
    compositor: WlCompositor,
    shm: WlShm,
    seat: Option<WlSeat>,
    layer_shell: ZwlrLayerShellV1,
    wm_base: XdgWmBase,
    wfs_mgr: Option<WpFractionalScaleManagerV1>,
    viewporter: Option<WpViewporter>,
    vkbd: Option<ZwpVirtualKeyboardV1>,
    pointer: Option<WlPointer>,
    touch: Option<WlTouch>,
    empty_region: Option<WlRegion>,
    positioner: Option<XdgPositioner>,

    keys: Option<Surf>,
    popup: Option<Surf>,
    layer_surface: Option<ZwlrLayerSurfaceV1>,
    wfs: Option<WpFractionalScaleV1>,
    viewport: Option<WpViewport>,
    popup_viewport: Option<WpViewport>,
    popup_xdg_surface: Option<XdgSurface>,
    popup_xdg_popup: Option<XdgPopup>,
    layer_configured: bool,
    popup_configured: bool,
    gen: u32,
    buf_serial: u64,
    bufs: [Option<ShmBuf>; 2],

    available: (u32, u32),
    height: u32,
    hidden: bool,
    run: bool,
    want_show: bool,
    want_hide: bool,
    show_started: i64,
    preferred_scale: f64,
    preferred_fractional_scale: f64,

    active_touch: i32,
    cur: (i32, i32),
    cur_press: bool,
    pen: Pen,
}

impl App {
    fn ready(&self) -> bool {
        self.layer_configured && self.popup_configured
    }

    /// run a keyboard method that sends to the compositor
    fn with_out<R>(&mut self, f: impl FnOnce(&mut Kbd, &mut dyn Output) -> R) -> R {
        let vk = self.vkbd.clone().expect("virtual keyboard");
        let mut hide = false;
        let r = f(&mut self.kb, &mut Vk { vk: &vk, hide: &mut hide });
        if hide {
            self.want_hide = true;
        }
        r
    }

    fn time_now(&self) -> u32 {
        (monotime() + self.kb.last_press_offset) as u32
    }

    fn redimension(&mut self) {
        let (aw, ah) = self.available;
        self.kb.landscape = aw > ah;
        let layer = if self.kb.landscape {
            self.height = self.opts.landscape_height;
            self.kb.landscape_layers[0]
        } else {
            self.height = self.opts.normal_height;
            self.kb.layers[0]
        };
        self.kb.w = if self.opts.custom_width > 0 {
            self.opts.custom_width
        } else if self.opts.dock != Dock::Full {
            (aw as f64 * self.opts.width_ratio) as u32
        } else {
            aw
        };
        self.kb.h = self.height;
        self.kb.layout = layer;
        self.kb.layer_index = 0;
        self.kb.prevlayout = layer;
        self.kb.last_abc_layout = layer;
        self.kb.last_abc_index = 0;
    }

    fn wl_layer(&self) -> zwlr_layer_shell_v1::Layer {
        match self.opts.layer {
            WlLayer::Background => zwlr_layer_shell_v1::Layer::Background,
            WlLayer::Bottom => zwlr_layer_shell_v1::Layer::Bottom,
            WlLayer::Top => zwlr_layer_shell_v1::Layer::Top,
            WlLayer::Overlay => zwlr_layer_shell_v1::Layer::Overlay,
        }
    }

    fn hide(&mut self) {
        let Some(ls) = self.layer_surface.take() else { return };
        // ensure active keys are released, so they're not repeated forever
        let t = self.time_now();
        self.with_out(|kb, o| {
            let _ = t;
            kb.on_hide(o)
        });
        self.active_touch = -1;
        if let Some(w) = self.wfs.take() {
            w.destroy();
        }
        if let Some(v) = self.viewport.take() {
            v.destroy();
        }
        if let Some(v) = self.popup_viewport.take() {
            v.destroy();
        }
        if let Some(p) = self.popup_xdg_popup.take() {
            p.destroy();
        }
        if let Some(s) = self.popup_xdg_surface.take() {
            s.destroy();
        }
        if let Some(s) = self.popup.take() {
            s.surface.destroy();
        }
        ls.destroy();
        self.layer_configured = false;
        self.popup_configured = false;
        if let Some(s) = self.keys.take() {
            s.surface.destroy();
        }
        self.hidden = true;
        self.prerender();
    }

    /// draw now what the next show will look like (the state is reset
    /// exactly as redimension does), so showing only attaches the buffers
    fn prerender(&mut self) {
        if self.kb.surf.is_none() {
            return;
        }
        let layer = if self.kb.landscape { self.kb.landscape_layers[0] } else { self.kb.layers[0] };
        self.kb.layout = layer;
        self.kb.layer_index = 0;
        self.kb.prevlayout = layer;
        self.kb.last_abc_layout = layer;
        self.kb.last_abc_index = 0;
        self.kb.draw_layout();
        let (w, h) = (self.kb.w, self.kb.h);
        if let Some(p) = self.kb.popup_surf.as_mut() {
            p.clear(0, 0, w, h * 2);
        }
    }

    /// while hidden, play what was drawn into the buffers
    fn flush_hidden(&mut self) {
        if !self.hidden {
            return;
        }
        for (i, c) in [self.kb.surf.as_mut(), self.kb.popup_surf.as_mut()].into_iter().enumerate() {
            if let (Some(c), Some(b)) = (c, self.bufs[i].as_ref()) {
                if !b.busy && c.has_pending() {
                    c.flush();
                }
            }
        }
    }

    /// buffers that still fit and are free: the next show can use them
    fn ready_to_reuse(&self, scale: f64) -> bool {
        let fits = |c: Option<&omarchy_osk::draw::Canvas>, w: u32, h: u32| {
            c.is_some_and(|c| c.scale == scale && c.pixel_width() == ((w as f64 * scale).ceil() as i32).max(1) && c.pixel_height() == ((h as f64 * scale).ceil() as i32).max(1))
        };
        fits(self.kb.surf.as_ref(), self.kb.w, self.kb.h)
            && fits(self.kb.popup_surf.as_ref(), self.kb.w, self.kb.h * 2)
            && self.bufs.iter().all(|b| b.as_ref().is_some_and(|b| !b.busy))
    }

    fn new_surf(&mut self) -> Surf {
        self.gen += 1;
        let surface = self.compositor.create_surface(&self.qh, self.gen);
        Surf { surface, frame_pending: false, force: false, gen: self.gen }
    }

    /* drawing: the canvas plays its operations into the surface's buffer
     * whenever the compositor has released it */

    /// a buffer for the surface, the canvas draws into its memory
    fn give_buffer(&mut self, which: Which) {
        let canvas = match which {
            Which::Keys => self.kb.surf.as_mut(),
            Which::Popup => self.kb.popup_surf.as_mut(),
        };
        let Some(canvas) = canvas else { return };
        let (pw, ph) = (canvas.pixel_width(), canvas.pixel_height());
        let stride = pw * 4;
        let size = (stride * ph) as usize;
        let fd = anon_file(size);
        let data = unsafe { libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd.as_raw_fd(), 0) };
        if data == libc::MAP_FAILED {
            die!("could not map buffer\n");
        }
        let pool: WlShmPool = self.shm.create_pool(fd.as_fd(), size as i32, &self.qh, ());
        self.buf_serial += 1;
        let id = self.buf_serial;
        let buffer = pool.create_buffer(0, pw, ph, stride, wl_shm::Format::Argb8888, &self.qh, (which, id));
        pool.destroy();
        let addr = data as usize;
        canvas.set_memory(unsafe { Memory::new(data as *mut u8, size, move || {
            libc::munmap(addr as *mut libc::c_void, size);
        }) });
        self.bufs[which as usize] = Some(ShmBuf { id, buffer, busy: false });
    }

    fn present(&mut self, which: Which) {
        let (surf, canvas) = match which {
            Which::Keys if self.layer_configured => (self.keys.as_mut(), self.kb.surf.as_mut()),
            Which::Popup if self.popup_configured => (self.popup.as_mut(), self.kb.popup_surf.as_mut()),
            _ => return,
        };
        let (Some(surf), Some(canvas)) = (surf, canvas) else { return };
        let Some(buf) = self.bufs[which as usize].as_mut() else { return };
        if surf.frame_pending || buf.busy || !(canvas.has_damage() || surf.force) {
            return;
        }
        canvas.flush();
        let damage = canvas.take_damage();
        surf.surface.attach(Some(&buf.buffer), 0, 0);
        if surf.force {
            surf.surface.damage(0, 0, i32::MAX, i32::MAX);
        }
        for r in &damage {
            surf.surface.damage(r.x, r.y, r.w, r.h);
        }
        buf.busy = true;
        surf.force = false;
        surf.surface.frame(&self.qh, (which, surf.gen));
        surf.frame_pending = true;
        surf.surface.commit();
    }

    fn present_all(&mut self) {
        self.present(Which::Keys);
        self.present(Which::Popup);
    }

    /// layer surface configure for the visible keyboard
    fn configure(&mut self, serial: u32, w: u32, h: u32) {
        if self.kb.w != w || self.kb.h != h {
            // not what we expected, or redimension: refresh and restart
            if let Some(ls) = &self.layer_surface {
                ls.ack_configure(serial);
            }
            self.hide();
            self.want_show = true;
            return;
        }
        if self.layer_configured {
            return;
        }
        self.layer_configured = true;
        let scale = if self.preferred_fractional_scale != 0.0 { self.preferred_fractional_scale } else { self.preferred_scale };
        self.kb.scale = scale;
        self.hidden = false;
        let fractional = self.wfs_mgr.is_some() && self.viewporter.is_some();
        let keys_surface = self.keys.as_ref().unwrap().surface.clone();
        if fractional {
            self.viewport.as_ref().unwrap().set_destination(self.kb.w as i32, self.kb.h as i32);
        } else {
            keys_surface.set_buffer_scale(scale as i32);
        }

        let popup = self.new_surf();
        let pos = self.positioner.as_ref().unwrap();
        pos.set_size(w as i32, (h * 2) as i32);
        pos.set_anchor_rect(0, -(h as i32), w as i32, (h * 2) as i32);
        popup.surface.set_input_region(self.empty_region.as_ref());
        let xs = self.wm_base.get_xdg_surface(&popup.surface, &self.qh, popup.gen);
        self.popup_configured = false;
        let xp = xs.get_popup(None, pos, &self.qh, ());
        self.layer_surface.as_ref().unwrap().get_popup(&xp);
        if fractional {
            let vp = self.viewporter.as_ref().unwrap().get_viewport(&popup.surface, &self.qh, ());
            vp.set_destination(self.kb.w as i32, (self.kb.h * 2) as i32);
            self.popup_viewport = Some(vp);
        } else {
            popup.surface.set_buffer_scale(scale as i32);
        }
        popup.surface.commit();
        self.popup = Some(popup);
        self.popup_xdg_surface = Some(xs);
        self.popup_xdg_popup = Some(xp);

        self.layer_surface.as_ref().unwrap().ack_configure(serial);
        let t = monotime();
        if !self.ready_to_reuse(scale) {
            self.kb.resize();
            self.give_buffer(Which::Keys);
            self.give_buffer(Which::Popup);
        }
        self.keys.as_mut().unwrap().force = true;
        self.present(Which::Keys);
        if self.kb.debug {
            eprintln!("show: configured after {}ms, drawing took {}ms", t - self.show_started, monotime() - t);
        }
    }

    fn touch_down(&mut self, time: u32, x: i32, y: i32) {
        self.with_out(|kb, o| kb.touch_down(o, time, x, y));
    }
    fn touch_motion(&mut self, time: u32, x: i32, y: i32) {
        self.with_out(|kb, o| kb.touch_motion(o, time, x, y));
    }
    fn touch_up(&mut self, time: u32) {
        self.with_out(|kb, o| kb.touch_up(o, time));
    }
}

/// temporary layer surface over the whole output, to learn its size
fn refresh_available_dimension(app: &mut App, queue: &mut EventQueue<App>) {
    let surface = app.compositor.create_surface(&app.qh, 0);
    let ls = app.layer_shell.get_layer_surface(&surface, None, app.wl_layer(), NAMESPACE.into(), &app.qh, true);
    ls.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
    surface.commit();
    if queue.roundtrip(app).is_err() {
        die!("Wayland roundtrip failed\n");
    }
    ls.destroy();
    surface.destroy();
}

fn show(app: &mut App, queue: &mut EventQueue<App>) {
    if app.layer_surface.is_some() {
        return;
    }
    app.show_started = monotime();
    refresh_available_dimension(app, queue);
    if app.kb.debug {
        eprintln!("show: dimensions after {}ms", monotime() - app.show_started);
    }
    app.redimension();

    let keys = app.new_surf();
    if let (Some(wfs_mgr), Some(vp)) = (&app.wfs_mgr, &app.viewporter) {
        app.wfs = Some(wfs_mgr.get_fractional_scale(&keys.surface, &app.qh, ()));
        app.viewport = Some(vp.get_viewport(&keys.surface, &app.qh, ()));
    }
    let ls = app.layer_shell.get_layer_surface(&keys.surface, None, app.wl_layer(), NAMESPACE.into(), &app.qh, false);
    let (anchor, req_w) = match app.opts.dock {
        Dock::Left => (Anchor::Bottom | Anchor::Left, app.kb.w),
        Dock::Right => (Anchor::Bottom | Anchor::Right, app.kb.w),
        Dock::Full => (Anchor::Bottom | Anchor::Left | Anchor::Right, 0),
    };
    ls.set_size(req_w, app.height);
    ls.set_anchor(anchor);
    if app.opts.exclusive {
        ls.set_exclusive_zone(app.height as i32);
    }
    ls.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
    keys.surface.commit();
    app.keys = Some(keys);
    app.layer_surface = Some(ls);
}

/// the signals the keyboard handles, blocked from the very start: one that
/// arrives while it is still starting waits instead of ending the process
fn block_signals() -> libc::sigset_t {
    let sigrtmin = libc::SIGRTMIN();
    unsafe {
        let mut mask: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut mask);
        for s in [libc::SIGUSR1, libc::SIGUSR2, sigrtmin, sigrtmin + 1, sigrtmin + 2, sigrtmin + 3, libc::SIGPIPE] {
            libc::sigaddset(&mut mask, s);
        }
        if libc::sigprocmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut()) == -1 {
            die!("Failed to disable handled signals\n");
        }
        mask
    }
}

/// block them before main, as soon as the program's own code runs
#[used]
#[link_section = ".init_array"]
static BLOCK_SIGNALS_EARLY: extern "C" fn() = {
    extern "C" fn early() {
        block_signals();
    }
    early
};

fn main() {
    let mask = block_signals();
    let args: Vec<String> = std::env::args().collect();
    let opts = options::parse(&args);
    let mut kb = opts.build_kbd();
    kb.landscape = true;
    kb.own_images = false;

    let conn = Connection::connect_to_env().unwrap_or_else(|_| die!("Failed to create display\n"));
    let (globals, mut queue) = registry_queue_init::<App>(&conn).unwrap_or_else(|_| die!("Failed to get the Wayland globals\n"));
    let qh = queue.handle();

    let compositor: WlCompositor = globals.bind(&qh, 1..=6, ()).unwrap_or_else(|_| die!("wl_compositor not available\n"));
    let shm: WlShm = globals.bind(&qh, 1..=1, ()).unwrap_or_else(|_| die!("wl_shm not available\n"));
    let seat: Option<WlSeat> = globals.bind(&qh, 1..=1, ()).ok();
    let layer_shell: ZwlrLayerShellV1 = globals.bind(&qh, 1..=1, ()).unwrap_or_else(|_| die!("layer_shell not available\n"));
    let wm_base: XdgWmBase = globals.bind(&qh, 1..=1, ()).unwrap_or_else(|_| die!("wm_base not available\n"));
    let wfs_mgr: Option<WpFractionalScaleManagerV1> = globals.bind(&qh, 1..=1, ()).ok();
    let viewporter: Option<WpViewporter> = globals.bind(&qh, 1..=1, ()).ok();
    let vkbd_mgr: ZwpVirtualKeyboardManagerV1 =
        globals.bind(&qh, 1..=1, ()).unwrap_or_else(|_| die!("virtual_keyboard_manager not available\n"));
    let tablet_mgr: Option<ZwpTabletManagerV2> = globals.bind(&qh, 1..=1, ()).ok();
    let im_mgr: Option<ZwpInputMethodManagerV2> = if opts.im_auto { globals.bind(&qh, 1..=1, ()).ok() } else { None };

    let hidden = opts.hidden;
    let mut app = App {
        kb,
        opts,
        qh: qh.clone(),
        compositor,
        shm,
        seat,
        layer_shell,
        wm_base,
        wfs_mgr,
        viewporter,
        vkbd: None,
        pointer: None,
        touch: None,
        empty_region: None,
        positioner: None,
        keys: None,
        popup: None,
        layer_surface: None,
        wfs: None,
        viewport: None,
        popup_viewport: None,
        popup_xdg_surface: None,
        popup_xdg_popup: None,
        layer_configured: false,
        popup_configured: false,
        gen: 0,
        buf_serial: 0,
        bufs: [None, None],
        available: (0, 0),
        height: 0,
        hidden,
        run: true,
        want_show: false,
        want_hide: false,
        show_started: 0,
        preferred_scale: 1.0,
        preferred_fractional_scale: 0.0,
        active_touch: -1,
        cur: (-1, -1),
        cur_press: false,
        pen: Pen::default(),
    };
    app.height = app.opts.height;

    if let (Some(tm), Some(seat)) = (&tablet_mgr, &app.seat) {
        tm.get_tablet_seat(seat, &qh, ());
    }
    // a second round-trip for the seat capabilities
    if queue.roundtrip(&mut app).is_err() {
        die!("Wayland roundtrip failed\n");
    }

    app.empty_region = Some(app.compositor.create_region(&qh, ()));
    app.positioner = Some(app.wm_base.create_positioner(&qh, ()));
    let Some(seat) = app.seat.clone() else { die!("failed to init virtual keyboard_manager\n") };
    app.vkbd = Some(vkbd_mgr.create_virtual_keyboard(&seat, &qh, ()));

    app.with_out(|kb, o| kb.init(o));
    if let Some(im) = &im_mgr {
        im.get_input_method(&seat, &qh, ());
    }
    options::load_fonts(&mut app.kb);

    if !app.hidden {
        show(&mut app, &mut queue);
    }

    // signals arrive on a signalfd next to the Wayland socket
    let sigrtmin = libc::SIGRTMIN();
    let signal_fd = unsafe {
        let fd = libc::signalfd(-1, &mask, libc::SFD_CLOEXEC);
        if fd == -1 {
            die!("Failed to get signalfd\n");
        }
        OwnedFd::from_raw_fd(fd)
    };

    let mut warm = false;
    while app.run {
        if queue.dispatch_pending(&mut app).is_err() {
            die!("Wayland dispatch failed\n");
        }
        if app.want_hide {
            app.want_hide = false;
            app.hide();
        }
        if app.want_show {
            app.want_show = false;
            show(&mut app, &mut queue);
            continue;
        }
        app.present_all();
        if let Err(e) = conn.flush() {
            die!("Wayland socket has been disconnected: {e}\n");
        }
        // idle: everything is drawn and sent
        if !warm {
            warm = true;
            app.kb.warm_up();
            if app.hidden && app.kb.surf.is_none() {
                // hidden start: draw the keyboard now, showing only attaches
                refresh_available_dimension(&mut app, &mut queue);
                app.redimension();
                app.kb.scale = if app.preferred_fractional_scale != 0.0 { app.preferred_fractional_scale } else { app.preferred_scale };
                app.kb.resize();
                app.give_buffer(Which::Keys);
                app.give_buffer(Which::Popup);
            }
        }
        app.flush_hidden();
        app.kb.prepare_swipe();
        let timeout = if app.kb.longpress_at != 0 { (app.kb.longpress_at - monotime()).max(0) as i32 } else { -1 };
        let Some(guard) = queue.prepare_read() else { continue };
        let mut fds = [
            libc::pollfd { fd: guard.connection_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: signal_fd.as_raw_fd(), events: libc::POLLIN, revents: 0 },
        ];
        unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout) };
        if fds[0].revents & libc::POLLIN != 0 {
            if let Err(e) = guard.read() {
                if !matches!(e, wayland_client::backend::WaylandError::Io(ref io) if io.kind() == std::io::ErrorKind::WouldBlock) {
                    die!("Wayland socket has been disconnected: {e}\n");
                }
            }
        } else {
            drop(guard);
        }
        if queue.dispatch_pending(&mut app).is_err() {
            die!("Wayland dispatch failed\n");
        }
        // after dispatching, so a release that is already queued wins
        if app.kb.longpress_at != 0 && monotime() >= app.kb.longpress_at {
            app.kb.longpress();
        }
        if fds[0].revents & libc::POLLERR != 0 {
            die!("Exceptional condition on wayland socket.\n");
        }
        if fds[0].revents & libc::POLLHUP != 0 {
            die!("Wayland socket has been disconnected.\n");
        }
        if fds[1].revents & libc::POLLIN != 0 {
            let mut si: libc::signalfd_siginfo = unsafe { std::mem::zeroed() };
            let n = unsafe {
                libc::read(signal_fd.as_raw_fd(), &mut si as *mut _ as *mut libc::c_void, std::mem::size_of::<libc::signalfd_siginfo>())
            };
            if n as usize != std::mem::size_of::<libc::signalfd_siginfo>() {
                eprintln!("Signal read error");
                continue;
            }
            let sig = si.ssi_signo as i32;
            if sig == libc::SIGUSR1 {
                app.hide();
            } else if sig == libc::SIGUSR2 {
                show(&mut app, &mut queue);
            } else if sig == sigrtmin + 1 {
                app.kb.reset_text(); // focus moved to another field
            } else if sig == sigrtmin + 2 {
                app.kb.set_private(true); // password field
            } else if sig == sigrtmin + 3 {
                app.kb.set_private(false);
            } else if sig == sigrtmin {
                if app.hidden {
                    show(&mut app, &mut queue);
                } else {
                    app.hide();
                }
            } else if sig == libc::SIGPIPE {
                eprintln!("wvkbd: cannot pipe data out.");
            }
        }
    }
}

/* ---------------------------------------------------------------------- */
/* Wayland events                                                          */

impl Dispatch<WlRegistry, GlobalListContents> for App {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlSeat, ()> for App {
    fn event(app: &mut Self, seat: &WlSeat, event: wl_seat::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(caps) } = event {
            if caps.contains(wl_seat::Capability::Pointer) {
                if app.pointer.is_none() {
                    app.pointer = Some(seat.get_pointer(qh, ()));
                }
            } else if let Some(p) = app.pointer.take() {
                if p.version() >= 3 {
                    p.release();
                }
            }
            if caps.contains(wl_seat::Capability::Touch) {
                if app.touch.is_none() {
                    app.touch = Some(seat.get_touch(qh, ()));
                }
            } else if let Some(t) = app.touch.take() {
                if t.version() >= 3 {
                    t.release();
                }
            }
        }
    }
}

fn fixed_to_int(v: f64) -> i32 {
    // wl_fixed_to_int: truncation of the 24.8 fixed point value
    ((v * 256.0).round() as i32) / 256
}

impl Dispatch<WlPointer, ()> for App {
    fn event(app: &mut Self, _: &WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_pointer::Event::Enter { surface, .. } => {
                if app.kb.debug {
                    let which = if app.keys.as_ref().is_some_and(|s| s.surface == surface) {
                        "keyboard"
                    } else if app.popup.as_ref().is_some_and(|s| s.surface == surface) {
                        "popup"
                    } else {
                        "other"
                    };
                    eprintln!("pointer enter {which}");
                }
            }
            wl_pointer::Event::Leave { .. } => app.cur = (-1, -1),
            wl_pointer::Event::Motion { time, surface_x, surface_y } => {
                if !app.ready() {
                    return;
                }
                app.cur = (fixed_to_int(surface_x), fixed_to_int(surface_y));
                if app.cur_press {
                    let (x, y) = app.cur;
                    app.touch_motion(time, x, y);
                }
            }
            wl_pointer::Event::Button { time, state, .. } => {
                if !app.ready() {
                    return;
                }
                app.cur_press = state == WEnum::Value(wl_pointer::ButtonState::Pressed);
                if !app.cur_press {
                    app.touch_up(time);
                } else if app.cur.0 >= 0 && app.cur.1 >= 0 {
                    let (x, y) = app.cur;
                    app.touch_down(time, x, y);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlTouch, ()> for App {
    fn event(app: &mut Self, _: &WlTouch, event: wl_touch::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        // one finger at a time drives the keyboard; a second finger landing
        // first finishes the key under the previous one (two-thumb rollover)
        match event {
            wl_touch::Event::Down { time, id, x, y, .. } => {
                if !app.ready() {
                    return;
                }
                if app.active_touch != -1 {
                    app.touch_up(time);
                }
                app.active_touch = id;
                app.touch_down(time, fixed_to_int(x), fixed_to_int(y));
            }
            wl_touch::Event::Up { time, id, .. } => {
                if !app.ready() || id != app.active_touch {
                    return;
                }
                app.active_touch = -1;
                app.touch_up(time);
            }
            wl_touch::Event::Motion { time, id, x, y } => {
                if !app.ready() || id != app.active_touch {
                    return;
                }
                app.touch_motion(time, fixed_to_int(x), fixed_to_int(y));
            }
            wl_touch::Event::Cancel => {
                app.active_touch = -1;
                if app.ready() {
                    let t = app.time_now();
                    app.with_out(|kb, o| kb.touch_cancel(o, t));
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpTabletManagerV2, ()> for App {
    fn event(_: &mut Self, _: &ZwpTabletManagerV2, _: <ZwpTabletManagerV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletSeatV2, ()> for App {
    fn event(_: &mut Self, _: &ZwpTabletSeatV2, _: zwp_tablet_seat_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
    event_created_child!(App, ZwpTabletSeatV2, [
        zwp_tablet_seat_v2::EVT_TABLET_ADDED_OPCODE => (ZwpTabletV2, ()),
        zwp_tablet_seat_v2::EVT_TOOL_ADDED_OPCODE => (ZwpTabletToolV2, ()),
        zwp_tablet_seat_v2::EVT_PAD_ADDED_OPCODE => (ZwpTabletPadV2, ()),
    ]);
}

/// pen (tablet tool) input behaves like a finger
impl Dispatch<ZwpTabletToolV2, ()> for App {
    fn event(app: &mut Self, tool: &ZwpTabletToolV2, event: zwp_tablet_tool_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use zwp_tablet_tool_v2::Event;
        match event {
            Event::Removed => tool.destroy(),
            Event::ProximityIn { surface, .. } => {
                app.pen.on_keyboard = app.keys.as_ref().is_some_and(|s| s.surface == surface);
            }
            Event::ProximityOut => {
                if app.pen.down {
                    app.pen.pending_up = true;
                }
                app.pen.on_keyboard = false;
            }
            Event::Down { .. } => app.pen.pending_down = true,
            Event::Up => app.pen.pending_up = true,
            Event::Motion { x, y } => {
                app.pen.x = fixed_to_int(x);
                app.pen.y = fixed_to_int(y);
                app.pen.moved = true;
            }
            Event::Frame { time } => {
                let ready = app.ready();
                if app.pen.pending_down && app.pen.on_keyboard && ready {
                    app.pen.down = true;
                    let (x, y) = (app.pen.x, app.pen.y);
                    app.touch_down(time, x, y);
                } else if app.pen.moved && app.pen.down && ready {
                    let (x, y) = (app.pen.x, app.pen.y);
                    app.touch_motion(time, x, y);
                }
                if app.pen.pending_up && app.pen.down {
                    app.pen.down = false;
                    if ready {
                        app.touch_up(time);
                    }
                }
                app.pen.pending_down = false;
                app.pen.pending_up = false;
                app.pen.moved = false;
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpTabletV2, ()> for App {
    fn event(_: &mut Self, _: &ZwpTabletV2, _: <ZwpTabletV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletPadV2, ()> for App {
    fn event(_: &mut Self, _: &ZwpTabletPadV2, _: zwp_tablet_pad_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
    event_created_child!(App, ZwpTabletPadV2, [
        zwp_tablet_pad_v2::EVT_GROUP_OPCODE => (ZwpTabletPadGroupV2, ()),
    ]);
}

impl Dispatch<ZwpTabletPadGroupV2, ()> for App {
    fn event(_: &mut Self, _: &ZwpTabletPadGroupV2, _: zwp_tablet_pad_group_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
    event_created_child!(App, ZwpTabletPadGroupV2, [
        zwp_tablet_pad_group_v2::EVT_RING_OPCODE => (ZwpTabletPadRingV2, ()),
        zwp_tablet_pad_group_v2::EVT_STRIP_OPCODE => (ZwpTabletPadStripV2, ()),
        zwp_tablet_pad_group_v2::EVT_DIAL_OPCODE => (ZwpTabletPadDialV2, ()),
    ]);
}

impl Dispatch<ZwpTabletPadRingV2, ()> for App {
    fn event(_: &mut Self, _: &ZwpTabletPadRingV2, _: <ZwpTabletPadRingV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ZwpTabletPadStripV2, ()> for App {
    fn event(_: &mut Self, _: &ZwpTabletPadStripV2, _: <ZwpTabletPadStripV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ZwpTabletPadDialV2, ()> for App {
    fn event(_: &mut Self, _: &ZwpTabletPadDialV2, _: <ZwpTabletPadDialV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<XdgWmBase, ()> for App {
    fn event(_: &mut Self, wm: &XdgWmBase, event: xdg_wm_base::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm.pong(serial);
        }
    }
}

/// surfaces carry their generation; 0 for the probing surface
impl Dispatch<WlSurface, u32> for App {
    fn event(app: &mut Self, _: &WlSurface, event: wl_surface::Event, gen: &u32, _: &Connection, _: &QueueHandle<Self>) {
        if let wl_surface::Event::PreferredBufferScale { factor } = event {
            if app.keys.as_ref().is_some_and(|s| s.gen == *gen) {
                app.preferred_scale = factor as f64;
            }
        }
    }
}

impl Dispatch<WlCallback, (Which, u32)> for App {
    fn event(app: &mut Self, _: &WlCallback, event: wl_callback::Event, data: &(Which, u32), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_callback::Event::Done { .. } = event {
            let surf = match data.0 {
                Which::Keys => app.keys.as_mut(),
                Which::Popup => app.popup.as_mut(),
            };
            if let Some(s) = surf.filter(|s| s.gen == data.1) {
                s.frame_pending = false;
            }
        }
    }
}

impl Dispatch<WlBuffer, (Which, u64)> for App {
    fn event(app: &mut Self, _: &WlBuffer, event: wl_buffer::Event, data: &(Which, u64), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_buffer::Event::Release = event {
            if let Some(b) = app.bufs[data.0 as usize].as_mut().filter(|b| b.id == data.1) {
                b.busy = false;
                if app.kb.debug && app.hidden {
                    eprintln!("buffer {:?} released while hidden", data.0);
                }
            }
        }
    }
}

/// the layer surface: true for the probing one that measures the output
impl Dispatch<ZwlrLayerSurfaceV1, bool> for App {
    fn event(app: &mut Self, surface: &ZwlrLayerSurfaceV1, event: zwlr_layer_surface_v1::Event, probe: &bool, _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                if *probe {
                    app.available = (width, height);
                    surface.ack_configure(serial);
                } else if app.layer_surface.as_ref() != Some(surface) {
                    // swallow events for old/destroyed surfaces
                    surface.ack_configure(serial);
                } else {
                    app.configure(serial, width, height);
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                if !*probe && app.layer_surface.as_ref() == Some(surface) {
                    surface.destroy();
                    app.layer_surface = None;
                    if let Some(s) = app.keys.take() {
                        s.surface.destroy();
                    }
                    app.run = false;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<XdgSurface, u32> for App {
    fn event(app: &mut Self, xs: &XdgSurface, event: xdg_surface::Event, gen: &u32, _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_surface::Event::Configure { serial } = event {
            xs.ack_configure(serial);
            if app.popup.as_ref().is_some_and(|p| p.gen == *gen) {
                app.popup_configured = true;
                if let Some(p) = app.popup.as_mut() {
                    p.force = true;
                }
                app.present(Which::Popup);
            }
        }
    }
}

impl Dispatch<XdgPopup, ()> for App {
    fn event(_: &mut Self, _: &XdgPopup, _: xdg_popup::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WpFractionalScaleV1, ()> for App {
    fn event(app: &mut Self, _: &WpFractionalScaleV1, event: wp_fractional_scale_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            app.preferred_fractional_scale = scale as f64 / 120.0;
        }
    }
}

impl Dispatch<ZwpInputMethodV2, ()> for App {
    fn event(app: &mut Self, _: &ZwpInputMethodV2, event: zwp_input_method_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwp_input_method_v2::Event::Activate => app.want_show = true,
            zwp_input_method_v2::Event::Deactivate => app.hide(),
            _ => {}
        }
    }
}

macro_rules! no_events {
    ($($t:ty),*) => {$(
        impl Dispatch<$t, ()> for App {
            fn event(_: &mut Self, _: &$t, _: <$t as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
        }
    )*};
}

no_events!(
    WlCompositor,
    WlShm,
    WlShmPool,
    WlRegion,
    ZwlrLayerShellV1,
    XdgPositioner,
    WpFractionalScaleManagerV1,
    WpViewporter,
    WpViewport,
    ZwpVirtualKeyboardManagerV1,
    ZwpVirtualKeyboardV1,
    ZwpInputMethodManagerV2
);
