use smithay_client_toolkit::reexports::calloop::{
    EventLoop, RegistrationToken,
    channel::{self, Sender},
    timer::{TimeoutAction, Timer},
};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        xdg::{
            XdgShell, XdgSurface,
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use std::time::Duration;
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};
use xkeysym::Keysym;

use ab_glyph::{Font, PxScale, ScaleFont};
use tiny_skia::{PixmapMut, Transform};

mod window_switcher;
use window_switcher::WindowSwitcherState;

mod test_switcher;

fn main() {
    let mut event_loop: EventLoop<App> = EventLoop::try_new().unwrap();
    let loop_handle = event_loop.handle();
    let (sender, channel) = channel::channel();

    let conn = Connection::connect_to_env().unwrap();
    let (globals, mut event_queue) = registry_queue_init::<App>(&conn).unwrap();
    let qh = event_queue.handle();

    let shm = Shm::bind(&globals, &qh).expect("shm bind");
    let compositor = CompositorState::bind(&globals, &qh).expect("compositor bind");

    // Create OutputState early to detect outputs
    let output_state = OutputState::new(&globals, &qh);

    let surface = compositor.create_surface(&qh);

    let xdg_shell = XdgShell::bind(&globals, &qh).expect("xdg shell bind");
    let window = xdg_shell.create_window(surface, WindowDecorations::None, &qh);

    window.set_title("Gofi");
    window.set_app_id("gofi");

    let pool = SlotPool::new(1024 * 600 * 4, &shm).expect("Failed to create pool");

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state,
        shm,

        exit: false,
        first_configure: true,
        pool,
        width: 1024,
        height: 600,
        window,
        keyboard: None,
        pointer: None,
        window_switcher_state: WindowSwitcherState::new(),
        font: ab_glyph::FontVec::try_from_vec(
            std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").expect("font load"),
        )
        .expect("font parse"),
        qh: qh.clone(),
        current_key_repeat: None,
        repeat_rate: 25,
        repeat_delay: 600,
        repeat_sender: sender,
    };

    // Roundtrip
    event_queue.roundtrip(&mut app).unwrap();
    event_queue.roundtrip(&mut app).unwrap();

    let mut target_width = 1024;
    let mut target_height = 600;

    if let Some(output) = app.output_state.outputs().next() {
        if let Some(info) = app.output_state.info(&output) {
            if let Some(mode) = info.logical_size {
                target_width = (mode.0 as f32 * 0.8) as u32;
                target_height = (mode.1 as f32 * 0.3) as u32;
            } else if let Some(mode) = info.modes.iter().find(|m| m.current || m.preferred) {
                target_width = (mode.dimensions.0 as f32 * 0.8) as u32;
                target_height = (mode.dimensions.1 as f32 * 0.3) as u32;
            }
        }
    }

    app.width = target_width;
    app.height = target_height;
    app.window
        .xdg_toplevel()
        .set_min_size(target_width as i32, target_height as i32);

    app.pool
        .resize((target_width * target_height * 4) as usize)
        .expect("resize pool");

    app.window_switcher_state.refresh();
    app.draw(&qh);

    println!("Gofi started. Press ESC to exit.");

    WaylandSource::new(conn.clone(), event_queue)
        .insert(loop_handle.clone())
        .unwrap();

    // Setup channel for key repeat
    let loop_handle_clone = loop_handle.clone();
    loop_handle
        .insert_source(channel, move |event, _, app| match event {
            channel::Event::Msg(RepeatCommand::Start {
                keysym,
                utf8,
                delay,
                rate,
            }) => {
                if let Some(rep) = app.current_key_repeat.take() {
                    loop_handle_clone.remove(rep.token);
                }

                let sym = keysym;
                let text = utf8.clone();
                let qh = app.qh.clone();

                let delay_dur = Duration::from_millis(delay as u64);

                let token = loop_handle_clone
                    .insert_source(Timer::from_duration(delay_dur), move |_, _, app| {
                        app.handle_key_action(&qh, sym, text.clone());
                        let period = if rate > 0 { 1000 / rate as u64 } else { 200 };
                        TimeoutAction::ToDuration(Duration::from_millis(period))
                    })
                    .unwrap();

                app.current_key_repeat = Some(KeyRepeat { keysym, token });
            }
            channel::Event::Msg(RepeatCommand::Stop { keysym }) => {
                if let Some(rep) = &app.current_key_repeat {
                    if rep.keysym == keysym {
                        loop_handle_clone.remove(rep.token);
                        app.current_key_repeat = None;
                    }
                }
            }
            channel::Event::Closed => {}
        })
        .unwrap();

    loop {
        event_loop
            .dispatch(Duration::from_millis(16), &mut app)
            .unwrap();
        if app.exit {
            break;
        }
    }
}

struct App {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,

    exit: bool,
    first_configure: bool,
    pool: SlotPool,
    width: u32,
    height: u32,
    window: Window,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    window_switcher_state: WindowSwitcherState,
    font: ab_glyph::FontVec,

    qh: QueueHandle<App>,
    current_key_repeat: Option<KeyRepeat>,
    repeat_rate: i32,
    repeat_delay: i32,
    repeat_sender: Sender<RepeatCommand>,
}

#[derive(Debug)]
enum RepeatCommand {
    Start {
        keysym: Keysym,
        utf8: Option<String>,
        delay: u32,
        rate: u32,
    },
    Stop {
        keysym: Keysym,
    },
}

struct KeyRepeat {
    keysym: Keysym,
    token: RegistrationToken,
}

impl App {
    pub fn draw(&mut self, _qh: &QueueHandle<Self>) {
        let width = self.width;
        let height = self.height;
        let stride = width as i32 * 4;
        let (buffer, canvas) = self
            .pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .expect("create buffer");

        // Fill with transparent background
        for byte in canvas.iter_mut() {
            *byte = 0;
        }

        if let Some(mut pixmap) = PixmapMut::from_bytes(canvas, width, height) {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(30, 30, 30, 200);
            pixmap.fill_rect(
                tiny_skia::Rect::from_xywh(0.0, 0.0, width as f32, height as f32).unwrap(),
                &paint,
                Transform::identity(),
                None,
            );

            // Draw search query
            draw_text_pixel(
                &mut pixmap,
                &self.font,
                10.0,
                10.0,
                &format!("Search: {}", self.window_switcher_state.query),
                tiny_skia::Color::WHITE,
                &[], // No highlight for search bar
            );

            let selection = self.window_switcher_state.selection_index;
            // Draw below search bar
            let mut y = 50.0;

            // Calculate max items that fit in the view
            // Available height is roughly height - 50.0 (top) - 30.0 (bottom margin logic)
            let item_height = 28.0;
            let available_height = (height as f32 - 80.0).max(0.0);
            let max_items = (available_height / item_height).floor() as usize;

            self.window_switcher_state.ensure_visible(max_items);

            // Iterate over filtered windows
            for (i, (win_idx, indices)) in self
                .window_switcher_state
                .filtered_windows
                .iter()
                .enumerate()
                .skip(self.window_switcher_state.scroll_offset)
            {
                // Limit number of items drawn to fit screen
                if y > height as f32 - 30.0 {
                    break;
                }

                let win = &self.window_switcher_state.windows[*win_idx];

                let is_selected = i == selection;
                let rect_color = if is_selected {
                    tiny_skia::Color::from_rgba8(60, 100, 160, 255)
                } else {
                    tiny_skia::Color::from_rgba8(50, 50, 50, 255)
                };

                if let Some(rect) = tiny_skia::Rect::from_xywh(10.0, y, width as f32 - 20.0, 24.0) {
                    let mut paint = tiny_skia::Paint::default();
                    paint.set_color(rect_color);
                    pixmap.fill_rect(rect, &paint, Transform::identity(), None);

                    // Draw text
                    let title = format!("{} - {}", win.wm_class, win.title); // Show app name + title
                    draw_text_pixel(
                        &mut pixmap,
                        &self.font,
                        20.0,
                        y + 2.0, // Center vertically roughly
                        &title,  // Use combined title
                        tiny_skia::Color::WHITE,
                        indices,
                    );
                }
                y += 28.0;
            }
        }

        self.window
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);
        buffer
            .attach_to(self.window.wl_surface())
            .expect("buffer attach");
        self.window
            .xdg_surface()
            .set_window_geometry(0, 0, width as i32, height as i32);
        self.window.commit();
    }

    fn handle_key_action(&mut self, qh: &QueueHandle<Self>, keysym: Keysym, utf8: Option<String>) {
        use xkeysym::Keysym;

        if keysym == Keysym::Escape {
            self.exit = true;
        } else if keysym == Keysym::Down {
            self.window_switcher_state.next();
        } else if keysym == Keysym::Up {
            self.window_switcher_state.prev();
        } else if keysym == Keysym::Return {
            self.window_switcher_state.activate();
            self.exit = true;
        } else if keysym == Keysym::BackSpace {
            self.window_switcher_state.backspace();
        } else {
            if let Some(text) = utf8 {
                if !text.chars().any(|c| c.is_control()) {
                    self.window_switcher_state.input_text(&text);
                }
            }
        }
        self.draw(qh);
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl WindowHandler for App {
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        if let (Some(w), Some(h)) = configure.new_size {
            self.width = w.get();
            self.height = h.get();
        } else {
            // Keep current size (calculated dynamically)
        }

        if self.first_configure {
            self.first_configure = false;
            // self.draw(qh);
        }
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let keyboard = self
                .seat_state
                .get_keyboard(qh, &self.seat_state.seats().next().unwrap(), None)
                .expect("Failed to get keyboard");
            self.keyboard = Some(keyboard);
        }

        if capability == Capability::Pointer && self.pointer.is_none() {
            let pointer = self
                .seat_state
                .get_pointer(qh, &self.seat_state.seats().next().unwrap())
                .expect("Failed to get pointer");
            self.pointer = Some(pointer);
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            self.keyboard.take();
        }
        if capability == Capability::Pointer {
            self.pointer.take();
        }
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}

    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {
    }
}

impl KeyboardHandler for App {
    fn press_key(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        use xkeysym::Keysym;

        let sym = event.keysym;
        let utf8 = event.utf8;

        // Cancel previous repeat if new press
        self.repeat_sender
            .send(RepeatCommand::Stop { keysym: sym })
            .ok();

        // Execute action immediately
        self.handle_key_action(qh, sym, utf8.clone());

        // Schedule repeat if it is a repeatable key
        let should_repeat = matches!(sym, Keysym::BackSpace | Keysym::Down | Keysym::Up)
            || (utf8.is_some() && !utf8.as_ref().unwrap().chars().any(|c| c.is_control()));

        if should_repeat {
            self.repeat_sender
                .send(RepeatCommand::Start {
                    keysym: sym,
                    utf8,
                    delay: self.repeat_delay as u32,
                    rate: self.repeat_rate as u32,
                })
                .ok();
        }
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        self.repeat_sender
            .send(RepeatCommand::Stop {
                keysym: event.keysym,
            })
            .ok();
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _modifiers: Modifiers,
        _group: u32,
    ) {
    }

    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[xkeysym::Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
        // Stop all repeats? We can just send stop for current if any.
        // But we don't know current here easily without saving it.
        // Actually we can just not send anything or assume logic handles it?
        // Optimally we should stop.
        // But RepeatCommand::Stop requires keysym.
        // We can add StopAll.
        // For now ignore.
    }

    fn update_repeat_info(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        info: smithay_client_toolkit::seat::keyboard::RepeatInfo,
    ) {
        match info {
            smithay_client_toolkit::seat::keyboard::RepeatInfo::Repeat { rate, delay } => {
                self.repeat_rate = rate.get() as i32;
                self.repeat_delay = delay as i32;
            }
            smithay_client_toolkit::seat::keyboard::RepeatInfo::Disable => {
                self.repeat_rate = 0;
                self.repeat_delay = 0;
            }
        }
    }
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if let PointerEventKind::Press { .. } = event.kind {
                // Handle clicks?
            }
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

fn draw_text_pixel(
    pixmap: &mut PixmapMut,
    font: &ab_glyph::FontVec,
    x: f32,
    y: f32,
    text: &str,
    color: tiny_skia::Color,
    highlights: &[usize], // Add mismatch here
) {
    let scale = PxScale::from(18.0);
    let scaled_font = font.as_scaled(scale);
    let mut pen_x = x;
    let pen_y = y + 18.0; // Baseline

    // Convert color to u8 components (ARGB)
    // Default color
    let r_def = (color.red() * 255.0) as u8;
    let g_def = (color.green() * 255.0) as u8;
    let b_def = (color.blue() * 255.0) as u8;

    // Highlight color (Greenish Cyan: 0, 255, 255)
    let r_hl = 0;
    let g_hl = 255;
    let b_hl = 255;

    for (char_idx, c) in text.chars().enumerate() {
        if c.is_control() {
            continue;
        }

        let is_highlight = highlights.contains(&char_idx);
        let (r, g, b) = if is_highlight {
            (r_hl, g_hl, b_hl)
        } else {
            (r_def, g_def, b_def)
        };

        let glyph_id = scaled_font.glyph_id(c);
        let glyph = glyph_id.with_scale_and_position(scale, ab_glyph::point(pen_x, pen_y));

        if let Some(outlined) = scaled_font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|px, py, v| {
                let px = bounds.min.x as i32 + px as i32;
                let py = bounds.min.y as i32 + py as i32;
                if px >= 0 && px < pixmap.width() as i32 && py >= 0 && py < pixmap.height() as i32 {
                    let idx = (py as usize * pixmap.width() as usize + px as usize) * 4;
                    let pixel = &mut pixmap.data_mut()[idx..idx + 4];

                    // Alpha blending
                    // v is coverage (0.0 - 1.0)
                    if v > 0.01 {
                        let src_a = (v * 255.0) as u16;
                        let inv_a = 255 - src_a;

                        // pixel layout: [Blue, Green, Red, Alpha]
                        pixel[0] = ((b as u16 * src_a + pixel[0] as u16 * inv_a) / 255) as u8;
                        pixel[1] = ((g as u16 * src_a + pixel[1] as u16 * inv_a) / 255) as u8;
                        pixel[2] = ((r as u16 * src_a + pixel[2] as u16 * inv_a) / 255) as u8;
                        pixel[3] = ((255 * src_a + pixel[3] as u16 * inv_a) / 255) as u8;
                    }
                }
            });
        }
        pen_x += scaled_font.h_advance(glyph_id);
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_keyboard!(App);
delegate_pointer!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
delegate_registry!(App);
