//! Hover documentation provider resolving Tailwind CSS declarations and component docs.

use crate::parser::RsxDocument;
use crate::types::{Hover, Position, Range};

/// Resolves documentation tooltip when hovering over a symbol at `position`.
pub fn resolve_hover(doc: &RsxDocument, position: Position) -> Option<Hover> {
    let offset = doc.source_map.position_to_offset(position);
    let (word, token_range) = find_word_and_range_at_offset(&doc.source, offset, &doc.source_map)?;

    // 1. Check if token is a Tailwind utility class
    if let Some(css_doc) = get_tailwind_hover_doc(&word) {
        return Some(Hover {
            contents: format!("```css\n{}\n```\n\n*Strata Tailwind JIT Utility*", css_doc),
            range: Some(token_range),
        });
    }

    // 2. Check if token is an RSX element or component
    if let Some(tag_doc) = get_element_hover_doc(&word) {
        return Some(Hover {
            contents: format!("### `<{}>`\n\n{}\n\n*Strata Framework Primitive*", word, tag_doc),
            range: Some(token_range),
        });
    }

    None
}

fn find_word_and_range_at_offset(
    source: &str,
    offset: usize,
    map: &crate::parser::SourceMap,
) -> Option<(String, Range)> {
    let bytes = source.as_bytes();
    if offset >= bytes.len() {
        return None;
    }

    let mut start = offset;
    while start > 0 && is_token_char(bytes[start - 1]) {
        start -= 1;
    }

    let mut end = offset;
    while end < bytes.len() && is_token_char(bytes[end]) {
        end += 1;
    }

    if start == end {
        return None;
    }

    let word = source[start..end].to_string();
    let range = Range::new(map.offset_to_position(start), map.offset_to_position(end));
    Some((word, range))
}

fn is_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'#' || b == b'[' || b == b']'
}

fn get_tailwind_hover_doc(class: &str) -> Option<&'static str> {
    match class {
        "flex" => Some("display: flex;"),
        "inline-flex" => Some("display: inline-flex;"),
        "grid" => Some("display: grid;"),
        "hidden" => Some("display: none;"),
        "block" => Some("display: block;"),
        "relative" => Some("position: relative;"),
        "absolute" => Some("position: absolute;"),
        "fixed" => Some("position: fixed;"),
        "sticky" => Some("position: sticky;"),
        "flex-row" => Some("flex-direction: row;"),
        "flex-col" => Some("flex-direction: column;"),
        "items-center" => Some("align-items: center;"),
        "items-start" => Some("align-items: flex-start;"),
        "items-end" => Some("align-items: flex-end;"),
        "justify-between" => Some("justify-content: space-between;"),
        "justify-center" => Some("justify-content: center;"),
        "justify-start" => Some("justify-content: flex-start;"),
        "justify-end" => Some("justify-content: flex-end;"),
        "p-2" => Some("padding: 0.5rem; /* 8px */"),
        "p-4" => Some("padding: 1rem; /* 16px */"),
        "p-6" => Some("padding: 1.5rem; /* 24px */"),
        "p-8" => Some("padding: 2rem; /* 32px */"),
        "px-2" => Some("padding-left: 0.5rem; padding-right: 0.5rem; /* 8px */"),
        "px-3" => Some("padding-left: 0.75rem; padding-right: 0.75rem; /* 12px */"),
        "px-4" => Some("padding-left: 1rem; padding-right: 1rem; /* 16px */"),
        "py-1" => Some("padding-top: 0.25rem; padding-bottom: 0.25rem; /* 4px */"),
        "py-2" => Some("padding-top: 0.5rem; padding-bottom: 0.5rem; /* 8px */"),
        "py-3" => Some("padding-top: 0.75rem; padding-bottom: 0.75rem; /* 12px */"),
        "gap-2" => Some("gap: 0.5rem; /* 8px */"),
        "gap-4" => Some("gap: 1rem; /* 16px */"),
        "gap-6" => Some("gap: 1.5rem; /* 24px */"),
        "w-full" => Some("width: 100%;"),
        "w-screen" => Some("width: 100vw;"),
        "h-full" => Some("height: 100%;"),
        "h-screen" => Some("height: 100vh;"),
        "text-xs" => Some("font-size: 0.75rem; /* 12px */\nline-height: 1rem;"),
        "text-sm" => Some("font-size: 0.875rem; /* 14px */\nline-height: 1.25rem;"),
        "text-base" => Some("font-size: 1rem; /* 16px */\nline-height: 1.5rem;"),
        "text-lg" => Some("font-size: 1.125rem; /* 18px */\nline-height: 1.75rem;"),
        "text-xl" => Some("font-size: 1.25rem; /* 20px */\nline-height: 1.75rem;"),
        "text-2xl" => Some("font-size: 1.5rem; /* 24px */\nline-height: 2rem;"),
        "font-mono" => Some("font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;"),
        "font-bold" => Some("font-weight: 700;"),
        "font-semibold" => Some("font-weight: 600;"),
        "rounded" => Some("border-radius: 0.25rem; /* 4px */"),
        "rounded-lg" => Some("border-radius: 0.5rem; /* 8px */"),
        "rounded-full" => Some("border-radius: 9999px;"),
        "border" => Some("border-width: 1px;"),
        "overflow-hidden" => Some("overflow: hidden;"),
        "select-none" => Some("user-select: none;"),
        "bg-[#08090a]" => Some("background-color: #08090a;"),
        "bg-[#0f1012]" => Some("background-color: #0f1012;"),
        "bg-[#15171a]" => Some("background-color: #15171a;"),
        "bg-emerald-500" => Some("background-color: rgb(16 185 129);"),
        "text-white" => Some("color: #ffffff;"),
        _ => None,
    }
}

fn get_element_hover_doc(tag: &str) -> Option<&'static str> {
    match tag {
        "Canvas3D" => Some("Unified 3D Viewport engine rendering Three.js scene graphs and hardware shaders."),
        "Mesh" => Some("3D Scene mesh binding a parametric/custom geometry to a PBR or shader material."),
        "BoxGeometry" => Some("Parametric 3D cuboid box geometry specification."),
        "SphereGeometry" => Some("Parametric 3D UV sphere geometry specification."),
        "StandardMaterial" => Some("Physically Based Rendering (PBR) Metallic-Roughness material."),
        "ShaderMaterial" => Some("Hardware custom shader material (SPIR-V, GLSL, WGSL)."),
        "PerspectiveCamera" => Some("Perspective 3D projection camera with FOV and target look-at."),
        "Button" => Some("Accessible, styled multi-variant interactive button component."),
        "Badge" => Some("Status tag or metadata pill indicator with domain styling."),
        "Card" => Some("Elevated content card with dark theme borders and spotlight support."),
        "Modal" => Some("Focus-trapped dialog overlay for critical user confirmation."),
        "Slider" => Some("Fine-grained numeric parameter slider control."),
        "Switch" => Some("Binary boolean toggle switch control."),
        _ => None,
    }
}
