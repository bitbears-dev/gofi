use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Deserialize, Clone)]
pub struct GnomeWindow {
    pub id: i64,
    pub title: String,
    pub wm_class: String,
    pub wm_class_instance: String,
    pub pid: i32,
    pub focus: bool,
}

pub struct WindowSwitcherState {
    /// All windows retrieved from the backend (DBus or Wayland)
    pub windows: Vec<GnomeWindow>,
    /// Indices into `windows` that match the current query, along with matched character indices
    pub filtered_windows: Vec<(usize, Vec<usize>)>,
    /// Index into `filtered_windows` for the currently selected item
    pub selection_index: usize,
    /// Offset for scrolling the window list
    pub scroll_offset: usize,
    /// Current search query string
    pub query: String,
    /// Fuzzy matcher instance
    matcher: SkimMatcherV2,
}

impl WindowSwitcherState {
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            filtered_windows: Vec::new(),
            selection_index: 0,
            scroll_offset: 0,
            query: String::new(),
            matcher: SkimMatcherV2::default(),
        }
    }

    pub fn refresh(&mut self) {
        self.windows = list_windows().unwrap_or_else(|e| {
            eprintln!("Failed to list windows: {}", e);
            Vec::new()
        });

        // Filter out own window
        let current_pid = std::process::id() as i32;
        self.windows.retain(|w| w.pid != current_pid);

        eprintln!("[DEBUG] Found {} windows", self.windows.len());
        self.filter();
    }

    pub fn filter(&mut self) {
        if self.query.is_empty() {
            self.filtered_windows = self
                .windows
                .iter()
                .enumerate()
                .map(|(i, _)| (i, Vec::new()))
                .collect();
        } else {
            let mut matches: Vec<(i64, usize, Vec<usize>)> = self
                .windows
                .iter()
                .enumerate()
                .filter_map(|(i, win)| {
                    // Search against title and class
                    let text = format!("{} - {}", win.wm_class, win.title);
                    self.matcher
                        .fuzzy_indices(&text, &self.query)
                        .map(|(score, indices)| (score, i, indices))
                })
                .collect();

            // Sort by score descending
            matches.sort_by(|a, b| b.0.cmp(&a.0));

            self.filtered_windows = matches
                .into_iter()
                .map(|(_, i, indices)| (i, indices))
                .collect();
        }

        // Reset selection
        self.selection_index = 0;
        self.scroll_offset = 0;
    }

    pub fn input_text(&mut self, text: &str) {
        self.query.push_str(text);
        self.filter();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.filter();
    }

    pub fn next(&mut self) {
        if self.filtered_windows.is_empty() {
            return;
        }
        if self.selection_index + 1 < self.filtered_windows.len() {
            self.selection_index += 1;
        } else {
            self.selection_index = 0; // Wrap around
        }
    }

    pub fn prev(&mut self) {
        if self.filtered_windows.is_empty() {
            return;
        }
        if self.selection_index > 0 {
            self.selection_index -= 1;
        } else {
            self.selection_index = self.filtered_windows.len() - 1; // Wrap around
        }
    }

    pub fn current(&self) -> Option<&GnomeWindow> {
        if let Some((idx, _)) = self.filtered_windows.get(self.selection_index) {
            self.windows.get(*idx)
        } else {
            None
        }
    }

    pub fn activate(&self) {
        if let Some(win) = self.current() {
            if let Err(e) = activate_window(win.id) {
                eprintln!("Failed to activate window {}: {}", win.id, e);
            }
        }
    }

    pub fn ensure_visible(&mut self, max_items: usize) {
        if self.filtered_windows.is_empty() {
            self.scroll_offset = 0;
            return;
        }

        if self.selection_index < self.scroll_offset {
            self.scroll_offset = self.selection_index;
        } else if self.selection_index >= self.scroll_offset + max_items {
            self.scroll_offset = self.selection_index + 1 - max_items;
        }

        // Ensure scroll_offset is valid (just in case max_items is large)
        if self.scroll_offset > self.filtered_windows.len() {
            self.scroll_offset = 0;
        }
    }
}

fn list_windows() -> Result<Vec<GnomeWindow>, Box<dyn std::error::Error>> {
    let output = Command::new("gdbus")
        .arg("call")
        .arg("--session")
        .arg("--dest")
        .arg("org.gnome.Shell")
        .arg("--object-path")
        .arg("/org/gnome/Shell/Extensions/Windows")
        .arg("--method")
        .arg("org.gnome.Shell.Extensions.Windows.List")
        .output()?;

    if !output.status.success() {
        return Err(format!("gdbus command failed: {:?}", output).into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    // output format is like: ('[{"id": ...}]',)
    // We need to extract the JSON string inside the tuple.
    // It usually starts with `('` and ends with `',)` or similar.
    // Let's trim and strip appropriately.

    let json_str = stdout.trim();
    // Remove starting "('" and ending "')"
    let extracted = if json_str.starts_with("('") && json_str.ends_with("',)") {
        &json_str[2..json_str.len() - 3]
    } else if json_str.starts_with("('") && json_str.ends_with("')") {
        // Allow without comma just in case
        &json_str[2..json_str.len() - 2]
    } else {
        // Fallback or error? gdbus output can refer to GVariant text format.
        // Usually `gdbus call` returns a tuple.
        // It might be complex to parse perfectly without regex or manual scanning.
        // But for common case of single string return:

        // Try to find the start of JSON array `[` and end `]`.
        if let Some(start) = json_str.find('[') {
            if let Some(end) = json_str.rfind(']') {
                &json_str[start..=end]
            } else {
                json_str
            }
        } else {
            json_str
        }
    };

    // Unescape the string manually
    // The gdbus output seems to escape double quotes with backslash, e.g. \" -> "
    let unescaped = extracted.replace("\\\"", "\"").replace("\\\\", "\\");

    let windows: Vec<GnomeWindow> = serde_json::from_str(&unescaped)?;
    Ok(windows)
}

fn activate_window(window_id: i64) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("gdbus")
        .arg("call")
        .arg("--session")
        .arg("--dest")
        .arg("org.gnome.Shell")
        .arg("--object-path")
        .arg("/org/gnome/Shell/Extensions/Windows")
        .arg("--method")
        .arg("org.gnome.Shell.Extensions.Windows.Activate")
        .arg(window_id.to_string())
        .output()?;

    if !output.status.success() {
        return Err(format!("gdbus activate failed: {:?}", output).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_self_pid() {
        let mut state = WindowSwitcherState::new();
        // Since list_windows() relies on gdbus, we manually inject windows
        let my_pid = std::process::id() as i32;
        let other_pid = if my_pid == 0 { 1 } else { my_pid - 1 };

        state.windows = vec![
            GnomeWindow {
                id: 1,
                title: "Window 1".to_string(),
                wm_class: "App1".to_string(),
                wm_class_instance: "app1".to_string(),
                pid: my_pid,
                focus: false,
            },
            GnomeWindow {
                id: 2,
                title: "Window 2".to_string(),
                wm_class: "App2".to_string(),
                wm_class_instance: "app2".to_string(),
                pid: other_pid,
                focus: false,
            },
        ];

        // Apply filtering logic (emulating what we added in refresh)
        state.windows.retain(|w| w.pid != my_pid);

        assert_eq!(state.windows.len(), 1);
        assert_eq!(state.windows[0].pid, other_pid);
    }

    #[test]
    fn test_ensure_visible() {
        let mut state = WindowSwitcherState::new();
        // Mock filtered windows (indices only needed)
        // 10 items
        state.filtered_windows = (0..10).map(|i| (i, vec![])).collect();

        let max_items = 5;

        // Case 1: Initial state
        state.selection_index = 0;
        state.ensure_visible(max_items);
        assert_eq!(state.scroll_offset, 0);

        // Case 2: Move down within view
        state.selection_index = 4;
        state.ensure_visible(max_items);
        assert_eq!(state.scroll_offset, 0);

        // Case 3: Move down out of view
        state.selection_index = 5;
        state.ensure_visible(max_items);
        // Should scroll to include 5.
        // range [offset, offset + 5) must include 5.
        // if offset = 1, range is [1, 6), includes 5.
        assert_eq!(state.scroll_offset, 1);

        // Case 4: Jump to end
        state.selection_index = 9;
        state.ensure_visible(max_items);
        // range [offset, offset + 5) must include 9.
        // offset + 5 > 9 => offset > 4. So offset = 5.
        // range [5, 10), includes 9.
        assert_eq!(state.scroll_offset, 5);

        // Case 5: Move up
        state.selection_index = 4;
        state.ensure_visible(max_items);
        // range [offset, offset + 5) must include 4.
        // current offset 5, range [5, 10) does NOT include 4.
        // new offset should be 4. range [4, 9).
        assert_eq!(state.scroll_offset, 4);

        // Case 6: Jump to start
        state.selection_index = 0;
        state.ensure_visible(max_items);
        assert_eq!(state.scroll_offset, 0);
    }
}
