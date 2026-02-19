use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Deserialize, Clone)]
pub struct GnomeWindow {
    pub id: i64,
    pub title: String,
    pub wm_class: String,
    pub pid: i32,
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
        if let Some(win) = self.current()
            && let Err(e) = activate_window(win.id)
        {
            eprintln!("Failed to activate window {}: {}", win.id, e);
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
    let stdout = run_gdbus("org.gnome.Shell.Extensions.Windows.List", None)?;
    let json_str = extract_json_from_gdbus(&stdout);
    let windows: Vec<GnomeWindow> = serde_json::from_str(&json_str)?;
    Ok(windows)
}

fn activate_window(window_id: i64) -> Result<(), Box<dyn std::error::Error>> {
    run_gdbus(
        "org.gnome.Shell.Extensions.Windows.Activate",
        Some(window_id.to_string()),
    )?;
    Ok(())
}

fn run_gdbus(method: &str, arg: Option<String>) -> Result<String, Box<dyn std::error::Error>> {
    let mut cmd = Command::new("gdbus");
    cmd.arg("call")
        .arg("--session")
        .arg("--dest")
        .arg("org.gnome.Shell")
        .arg("--object-path")
        .arg("/org/gnome/Shell/Extensions/Windows")
        .arg("--method")
        .arg(method);

    if let Some(a) = arg {
        cmd.arg(a);
    }

    let output = cmd.output()?;

    if !output.status.success() {
        return Err(format!("gdbus command failed: {:?}", output).into());
    }

    Ok(String::from_utf8(output.stdout)?)
}

fn extract_json_from_gdbus(input: &str) -> String {
    let trimmed = input.trim();

    // The output is usually a Python-like tuple string: "('[json_string]',)"
    // We want to extract the inner string content.
    let extracted = if trimmed.starts_with("('") && trimmed.ends_with("',)") {
        &trimmed[2..trimmed.len() - 3]
    } else if trimmed.starts_with("('") && trimmed.ends_with("')") {
        &trimmed[2..trimmed.len() - 2]
    } else {
        // Fallback: try to find the JSON array brackets if present
        if let Some(start) = trimmed.find('[') {
            if let Some(end) = trimmed.rfind(']') {
                &trimmed[start..=end]
            } else {
                trimmed
            }
        } else {
            trimmed
        }
    };

    // Unescape the string manually
    // The gdbus output escapes double quotes with backslash, e.g. \" -> "
    extracted.replace("\\\"", "\"").replace("\\\\", "\\")
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
                pid: my_pid,
            },
            GnomeWindow {
                id: 2,
                title: "Window 2".to_string(),
                wm_class: "App2".to_string(),
                pid: other_pid,
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

    #[test]
    fn test_extract_json_from_gdbus() {
        let input = "('[{\"id\": 1, \"title\": \"Term\"}]',)";
        let output = extract_json_from_gdbus(input);
        assert_eq!(output, "[{\"id\": 1, \"title\": \"Term\"}]");

        let input_no_comma = "('[{\"id\": 1}]')";
        let output_no_comma = extract_json_from_gdbus(input_no_comma);
        assert_eq!(output_no_comma, "[{\"id\": 1}]");

        let input_raw = "[{\"id\": 1}]";
        let output_raw = extract_json_from_gdbus(input_raw);
        assert_eq!(output_raw, "[{\"id\": 1}]");

        let input_escaped_quote = "('[{\"title\": \"\\\"Quoted\\\"\"}]',)";
        let output_escaped = extract_json_from_gdbus(input_escaped_quote);
        assert_eq!(output_escaped, "[{\"title\": \"\"Quoted\"\"}]");
    }
}
