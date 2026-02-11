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
            query: String::new(),
            matcher: SkimMatcherV2::default(),
        }
    }

    pub fn refresh(&mut self) {
        self.windows = list_windows().unwrap_or_else(|e| {
            eprintln!("Failed to list windows: {}", e);
            Vec::new()
        });
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
    let json_str = if json_str.starts_with("('") && json_str.ends_with("',)") {
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

    // Unescape the string if it was a GVariant string literal?
    // GVariant string literal might have escaped quotes.
    // e.g. "('\"[{\\\"id\\\": ...}]\"',)"
    // The user's output suggests raw JSON inside the string: `('[{"in_current_workspace":...}]',)`
    // So extracting between `('` and `',)` should be enough.
    // But `gdbus` might escape internal quotes if they exist.
    // However, the JSON itself uses double quotes. `gdbus` wraps the string in single quotes.
    // If the JSON contains single quotes, `gdbus` would escape them.
    // Assuming standard output for now.

    // Also handle escaped characters if gdbus escapes them.
    // Rust's `String` from `gdbus` output is raw bytes.
    // If we take the slice, it's still raw.
    // Use `serde_json` to parse directly.

    let windows: Vec<GnomeWindow> = serde_json::from_str(json_str)?;
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
