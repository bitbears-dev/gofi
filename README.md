# Gofi

Gofi is a [Rofi](https://github.com/davatorium/rofi) alternative written in Rust, designed for Wayland environments.
It addresses the limitations of existing tools on certain compositors (specifically Gnome) by implementing custom solutions for window management.

## Features

### 1. Window Switcher

- **Protocol Support**: Primarily uses `wlr-foreign-toplevel-management` for Wayland compositors (e.g., Sway, Hyprland) [Planned/Partial].
- **Gnome Support**: Includes a fallback mechanism using Gnome Shell's private API via DBus (`org.gnome.Shell.Extensions.Windows`), ensuring functionality on standard Ubuntu/Fedora workstations.
- **Activation**: Switches focus to the selected window upon pressing `Enter`.

### 2. Fuzzy Search

- **Algorithm**: Uses `fuzzy-matcher` (clangd algorithm) for efficient filtering.
- **UI Feedback**: Matches are highlighted in cyan within the window list.

### 3. User Interface

- **Rendering**: Custom software rendering using `tiny-skia` and `ab_glyph`.
- **Keyboard Control**:
  - `Up` / `Down`: Navigate selection.
  - `Enter`: Activate window.
  - `Esc`: Exit application.
  - Type to filter.

## Architecture & Design Decisions

### Why not standard Wayland protocols?

Standard Wayland protocols like `wlr-foreign-toplevel-management` (used by wlroots-based compositors like Sway) are **not supported by GNOME (Mutter)** by default. GNOME relies on its own private APIs and D-Bus interfaces for window management to ensure security and integration with the shell.

### Implementation Strategy

To support window switching on GNOME Wayland, Gofi uses the **D-Bus** interface provided by the [Window Calls](https://github.com/ickyicky/window-calls) GNOME Shell Extension.

1. **Listing Windows**: Gofi executes `gdbus call ... org.gnome.Shell.Extensions.Windows.List` to retrieve a list of open windows in JSON format.
2. **Activating Windows**: Gofi executes `gdbus call ... org.gnome.Shell.Extensions.Windows.Activate <id>` to switch focus to the selected window.
3. **Rendering**: Gofi uses `tiny-skia` to render the UI directly to a Wayland surface buffer (`wl_shm`), bypassing GTK or other toolkits for minimal overhead.
4. **Input Handling**: Uses `smithay-client-toolkit` to handle keyboard events for navigation and text input.

## Requirements

1. **GNOME Shell Extension**: [Window Calls](https://extensions.gnome.org/extension/4724/window-calls/) must be installed and enabled if running on Gnome.
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
   - Type to filter windows.

## Known Limitations

- **Window Positioning**: On some compositors (specifically Gnome), the window may appear left-aligned or at a default position (0,0) despite the application providing geometry hints (`set_window_geometry` and `set_min_size`). This is determined by the compositor's placement policies for XDG Toplevel windows and is currently considered a known limitation.

## Tech Stack

- **Language**: Rust (2024 edition)
- **Wayland**: `smithay-client-toolkit`
- **Graphics**: `tiny-skia` for CPU-based 2D rendering.
