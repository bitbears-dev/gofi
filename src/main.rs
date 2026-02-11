mod window_switcher;
use ab_glyph::{Font, PxScale, ScaleFont};
use window_switcher::WindowSwitcherState;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{PointerEvent, PointerHandler},
    },
    shell::{
        WaylandSurface,
        xdg::{
            XdgShell,
            window::{Window, WindowConfigure, WindowHandler},
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};

fn main() {
    env_logger::init();
    let conn = Connection::connect_to_env().unwrap();
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();
    println!("Gofi: Registry initialized. enumerating globals...");

    // プロトコルのバインド
    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor is not available");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("xdg_shell is not available");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm is not available");

    // ウィンドウ（Window）の作成
    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(
        surface,
        smithay_client_toolkit::shell::xdg::window::WindowDecorations::None,
        &qh,
    );

    // ウィンドウタイトルの設定（Gnomeのトップバーなどに表示される場合があります）
    window.set_title("Gofi (Rofi Alternative)");
    // アプリケーションIDの設定
    window.set_app_id("gofi");
    // 初期コミット
    window.commit();

    let pool = SlotPool::new(600 * 300 * 4, &shm).expect("Failed to create pool");

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,

        exit: false,
        first_configure: true,
        pool,
        width: 600,
        height: 300,
        window,
        keyboard: None,
        pointer: None,
        window_switcher_state: WindowSwitcherState::new(),
        font: ab_glyph::FontVec::try_from_vec(
            std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").expect("font load"),
        )
        .expect("font parse"),
    };

    // Initial refresh of windows
    app.window_switcher_state.refresh();

    println!("Gofi started. Press ESC to exit.");

    while !app.exit {
        event_queue.blocking_dispatch(&mut app).unwrap();
    }
}

pub struct App {
    pub registry_state: RegistryState,
    pub seat_state: SeatState,
    pub output_state: OutputState,
    pub shm: Shm,

    pub exit: bool,
    pub first_configure: bool,
    pub pool: SlotPool,
    pub width: u32,
    pub height: u32,
    pub window: Window,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,
    pub pointer: Option<wl_pointer::WlPointer>,
    pub window_switcher_state: WindowSwitcherState,
    pub font: ab_glyph::FontVec,
}

impl AsMut<WindowSwitcherState> for App {
    fn as_mut(&mut self) -> &mut WindowSwitcherState {
        &mut self.window_switcher_state
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.draw(qh);
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

// WindowHandlerの実装 (xdg-shell用)
impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        if let (Some(w), Some(h)) = configure.new_size {
            self.width = w.get();
            self.height = h.get();
        } else {
            // デフォルトサイズ
            self.width = 600;
            self.height = 300;
        }

        if self.first_configure {
            self.first_configure = false;
            self.draw(qh);
        }
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = Some(
                self.seat_state
                    .get_keyboard(qh, &seat, None)
                    .expect("Failed to get keyboard"),
            );
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            self.pointer = Some(
                self.seat_state
                    .get_pointer(qh, &seat)
                    .expect("Failed to get pointer"),
            );
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Keyboard {
            self.keyboard.take().unwrap().release();
        }
        if cap == Capability::Pointer {
            self.pointer.take().unwrap().release();
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }
    fn press_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        let escape_keysym = Keysym::from(xkeysym::key::Escape);
        let up_keysym = Keysym::from(xkeysym::key::Up);
        let down_keysym = Keysym::from(xkeysym::key::Down);
        let enter_keysym = Keysym::from(xkeysym::key::Return);

        if event.keysym == escape_keysym {
            self.exit = true;
        } else if event.keysym == up_keysym {
            self.window_switcher_state.prev();
            self.draw(qh); // Redraw on change
        } else if event.keysym == down_keysym {
            self.window_switcher_state.next();
            self.draw(qh);
        } else if event.keysym == enter_keysym {
            self.window_switcher_state.activate();
            self.exit = true;
        }
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: Modifiers,
        _: u32,
    ) {
    }
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface != self.window.wl_surface() {
                continue;
            }
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
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

        if let Some(mut pixmap) = tiny_skia::PixmapMut::from_bytes(canvas, width, height) {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(30, 30, 30, 200);
            pixmap.fill_rect(
                tiny_skia::Rect::from_xywh(0.0, 0.0, width as f32, height as f32).unwrap(),
                &paint,
                tiny_skia::Transform::identity(),
                None,
            );

            let windows = &self.window_switcher_state.windows;
            let selection = self.window_switcher_state.selection_index;
            let mut y = 10.0;

            for (i, win) in windows.iter().enumerate() {
                let is_selected = i == selection;
                let rect_color = if is_selected {
                    tiny_skia::Color::from_rgba8(60, 100, 160, 255)
                } else {
                    tiny_skia::Color::from_rgba8(50, 50, 50, 255)
                };

                if let Some(rect) = tiny_skia::Rect::from_xywh(10.0, y, width as f32 - 20.0, 30.0) {
                    let mut paint = tiny_skia::Paint::default();
                    paint.set_color(rect_color);
                    pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);

                    // Draw text
                    let title = format!("{} - {}", win.wm_class, win.title); // Show app name + title
                    draw_text_pixel(
                        &mut pixmap,
                        &self.font,
                        20.0,
                        y,
                        &title, // Use combined title
                        tiny_skia::Color::WHITE,
                    );
                }
                y += 35.0;
            }
        }

        self.window
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);
        buffer
            .attach_to(self.window.wl_surface())
            .expect("buffer attach");
        self.window.commit();
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_keyboard!(App);
delegate_pointer!(App);
delegate_xdg_shell!(App); // xdg-shell デリゲートを追加
delegate_xdg_window!(App); // xdg-window デリゲートを追加
delegate_registry!(App);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState,];
}

fn draw_text_pixel(
    pixmap: &mut tiny_skia::PixmapMut,
    font: &ab_glyph::FontVec,
    x: f32,
    y: f32,
    text: &str,
    color: tiny_skia::Color,
) {
    let scale = PxScale::from(24.0);
    let scaled_font = font.as_scaled(scale);
    let mut pen_x = x;
    let pen_y = y + 24.0; // Baseline

    // Convert color to u8 components (ARGB)
    let r = (color.red() * 255.0) as u8;
    let g = (color.green() * 255.0) as u8;
    let b = (color.blue() * 255.0) as u8;
    let a = (color.alpha() * 255.0) as u8;

    for c in text.chars() {
        if c.is_control() {
            continue;
        }
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

                    if v > 0.5 {
                        pixel[0] = b; // Blue
                        pixel[1] = g; // Green
                        pixel[2] = r; // Red
                        pixel[3] = a; // Alpha
                    }
                }
            });
        }
        pen_x += scaled_font.h_advance(glyph_id);
    }
}
