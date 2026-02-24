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
use crate::rendering::{TextDrawParams, draw_text_pixel};
use crate::theme;
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
            let (r, g, b, a) = theme::BG_COLOR;
            paint.set_color_rgba8(r, g, b, a);
            pixmap.fill_rect(
                tiny_skia::Rect::from_xywh(0.0, 0.0, width as f32, height as f32).unwrap(),
                &paint,
                Transform::identity(),
                None,
            );

            // Draw search query
            let search_text = format!("Search: {}", self.window_switcher_state.query);
            draw_text_pixel(
                &mut pixmap,
                TextDrawParams {
                    fonts: &self.fonts,
                    x: theme::PADDING,
                    y: theme::PADDING,
                    text: &search_text,
                    color: tiny_skia::Color::WHITE,
                    highlights: &[],
                },
            );

            let selection = self.window_switcher_state.selection_index;
            // Draw below search bar
            let mut y = theme::SEARCH_BAR_Y;

            // Calculate max items that fit in the view
            let item_height = theme::ITEM_HEIGHT;
            let available_height = (height as f32 - theme::TOP_BOTTOM_MARGIN).max(0.0);
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
                if y > height as f32 - theme::BOTTOM_MARGIN {
                    break;
                }

                let win = &self.window_switcher_state.windows[*win_idx];

                let is_selected = i == selection;
                let rect_color = {
                    let (r, g, b, a) = if is_selected {
                        theme::SELECTED_COLOR
                    } else {
                        theme::ITEM_COLOR
                    };
                    tiny_skia::Color::from_rgba8(r, g, b, a)
                };

                if let Some(rect) = tiny_skia::Rect::from_xywh(
                    theme::PADDING,
                    y,
                    width as f32 - theme::PADDING * 2.0,
                    theme::ITEM_RECT_HEIGHT,
                ) {
                    let mut paint = tiny_skia::Paint::default();
                    paint.set_color(rect_color);
                    pixmap.fill_rect(rect, &paint, Transform::identity(), None);

                    // Draw text
                    let title = format!("{} - {}", win.wm_class, win.title); // Show app name + title
                    draw_text_pixel(
                        &mut pixmap,
                        TextDrawParams {
                            fonts: &self.fonts,
                            x: theme::ITEM_TEXT_OFFSET_X,
                            y: y + theme::ITEM_TEXT_OFFSET_Y,
                            text: &title,
                            color: tiny_skia::Color::WHITE,
                            highlights: indices,
                        },
                    );
                }
                y += theme::ITEM_HEIGHT;
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
        match keysym {
            Keysym::Escape => self.exit = true,
            Keysym::Down => self.window_switcher_state.next(),
            Keysym::Up => self.window_switcher_state.prev(),
            Keysym::Return => {
                self.window_switcher_state.activate();
                self.exit = true;
            }
            Keysym::BackSpace => self.window_switcher_state.backspace(),
            _ if utf8
                .as_ref()
                .is_some_and(|t| !t.chars().any(|c| c.is_control())) =>
            {
                self.window_switcher_state
                    .input_text(utf8.as_ref().unwrap());
            }
            _ => {}
        }
        self.draw(qh);
    }
}
