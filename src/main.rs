use std::time::Duration;

use smithay_client_toolkit::reexports::calloop::{
    EventLoop,
    channel::{self},
    timer::{TimeoutAction, Timer},
};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::xdg::{XdgShell, window::WindowDecorations},
    shm::{Shm, slot::SlotPool},
};
use wayland_client::{Connection, globals::registry_queue_init};

mod app;
mod fonts;
mod handlers;
mod key_repeat;
mod rendering;
mod test_switcher;
mod theme;
mod window_switcher;

use app::App;
use fonts::load_fonts;
use key_repeat::{KeyRepeat, RepeatCommand};
use window_switcher::WindowSwitcherState;

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
        fonts: load_fonts(),
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

    if let Some(output) = app.output_state.outputs().next()
        && let Some(info) = app.output_state.info(&output)
    {
        if let Some(mode) = info.logical_size {
            target_width = (mode.0 as f32 * 0.8) as u32;
            target_height = (mode.1 as f32 * 0.3) as u32;
        } else if let Some(mode) = info.modes.iter().find(|m| m.current || m.preferred) {
            target_width = (mode.dimensions.0 as f32 * 0.8) as u32;
            target_height = (mode.dimensions.1 as f32 * 0.3) as u32;
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
                if let Some(rep) = &app.current_key_repeat
                    && rep.keysym == keysym
                {
                    loop_handle_clone.remove(rep.token);
                    app.current_key_repeat = None;
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
