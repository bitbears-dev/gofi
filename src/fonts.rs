pub fn load_fonts() -> Vec<ab_glyph::FontVec> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    let mut loaded_fonts = Vec::new();

    // 1. Try to get default font from GNOME settings (Primary)
    let de_font = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "font-name"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| {
            let s = s.trim().trim_matches('\'');
            let parts: Vec<&str> = s.rsplitn(2, ' ').collect();
            if parts.len() == 2 && parts[0].parse::<f32>().is_ok() {
                parts[1].to_string()
            } else {
                s.to_string()
            }
        });

    // Helper to load a font by family name
    let load_font = |family: fontdb::Family| -> Option<ab_glyph::FontVec> {
        let query = fontdb::Query {
            families: &[family],
            ..Default::default()
        };

        if let Some(id) = db.query(&query) {
            let face = db.face(id).expect("font face");
            let (path, index) = match &face.source {
                fontdb::Source::File(path) => (path, face.index),
                fontdb::Source::SharedFile(path, _) => (path, face.index),
                _ => return None, // Skip unsupported sources
            };

            println!("Loading font: {:?}", path);
            if let Ok(data) = std::fs::read(path) {
                return ab_glyph::FontVec::try_from_vec_and_index(data, index).ok();
            }
        }
        None
    };

    // Load Primary Font
    if let Some(ref name) = de_font {
        println!("Detected DE font: {}", name);
        if let Some(font) = load_font(fontdb::Family::Name(name)) {
            loaded_fonts.push(font);
        }
    } else {
        // Fallback to generic SansSerif if no DE font detected
        if let Some(font) = load_font(fontdb::Family::SansSerif) {
            loaded_fonts.push(font);
        }
    }

    // 2. Load Fallback CJK Font
    let cjk_families = [
        "Noto Sans CJK JP",
        "Noto Sans CJK KR",
        "Noto Sans CJK SC",
        "Noto Sans CJK TC",
        "Noto Sans CJK",
        "Droid Sans Fallback",
    ];

    for family in cjk_families {
        if let Some(font) = load_font(fontdb::Family::Name(family)) {
            loaded_fonts.push(font);
            // We only need one good CJK fallback usually
            break;
        }
    }

    // Ensure we have at least one font
    if loaded_fonts.is_empty()
        && let Some(font) = load_font(fontdb::Family::SansSerif)
    {
        loaded_fonts.push(font);
    }
    // If still empty, try to load *any* sans-serif from system (last ditch)
    if loaded_fonts.is_empty() {
        panic!("Could not load any suitable font");
    }

    loaded_fonts
}
