use std::process::Command;
use x11rb::connection::Connection;
use x11rb::protocol::{xproto::*, Event};
use x11rb::rust_connection::RustConnection;

const MOD: ModMask = ModMask::M4; // Super (Win)
const BW: u32 = 2;
const GAP: i32 = 8;

#[derive(Clone, Copy)]
struct Client {
    win: Window,
    ws: usize,
    hidden_by_ws: bool,
}

struct Wm {
    conn: RustConnection,
    root: Window,
    sw: u16,
    sh: u16,
    cur_ws: usize,
    clients: Vec<Client>,
    focused: Option<Window>,
    wm_protocols: Atom,
    wm_delete: Atom,
    drag: Option<(Window, u8, i16, i16, i16, i16, u16, u16)>,
}

impl Wm {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (conn, scr) = RustConnection::connect(None)?;
        let s = &conn.setup().roots[scr];
        let (root, sw, sh, depth) = (s.root, s.width_in_pixels, s.height_in_pixels, s.root_depth);

        conn.change_window_attributes(root, &ChangeWindowAttributesAux::new().event_mask(
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY | EventMask::BUTTON_PRESS | EventMask::KEY_PRESS
        ))?.check()?;

        let wm_protocols = conn.intern_atom(false, b"WM_PROTOCOLS")?.reply()?.atom;
        let wm_delete = conn.intern_atom(false, b"WM_DELETE_WINDOW")?.reply()?.atom;

        let mut wm = Self { conn, root, sw, sh, cur_ws: 0, clients: Vec::new(), focused: None, wm_protocols, wm_delete, drag: None };
        wm.draw_wallpaper(depth)?;
        wm.grab_keys()?;
        wm.scan_existing()?;
        wm.conn.flush()?;
        Ok(wm)
    }

    fn draw_wallpaper(&self, depth: u8) -> Result<(), Box<dyn std::error::Error>> {
        let (w, h) = (self.sw as usize, self.sh as usize);
        let pix = self.conn.generate_id()?;
        let gc = self.conn.generate_id()?;
        self.conn.create_pixmap(depth, pix, self.root, self.sw, self.sh)?;
        self.conn.create_gc(gc, pix, &CreateGCAux::new())?;

        let mut pixels = vec![0u32; w * h];
        for y in 0..h {
            let t = (y * 255 / h.max(1)) as u32;
            let r = 0x16 + (t * 0x09 / 255);
            let g = 0x16 + (t * 0x0d / 255);
            let b = 0x1e + (t * 0x17 / 255);
            let base = (r << 16) | (g << 8) | b;
            let grid = ((r + 8) << 16) | ((g + 10) << 8) | (b + 16);
            let row_line = y % 48 == 0;
            for x in 0..w {
                pixels[y * w + x] = if row_line || x % 48 == 0 { grid } else { base };
            }
        }
        let chunk_rows = 64usize;
        for y0 in (0..h).step_by(chunk_rows) {
            let rows = (h - y0).min(chunk_rows);
            let slice = &pixels[y0 * w..(y0 + rows) * w];
            let raw: &[u8] = unsafe { std::slice::from_raw_parts(slice.as_ptr() as *const u8, slice.len() * 4) };
            self.conn.put_image(ImageFormat::Z_PIXMAP, pix, gc, self.sw, rows as u16, 0, y0 as i16, 0, depth, raw)?;
        }
        self.conn.change_window_attributes(self.root, &ChangeWindowAttributesAux::new().background_pixmap(pix))?;
        self.conn.clear_area(false, self.root, 0, 0, 0, 0)?;
        self.conn.free_gc(gc)?;
        self.conn.free_pixmap(pix)?;
        Ok(())
    }

    fn scan_existing(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let children = self.conn.query_tree(self.root)?.reply()?.children;
        for w in children {
            let ok = self.conn.get_window_attributes(w)?.reply()
                .map(|a| !a.override_redirect && a.map_state == MapState::VIEWABLE)
                .unwrap_or(false);
            if ok { self.manage(w)?; }
        }
        Ok(())
    }

    fn manage(&mut self, w: Window) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(attr) = self.conn.get_window_attributes(w)?.reply() {
            if attr.override_redirect {
                self.conn.map_window(w)?;
                return Ok(());
            }
        }
        self.conn.configure_window(w, &ConfigureWindowAux::new().border_width(BW))?;
        self.conn.change_window_attributes(w, &ChangeWindowAttributesAux::new().event_mask(EventMask::ENTER_WINDOW))?;
        if !self.clients.iter().any(|c| c.win == w) {
            self.clients.push(Client { win: w, ws: self.cur_ws, hidden_by_ws: false });
        }
        self.conn.map_window(w)?;
        self.focus(w)?;
        Ok(())
    }

    fn unmanage(&mut self, w: Window) {
        self.clients.retain(|c| c.win != w);
        if self.focused == Some(w) {
            self.focused = self.clients.iter().rev().find(|c| c.ws == self.cur_ws).map(|c| c.win);
            if let Some(next) = self.focused {
                let _ = self.focus(next);
            } else {
                let _ = self.conn.set_input_focus(InputFocus::POINTER_ROOT, self.root, x11rb::CURRENT_TIME);
            }
        }
    }

    fn grab_keys(&self) -> Result<(), Box<dyn std::error::Error>> {
        for kc in [36u8, 40, 24, 23, 28, 41, 10, 11, 12, 13, 14, 15] {
            self.conn.grab_key(false, self.root, MOD, kc, GrabMode::ASYNC, GrabMode::ASYNC)?;
        }
        for kc in [26u8, 10, 11, 12, 13, 14, 15] {
            self.conn.grab_key(false, self.root, MOD | ModMask::SHIFT, kc, GrabMode::ASYNC, GrabMode::ASYNC)?;
        }
        for btn in [ButtonIndex::M1, ButtonIndex::M3] {
            self.conn.grab_button(false, self.root, EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
                GrabMode::ASYNC, GrabMode::ASYNC, x11rb::NONE, x11rb::NONE, btn, MOD)?;
        }
        Ok(())
    }

    fn focus(&mut self, win: Window) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(prev) = self.focused {
            if prev != win && self.clients.iter().any(|c| c.win == prev) {
                let _ = self.conn.change_window_attributes(prev, &ChangeWindowAttributesAux::new().border_pixel(0x2f343f));
            }
        }
        self.conn.change_window_attributes(win, &ChangeWindowAttributesAux::new().border_pixel(0x7aa2f7))?;
        self.conn.configure_window(win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        self.conn.set_input_focus(InputFocus::POINTER_ROOT, win, x11rb::CURRENT_TIME)?;
        self.focused = Some(win);
        Ok(())
    }

    fn switch_ws(&mut self, target: usize) -> Result<(), Box<dyn std::error::Error>> {
        if target == self.cur_ws { return Ok(()); }
        self.cur_ws = target;
        let mut last_vis = None;
        for c in &mut self.clients {
            if c.ws == self.cur_ws {
                c.hidden_by_ws = false;
                self.conn.map_window(c.win)?;
                last_vis = Some(c.win);
            } else {
                c.hidden_by_ws = true;
                self.conn.unmap_window(c.win)?;
            }
        }
        if let Some(w) = last_vis {
            self.focus(w)?;
        } else {
            self.focused = None;
            self.conn.set_input_focus(InputFocus::POINTER_ROOT, self.root, x11rb::CURRENT_TIME)?;
        }
        Ok(())
    }

    fn tile_ws(&self) -> Result<(), Box<dyn std::error::Error>> {
        let vis: Vec<Window> = self.clients.iter().filter(|c| c.ws == self.cur_ws).map(|c| c.win).collect();
        let n = vis.len() as i32;
        if n == 0 { return Ok(()); }
        let (sw, sh, bw) = (self.sw as i32, self.sh as i32, BW as i32);
        if n == 1 {
            self.conn.configure_window(vis[0], &ConfigureWindowAux::new()
                .x(GAP).y(GAP).width((sw - 2 * (GAP + bw)) as u32).height((sh - 2 * (GAP + bw)) as u32))?;
            return Ok(());
        }
        let mw = sw / 2;
        self.conn.configure_window(vis[0], &ConfigureWindowAux::new()
            .x(GAP).y(GAP).width((mw - GAP - GAP / 2 - 2 * bw) as u32).height((sh - 2 * (GAP + bw)) as u32))?;
        let stack_n = n - 1;
        let step_h = (sh - GAP) / stack_n;
        for (i, &w) in vis[1..].iter().enumerate() {
            let y = GAP + (i as i32) * step_h;
            let h = step_h - GAP - 2 * bw;
            self.conn.configure_window(w, &ConfigureWindowAux::new()
                .x(mw + GAP / 2).y(y).width((sw - mw - GAP - GAP / 2 - 2 * bw) as u32).height(h.max(40) as u32))?;
        }
        Ok(())
    }

    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            match self.conn.wait_for_event()? {
                Event::MapRequest(e) => self.manage(e.window)?,
                Event::ConfigureRequest(e) => {
                    let mut aux = ConfigureWindowAux::new();
                    if e.value_mask.contains(ConfigWindow::X) { aux = aux.x(e.x as i32); }
                    if e.value_mask.contains(ConfigWindow::Y) { aux = aux.y(e.y as i32); }
                    if e.value_mask.contains(ConfigWindow::WIDTH) { aux = aux.width(e.width as u32); }
                    if e.value_mask.contains(ConfigWindow::HEIGHT) { aux = aux.height(e.height as u32); }
                    self.conn.configure_window(e.window, &aux)?;
                }
                Event::EnterNotify(e) => {
                    if self.clients.iter().any(|c| c.win == e.event && c.ws == self.cur_ws) {
                        self.focus(e.event)?;
                    }
                }
                Event::UnmapNotify(e) => {
                    if let Some(c) = self.clients.iter().find(|c| c.win == e.window) {
                        if !c.hidden_by_ws {
                            self.unmanage(e.window);
                        }
                    }
                }
                Event::DestroyNotify(e) => self.unmanage(e.window),
                Event::KeyPress(e) => {
                    let shift = e.state.contains(KeyButMask::SHIFT);
                    match e.detail {
                        36 => { let _ = Command::new("TixTerm").spawn(); }
                        40 => { let _ = Command::new("TixMenu").spawn(); }
                        24 => if let Some(w) = self.focused {
let msg = ClientMessageEvent::new(32, w, self.wm_protocols, [self.wm_delete, x11rb::CURRENT_TIME, 0, 0, 0]);
let _ = self.conn.send_event(false, w, EventMask::NO_EVENT, msg);
},
23 => {
let vis: Vec<Window> = self.clients.iter().filter(|c| c.ws == self.cur_ws).map(|c| c.win).collect();
if vis.len() > 1 {
let idx = self.focused.and_then(|f| vis.iter().position(|&w| w == f)).unwrap_or(0);
self.focus(vis[(idx + 1) % vis.len()])?;
}
}
28 => self.tile_ws()?,
41 => if let Some(w) = self.focused {
self.conn.configure_window(w, &ConfigureWindowAux::new().x(0).y(0)
.width((self.sw as u32).saturating_sub(2 * BW)).height((self.sh as u32).saturating_sub(2 * BW)))?;
},
10..=15 => {
let target = (e.detail - 10) as usize;
if shift {
if let Some(w) = self.focused {
if let Some(c) = self.clients.iter_mut().find(|c| c.win == w) {
c.ws = target;
c.hidden_by_ws = target != self.cur_ws;
}
if target != self.cur_ws {
self.conn.unmap_window(w)?;
self.focused = self.clients.iter().rev().find(|c| c.ws == self.cur_ws).map(|c| c.win);
if let Some(next) = self.focused { self.focus(next)?; }
}
}
} else {
self.switch_ws(target)?;
}
}
26 if shift => break,
_ => {}
}
}
Event::ButtonPress(e) => {
let w = if e.child != x11rb::NONE { e.child } else { e.event };
if self.clients.iter().any(|c| c.win == w && c.ws == self.cur_ws) {
self.focus(w)?;
if let Ok(g) = self.conn.get_geometry(w)?.reply() {
self.drag = Some((w, e.detail, e.root_x, e.root_y, g.x, g.y, g.width, g.height));
}
}
}
Event::MotionNotify(e) => if let Some((w, btn, rx, ry, wx, wy, ww, wh)) = self.drag {
let (dx, dy) = (e.root_x - rx, e.root_y - ry);
if btn == 1 {
self.conn.configure_window(w, &ConfigureWindowAux::new().x((wx + dx) as i32).y((wy + dy) as i32))?;
} else if btn == 3 {
self.conn.configure_window(w, &ConfigureWindowAux::new()
.width((ww as i16 + dx).max(120) as u32).height((wh as i16 + dy).max(80) as u32))?;
}
},
Event::ButtonRelease(_) => self.drag = None,
_ => {}
}
self.conn.flush()?;
}
Ok(())
}
}
fn main() {
if let Ok(mut wm) = Wm::new() { let _ = wm.run(); }
}
