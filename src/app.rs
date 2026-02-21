use smithay_client_toolkit::reexports::calloop::channel::Sender;
use smithay_client_toolkit::{
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::{
        WaylandSurface,
        xdg::{XdgSurface, window::Window},
    },
    shm::{Shm, slot::SlotPool},
};
use tiny_skia::{PixmapMut, Transform};
use wayland_client::{
    QueueHandle,
    protocol::{wl_keyboard, wl_pointer, wl_shm},
};
use xkeysym::Keysym;

use crate::key_repeat::{KeyRepeat, RepeatCommand};
use crate::rendering::draw_text_pixel;
use crate::window_switcher::WindowSwitcherState;

pub(crate) struct App {
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
    pub fonts: Vec<ab_glyph::FontVec>,

    pub qh: QueueHandle<App>,
    pub current_key_repeat: Option<KeyRepeat>,
    pub repeat_rate: i32,
    pub repeat_delay: i32,
    pub repeat_sender: Sender<RepeatCommand>,
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
                &self.fonts,
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
                        &self.fonts,
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

    pub(crate) fn handle_key_action(
        &mut self,
        qh: &QueueHandle<Self>,
        keysym: Keysym,
        utf8: Option<String>,
    ) {
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
        } else if let Some(text) = utf8
            && !text.chars().any(|c| c.is_control())
        {
            self.window_switcher_state.input_text(&text);
        }
        self.draw(qh);
    }
}
