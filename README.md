# Gofi - GNOME Wayland Window Switcher & Launcher

Gofi is a minimal, keyboard-driven window switcher and application launcher, designed as an alternative to Rofi for **GNOME on Wayland**.

## Features

- **Window Switcher**: Lists all active windows on the current workspace.
- **Application Launcher**: (Planned) Launch applications from `.desktop` files.
- **Lightweight**: Written in Rust, using `smithay-client-toolkit` for Wayland integration.
- **Customizable**: (Planned) Simple configuration for fonts and colors.

## Architecture & Design Decisions

### Why not standard Wayland protocols?

Standard Wayland protocols like `wlr-foreign-toplevel-management` (used by wlroots-based compositors like Sway) are **not supported by GNOME (Mutter)**. GNOME relies on its own private APIs and D-Bus interfaces for window management to ensure security and integration with the shell.

### Implementation Strategy

To support window switching on GNOME Wayland, Gofi uses the **D-Bus** interface provided by the [Window Calls](https://github.com/ickyicky/window-calls) GNOME Shell Extension.

1. **Listing Windows**: Gofi executes `gdbus call ... org.gnome.Shell.Extensions.Windows.List` to retrieve a list of open windows in JSON format.
2. **Activating Windows**: Gofi executes `gdbus call ... org.gnome.Shell.Extensions.Windows.Activate <id>` to switch focus to the selected window.
3. **Rendering**: Gofi uses `tiny-skia` to render the UI directly to a Wayland surface buffer (`wl_shm`), bypassing GTK or other toolkits for minimal overhead.
4. **Input Handling**: Uses `smithay-client-toolkit` to handle keyboard events for navigation (Up/Down) and selection (Enter).

## Requirements

1. **GNOME Shell Extension**: [Window Calls](https://extensions.gnome.org/extension/4724/window-calls/) must be installed and enabled.
    - This extension exposes the necessary D-Bus methods for window listing and activation.
2. **Dependencies**:
    - `gdbus` (usually pre-installed on GNOME systems as part of `glib2`).
    - Rust toolchain (cargo).

## Installation & Usage

1. Install the "Window Calls" extension from [extensions.gnome.org](https://extensions.gnome.org/extension/4724/window-calls/).
2. Clone this repository:

   ```bash
   git clone https://github.com/bitbears-dev/gofi.git
   cd gofi
   ```

3. Run Gofi:

   ```bash
   cargo run
   ```

4. **Controls**:
   - `Up` / `Down`: Navigate the window list.
   - `Enter`: Switch to the selected window.
   - `Esc`: Exit Gofi.
