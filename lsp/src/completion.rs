//! Context-aware code completion for RSX elements, attributes, and Tailwind JIT utilities.

use crate::parser::RsxDocument;
use crate::types::{CompletionItem, CompletionItemKind, Position};

/// Generates autocomplete proposals for the given document and cursor position.
pub fn compute_completions(doc: &RsxDocument, position: Position) -> Vec<CompletionItem> {
    let offset = doc.source_map.position_to_offset(position);
    let prefix = get_word_at_offset(&doc.source, offset);

    // Heuristic 1: If cursor is inside a class="..." attribute value
    if is_inside_class_attribute(&doc.source, offset) {
        return complete_tailwind_utilities(&prefix);
    }

    // Heuristic 2: If cursor is immediately after '<'
    if is_tag_name_context(&doc.source, offset) {
        return complete_tag_names(&prefix);
    }

    // Heuristic 3: Inside element attributes
    if is_attribute_context(&doc.source, offset) {
        return complete_attribute_names(&prefix);
    }

    // Default: Return common tags and Tailwind utilities
    let mut items = complete_tag_names(&prefix);
    items.extend(complete_tailwind_utilities(&prefix));
    items
}

fn get_word_at_offset(source: &str, offset: usize) -> String {
    let bytes = source.as_bytes();
    let mut start = offset;
    while start > 0 && is_word_char(bytes[start - 1]) {
        start -= 1;
    }
    source[start..offset].to_string()
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b':' || b == b'[' || b == b'#'
}

fn is_inside_class_attribute(source: &str, offset: usize) -> bool {
    let prefix_slice = &source[0..offset.min(source.len())];
    if let Some(last_class) = prefix_slice.rfind("class=") {
        let after_class = &prefix_slice[last_class + 6..];
        if let Some(stripped) = after_class.strip_prefix('"') {
            // Count unescaped quotes after the opening quote
            let quote_count = stripped.chars().filter(|&c| c == '"').count();
            return quote_count == 0;
        }
    }
    false
}

fn is_tag_name_context(source: &str, offset: usize) -> bool {
    let prefix_slice = &source[0..offset.min(source.len())];
    if let Some(last_lt) = prefix_slice.rfind('<') {
        let between = &prefix_slice[last_lt + 1..];
        return !between.contains('>') && !between.contains(' ') && !between.contains('\n');
    }
    false
}

fn is_attribute_context(source: &str, offset: usize) -> bool {
    let prefix_slice = &source[0..offset.min(source.len())];
    if let Some(last_lt) = prefix_slice.rfind('<') {
        let between = &prefix_slice[last_lt + 1..];
        return !between.contains('>') && (between.contains(' ') || between.contains('\n'));
    }
    false
}

/// Provides autocompletions for HTML tags, Strata 3D Viewport nodes, and strata_ui controls.
pub fn complete_tag_names(prefix: &str) -> Vec<CompletionItem> {
    const TAGS: &[(&str, &str)] = &[
        // HTML Primitives
        ("div", "Generic layout block container"),
        ("span", "Generic inline text container"),
        ("button", "Interactive clickable button element"),
        ("input", "Single-line interactive user text input"),
        ("textarea", "Multi-line user text input"),
        ("p", "Paragraph text element"),
        ("h1", "Top-level heading"),
        ("h2", "Second-level section heading"),
        ("h3", "Third-level heading"),
        ("header", "Header navigation banner"),
        ("footer", "Footer metadata banner"),
        ("main", "Dominant central document body"),
        ("table", "Tabular data display container"),
        ("tr", "Table row container"),
        ("th", "Table header column cell"),
        ("td", "Table data value cell"),
        ("img", "Image display element"),
        ("canvas", "2D drawing canvas"),
        // Strata 3D Viewport Primitives
        ("Canvas3D", "Unified Three.js-style 3D viewport canvas"),
        ("Mesh", "3D Scene mesh pairing geometry and material"),
        ("BoxGeometry", "Parametric 3D cuboid box geometry"),
        ("SphereGeometry", "Parametric 3D sphere geometry"),
        ("CylinderGeometry", "Parametric 3D cylinder geometry"),
        ("StandardMaterial", "High-level PBR metallic-roughness material"),
        ("ShaderMaterial", "Low-level custom hardware GLSL/SPIR-V shader"),
        ("PerspectiveCamera", "3D Perspective projection camera"),
        ("AmbientLight", "Non-directional ambient scene illumination"),
        ("DirectionalLight", "Sun-like directional scene light with shadows"),
        // strata_ui Component Library
        ("Button", "Accessible multi-variant button component"),
        ("Badge", "Status indicator and domain metadata tag"),
        ("Card", "Elevated content container with hairline borders"),
        ("Modal", "High-priority dialog overlay with focus trap"),
        ("Drawer", "Sliding perimeter flyout panel"),
        ("Slider", "Continuous and discrete numeric input slider"),
        ("Switch", "Binary boolean toggle switch control"),
        ("DataGrid", "High-frequency tabular data grid with sorting"),
        ("CommandPalette", "Keyboard-driven action palette (Cmd+K)"),
    ];

    TAGS.iter()
        .filter(|(name, _)| prefix.is_empty() || name.to_lowercase().starts_with(&prefix.to_lowercase()))
        .map(|(name, desc)| {
            CompletionItem::new(*name, CompletionItemKind::Class)
                .with_detail("Strata Element")
                .with_doc(*desc)
        })
        .collect()
}

/// Provides autocompletions for standard element attributes.
pub fn complete_attribute_names(prefix: &str) -> Vec<CompletionItem> {
    const ATTRS: &[(&str, &str)] = &[
        ("class", "Space-delimited Tailwind CSS utility classes"),
        ("id", "Unique document element identifier"),
        ("style", "Inline custom CSS style declarations"),
        ("onclick", "Pointer click synthetic event handler closure"),
        ("oninput", "Form input text change handler closure"),
        ("value", "Current value of form control"),
        ("disabled", "Boolean flag preventing interaction"),
        ("placeholder", "Subtle text placeholder prompt"),
        ("type", "Input control type (text, password, number)"),
    ];

    ATTRS.iter()
        .filter(|(name, _)| prefix.is_empty() || name.starts_with(prefix))
        .map(|(name, desc)| {
            CompletionItem::new(*name, CompletionItemKind::Property)
                .with_detail("Attribute")
                .with_doc(*desc)
                .with_insert(format!("{}=\"\"", name))
        })
        .collect()
}

/// Provides autocompletions for Tailwind CSS v3/v4 JIT utility classes.
pub fn complete_tailwind_utilities(prefix: &str) -> Vec<CompletionItem> {
    const UTILITIES: &[(&str, &str, &str)] = &[
        // Layout & Display
        ("flex", "display: flex;", "Flexbox container layout"),
        ("inline-flex", "display: inline-flex;", "Inline flexbox container"),
        ("grid", "display: grid;", "CSS Grid container layout"),
        ("hidden", "display: none;", "Removes element from layout tree"),
        ("block", "display: block;", "Standard block display"),
        ("relative", "position: relative;", "Relative positioning context"),
        ("absolute", "position: absolute;", "Absolute positioning relative to parent"),
        ("fixed", "position: fixed;", "Fixed viewport coordinates"),
        ("sticky", "position: sticky;", "Sticky scrolling boundary"),
        // Flexbox & Grid
        ("flex-row", "flex-direction: row;", "Horizontal flexbox direction"),
        ("flex-col", "flex-direction: column;", "Vertical flexbox column"),
        ("items-center", "align-items: center;", "Cross-axis center alignment"),
        ("items-start", "align-items: flex-start;", "Cross-axis start alignment"),
        ("items-end", "align-items: flex-end;", "Cross-axis end alignment"),
        ("justify-between", "justify-content: space-between;", "Main-axis space between items"),
        ("justify-center", "justify-content: center;", "Main-axis center alignment"),
        ("justify-start", "justify-content: flex-start;", "Main-axis start alignment"),
        ("justify-end", "justify-content: flex-end;", "Main-axis end alignment"),
        ("grid-cols-2", "grid-template-columns: repeat(2, minmax(0, 1fr));", "2-column CSS Grid"),
        ("grid-cols-3", "grid-template-columns: repeat(3, minmax(0, 1fr));", "3-column CSS Grid"),
        ("grid-cols-4", "grid-template-columns: repeat(4, minmax(0, 1fr));", "4-column CSS Grid"),
        ("grid-cols-12", "grid-template-columns: repeat(12, minmax(0, 1fr));", "12-column CSS Grid"),
        ("gap-2", "gap: 0.5rem (8px);", "8px grid/flex gutter spacing"),
        ("gap-4", "gap: 1rem (16px);", "16px grid/flex gutter spacing"),
        ("gap-6", "gap: 1.5rem (24px);", "24px grid/flex gutter spacing"),
        // Spacing (Padding & Margin)
        ("p-2", "padding: 0.5rem (8px);", "8px uniform padding"),
        ("p-4", "padding: 1rem (16px);", "16px uniform padding"),
        ("p-6", "padding: 1.5rem (24px);", "24px uniform padding"),
        ("px-3", "padding-left: 0.75rem; padding-right: 0.75rem;", "12px horizontal padding"),
        ("px-4", "padding-left: 1rem; padding-right: 1rem;", "16px horizontal padding"),
        ("py-2", "padding-top: 0.5rem; padding-bottom: 0.5rem;", "8px vertical padding"),
        ("m-auto", "margin: auto;", "Centered automatic margin"),
        ("mx-auto", "margin-left: auto; margin-right: auto;", "Horizontal centered margin"),
        // Sizing
        ("w-full", "width: 100%;", "Full width of container"),
        ("w-screen", "width: 100vw;", "Full viewport width"),
        ("h-full", "height: 100%;", "Full height of container"),
        ("h-screen", "height: 100vh;", "Full viewport height"),
        ("max-w-7xl", "max-width: 80rem (1280px);", "Standard 1280px container constraint"),
        // Typography & Colors
        ("text-xs", "font-size: 0.75rem (12px); line-height: 1rem;", "12px micro typography"),
        ("text-sm", "font-size: 0.875rem (14px); line-height: 1.25rem;", "14px body typography"),
        ("text-base", "font-size: 1rem (16px); line-height: 1.5rem;", "16px base text"),
        ("text-lg", "font-size: 1.125rem (18px); line-height: 1.75rem;", "18px large text"),
        ("font-mono", "font-family: ui-monospace, SFMono-Regular, monospace;", "Monospace font stack"),
        ("font-bold", "font-weight: 700;", "Bold font weight"),
        ("text-white", "color: #ffffff;", "Pure white text"),
        ("text-slate-400", "color: #94a3b8;", "Muted slate secondary text"),
        ("bg-[#08090a]", "background-color: #08090a;", "Strata deep atmospheric void canvas"),
        ("bg-[#0f1012]", "background-color: #0f1012;", "Base card canvas background"),
        ("bg-[#15171a]", "background-color: #15171a;", "Elevated card surface background"),
        ("bg-emerald-500", "background-color: #10b981;", "Operational health emerald status"),
        ("bg-cyan-500", "background-color: #06b6d4;", "Avionics cyan telemetry accent"),
        ("bg-amber-500", "background-color: #f59e0b;", "Caution / alert amber accent"),
        ("bg-red-500", "background-color: #ef4444;", "Alarm / critical red accent"),
        // Borders & Visual Effects
        ("border", "border-width: 1px;", "1px hairline border"),
        ("border-[rgba(255,255,255,0.07)]", "border-color: rgba(255, 255, 255, 0.07);", "Subtle card boundary"),
        ("rounded", "border-radius: 0.25rem (4px);", "4px corner radius"),
        ("rounded-lg", "border-radius: 0.5rem (8px);", "8px corner radius"),
        ("rounded-full", "border-radius: 9999px;", "Fully pill rounded radius"),
        ("overflow-hidden", "overflow: hidden;", "Clips child overflow"),
        ("select-none", "user-select: none;", "Prevents text selection"),
    ];

    UTILITIES
        .iter()
        .filter(|(name, _, _)| {
            if prefix.is_empty() || name.starts_with(prefix) {
                return true;
            }
            if (prefix == "p-" && (name.starts_with("px-") || name.starts_with("py-")))
                || (prefix == "m-" && (name.starts_with("mx-") || name.starts_with("my-")))
            {
                return true;
            }
            false
        })
        .map(|(name, css, doc)| {
            CompletionItem::new(*name, CompletionItemKind::Property)
                .with_detail(*css)
                .with_doc(*doc)
        })
        .collect()
}
