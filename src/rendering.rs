use ab_glyph::{Font, PxScale, ScaleFont};
use tiny_skia::PixmapMut;

use crate::theme;

pub fn draw_text_pixel(
    pixmap: &mut PixmapMut,
    fonts: &[ab_glyph::FontVec],
    x: f32,
    y: f32,
    text: &str,
    color: tiny_skia::Color,
    highlights: &[usize], // Add mismatch here
) {
    let scale = PxScale::from(theme::FONT_SIZE);
    // Use the measurement from the first font for simplicity of layout
    // In a real text layout engine, this would be more complex.
    let default_font = &fonts[0];

    let mut pen_x = x;
    let pen_y = y + theme::FONT_SIZE; // Baseline

    // Convert color to u8 components (ARGB)
    // Default color
    let r_def = (color.red() * 255.0) as u8;
    let g_def = (color.green() * 255.0) as u8;
    let b_def = (color.blue() * 255.0) as u8;

    // Highlight color
    let (r_hl, g_hl, b_hl) = theme::HIGHLIGHT_COLOR;

    for (char_idx, c) in text.chars().enumerate() {
        if c.is_control() {
            continue;
        }

        let is_highlight = highlights.contains(&char_idx);
        let (r, g, b) = if is_highlight {
            (r_hl, g_hl, b_hl)
        } else {
            (r_def, g_def, b_def)
        };

        // Find the font that contains the glyph
        let mut best_font = default_font;
        let mut glyph_id = best_font.glyph_id(c);

        if glyph_id.0 == 0 {
            for font in fonts.iter().skip(1) {
                let id = font.glyph_id(c);
                if id.0 != 0 {
                    best_font = font;
                    glyph_id = id;
                    break;
                }
            }
        }

        let scaled_font = best_font.as_scaled(scale);
        let glyph = glyph_id.with_scale_and_position(scale, ab_glyph::point(pen_x, pen_y));

        if let Some(outlined) = scaled_font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|px, py, v| {
                let px = bounds.min.x as i32 + px as i32;
                let py = bounds.min.y as i32 + py as i32;
                if px >= 0 && px < pixmap.width() as i32 && py >= 0 && py < pixmap.height() as i32 {
                    let idx = (py as usize * pixmap.width() as usize + px as usize) * 4;
                    let pixel = &mut pixmap.data_mut()[idx..idx + 4];

                    // Alpha blending
                    // v is coverage (0.0 - 1.0)
                    if v > 0.01 {
                        let src_a = (v * 255.0) as u16;
                        let inv_a = 255 - src_a;

                        // pixel layout: [Blue, Green, Red, Alpha]
                        pixel[0] = ((b as u16 * src_a + pixel[0] as u16 * inv_a) / 255) as u8;
                        pixel[1] = ((g as u16 * src_a + pixel[1] as u16 * inv_a) / 255) as u8;
                        pixel[2] = ((r as u16 * src_a + pixel[2] as u16 * inv_a) / 255) as u8;
                        pixel[3] = ((255 * src_a + pixel[3] as u16 * inv_a) / 255) as u8;
                    }
                }
            });
        }
        // Advance using the font that actually rendered the glyph, or default if it's 0-width?
        // Ideally we should use the advance from the font used.
        pen_x += scaled_font.h_advance(glyph_id);
    }
}
