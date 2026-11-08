//! Document color provider for gutter color swatches and color pickers.

use crate::parser::RsxDocument;
use crate::types::{ColorInformation, ColorPresentation, ColorRgba, Range, TextEdit};

/// Discovers all color definitions (Hex, RGBA, Tailwind classes) within the document.
pub fn find_document_colors(doc: &RsxDocument) -> Vec<ColorInformation> {
    let mut colors = Vec::new();
    let src = &doc.source;
    let mut i = 0;

    while i < src.len() {
        if !src.is_char_boundary(i) {
            i += 1;
            while i < src.len() && !src.is_char_boundary(i) {
                i += 1;
            }
            continue;
        }

        // 1. Hex Color Match: #[0-9a-fA-F]{6}
        if src.as_bytes()[i] == b'#' && i + 7 <= src.len() && src.is_char_boundary(i + 7) {
            let candidate = &src[i..i + 7];
            if let Some(rgba) = parse_hex_color(candidate) {
                let range = Range::new(
                    doc.source_map.offset_to_position(i),
                    doc.source_map.offset_to_position(i + 7),
                );
                colors.push(ColorInformation { range, color: rgba });
                i += 7;
                continue;
            }
        }

        // 2. Tailwind Named Color Classes in attributes
        if let Some((len, rgba)) = match_tailwind_color_class(&src[i..]) {
            let end = i + len;
            if src.is_char_boundary(end) {
                let range = Range::new(
                    doc.source_map.offset_to_position(i),
                    doc.source_map.offset_to_position(end),
                );
                colors.push(ColorInformation { range, color: rgba });
                i += len;
                continue;
            }
        }

        let ch = src[i..].chars().next().unwrap();
        i += ch.len_utf8();
    }

    colors
}

/// Generates replacement presentation formats when user changes color in editor color picker.
pub fn format_color_presentation(color: ColorRgba, range: Range) -> Vec<ColorPresentation> {
    let r = (color.red * 255.0).round() as u8;
    let g = (color.green * 255.0).round() as u8;
    let b = (color.blue * 255.0).round() as u8;

    let hex = format!("#{:02x}{:02x}{:02x}", r, g, b);
    let rgb = format!("rgb({}, {}, {})", r, g, b);
    let rgba = format!("rgba({}, {}, {}, {:.2})", r, g, b, color.alpha);

    vec![
        ColorPresentation {
            label: hex.clone(),
            text_edit: Some(TextEdit {
                range,
                new_text: hex,
            }),
        },
        ColorPresentation {
            label: rgb.clone(),
            text_edit: Some(TextEdit {
                range,
                new_text: rgb,
            }),
        },
        ColorPresentation {
            label: rgba.clone(),
            text_edit: Some(TextEdit {
                range,
                new_text: rgba,
            }),
        },
    ]
}

fn parse_hex_color(s: &str) -> Option<ColorRgba> {
    if !s.starts_with('#') || s.len() != 7 {
        return None;
    }
    let r = u8::from_str_radix(&s[1..3], 16).ok()?;
    let g = u8::from_str_radix(&s[3..5], 16).ok()?;
    let b = u8::from_str_radix(&s[5..7], 16).ok()?;
    Some(ColorRgba {
        red: (r as f32) / 255.0,
        green: (g as f32) / 255.0,
        blue: (b as f32) / 255.0,
        alpha: 1.0,
    })
}

fn match_tailwind_color_class(s: &str) -> Option<(usize, ColorRgba)> {
    const COLOR_CLASSES: &[(&str, (u8, u8, u8))] = &[
        ("text-white", (255, 255, 255)),
        ("text-black", (0, 0, 0)),
        ("bg-white", (255, 255, 255)),
        ("bg-black", (0, 0, 0)),
        ("bg-slate-900", (15, 23, 42)),
        ("bg-slate-800", (30, 41, 59)),
        ("bg-neutral-900", (23, 23, 23)),
        ("bg-neutral-950", (10, 10, 10)),
        ("bg-emerald-500", (16, 185, 129)),
        ("text-emerald-500", (16, 185, 129)),
        ("bg-cyan-500", (6, 182, 212)),
        ("text-cyan-400", (34, 211, 238)),
        ("bg-amber-500", (245, 158, 11)),
        ("text-amber-400", (251, 191, 36)),
        ("bg-red-500", (239, 68, 68)),
        ("text-red-500", (239, 68, 68)),
        ("bg-indigo-500", (99, 102, 241)),
    ];

    for (name, (r, g, b)) in COLOR_CLASSES {
        if s.starts_with(name) {
            // Ensure next char is whitespace, quote, or end of string
            let len = name.len();
            if len == s.len()
                || s.as_bytes()[len].is_ascii_whitespace()
                || matches!(s.as_bytes()[len], b'"' | b'\'' | b'<' | b'>' | b'}' | b'/')
            {
                return Some((
                    len,
                    ColorRgba {
                        red: (*r as f32) / 255.0,
                        green: (*g as f32) / 255.0,
                        blue: (*b as f32) / 255.0,
                        alpha: 1.0,
                    },
                ));
            }
        }
    }

    None
}
