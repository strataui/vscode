//! Navigation, Definition, Reference Collection, Document Symbols, Workspace Symbols, and Rename Refactoring.

use std::collections::HashMap;
use crate::parser::{RsxDocument, RsxNode, Span};
use crate::types::{
    DocumentSymbol, InlayHint, InlayHintKind, Location, Position, Range,
    SemanticTokenType, SemanticTokens, SymbolInformation, SymbolKind, TextEdit,
};

/// Symbol under the user cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolUnderCursor {
    ElementTag {
        name: String,
        span: Span,
    },
    AttributeName {
        element_name: String,
        attr_name: String,
        span: Span,
    },
}

/// Identifies the AST symbol under the given cursor position.
pub fn find_symbol_at_position(doc: &RsxDocument, pos: Position) -> Option<SymbolUnderCursor> {
    let offset = doc.source_map.position_to_offset(pos);
    for node in &doc.nodes {
        if let Some(sym) = find_in_node(node, offset) {
            return Some(sym);
        }
    }

    // Check Tree-Sitter AST for identifier / type_identifier under cursor
    if let Some(tree) = &doc.tree {
        if let Some(leaf) = find_ast_leaf_at_offset(tree.root_node(), offset) {
            if leaf.kind() == "identifier" || leaf.kind() == "type_identifier" || leaf.kind() == "field_identifier" {
                let name = doc.source.get(leaf.start_byte()..leaf.end_byte()).unwrap_or("").to_string();
                if !name.is_empty() {
                    return Some(SymbolUnderCursor::ElementTag {
                        name,
                        span: Span::new(leaf.start_byte(), leaf.end_byte()),
                    });
                }
            }
        }
    }

    // Fallback: Check if cursor is on a component function definition (e.g. `pub fn Header`)
    let source = &doc.source;
    let line_start = doc.source_map.position_to_offset(Position::new(pos.line, 0));
    let next_line = doc.source_map.position_to_offset(Position::new(pos.line + 1, 0));
    let line_text = &source[line_start..next_line.min(source.len())];

    if let Some(fn_pos) = line_text.find("fn ") {
        let after_fn = &line_text[fn_pos + 3..];
        let name_end = after_fn.find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(after_fn.len());
        let name = after_fn[..name_end].trim();
        if !name.is_empty() {
            let start = line_start + fn_pos + 3;
            let end = start + name.len();
            if offset >= start && offset <= end {
                return Some(SymbolUnderCursor::ElementTag {
                    name: name.to_string(),
                    span: Span::new(start, end),
                });
            }
        }
    }

    None
}

fn find_ast_leaf_at_offset<'a>(node: tree_sitter::Node<'a>, offset: usize) -> Option<tree_sitter::Node<'a>> {
    if offset < node.start_byte() || offset > node.end_byte() {
        return None;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if offset >= child.start_byte() && offset <= child.end_byte() {
            if let Some(deeper) = find_ast_leaf_at_offset(child, offset) {
                return Some(deeper);
            }
        }
    }
    Some(node)
}

fn find_in_node(node: &RsxNode, offset: usize) -> Option<SymbolUnderCursor> {
    match node {
        RsxNode::Element {
            name,
            name_span,
            close_span,
            attributes,
            children,
            ..
        } => {
            if offset >= name_span.start && offset <= name_span.end {
                return Some(SymbolUnderCursor::ElementTag {
                    name: name.clone(),
                    span: *name_span,
                });
            }
            if let Some(cs) = close_span {
                if offset >= cs.start && offset <= cs.end {
                    return Some(SymbolUnderCursor::ElementTag {
                        name: name.clone(),
                        span: *cs,
                    });
                }
            }
            for attr in attributes {
                if offset >= attr.name_span.start && offset <= attr.name_span.end {
                    return Some(SymbolUnderCursor::AttributeName {
                        element_name: name.clone(),
                        attr_name: attr.name.clone(),
                        span: attr.name_span,
                    });
                }
            }
            for child in children {
                if let Some(res) = find_in_node(child, offset) {
                    return Some(res);
                }
            }
            None
        }
        RsxNode::Block { children, .. } => {
            for child in children {
                if let Some(res) = find_in_node(child, offset) {
                    return Some(res);
                }
            }
            None
        }
        _ => None,
    }
}

/// Resolves the definition location for the symbol under cursor.
pub fn find_definition(
    current_uri: &str,
    documents: &HashMap<String, RsxDocument>,
    pos: Position,
) -> Option<Location> {
    let current_doc = documents.get(current_uri)?;
    let symbol = find_symbol_at_position(current_doc, pos)?;

    match symbol {
        SymbolUnderCursor::ElementTag { name, .. } => {
            // 1. Search AST in open documents
            for (uri, doc) in documents {
                if let Some(tree) = &doc.tree {
                    if let Some(loc) = find_ast_definition(tree.root_node(), doc, uri, &name) {
                        return Some(loc);
                    }
                }
            }

            // 2. Fallback: Search textual patterns across open documents
            let fn_patterns = [
                format!("fn {} ", name),
                format!("fn {}(", name),
                format!("struct {} ", name),
                format!("struct {}(", name),
                format!("struct {}{{", name),
            ];

            for (uri, doc) in documents {
                for pattern in &fn_patterns {
                    if let Some(idx) = doc.source.find(pattern) {
                        let name_offset = if pattern.starts_with("fn ") {
                            idx + 3
                        } else {
                            idx + 7
                        };
                        let range = Range::new(
                            doc.source_map.offset_to_position(name_offset),
                            doc.source_map.offset_to_position(name_offset + name.len()),
                        );
                        return Some(Location {
                            uri: uri.clone(),
                            range,
                        });
                    }
                }
            }

            // 3. Fallback: locate first element usage in current document
            for node in &current_doc.nodes {
                if let Some(loc) = find_first_element_occurrence(node, &name, current_uri, &current_doc.source_map) {
                    return Some(loc);
                }
            }
            None
        }
        SymbolUnderCursor::AttributeName { span, .. } => {
            Some(Location {
                uri: current_uri.to_string(),
                range: span.to_range(&current_doc.source_map),
            })
        }
    }
}

fn find_ast_definition(
    node: tree_sitter::Node,
    doc: &RsxDocument,
    uri: &str,
    target_name: &str,
) -> Option<Location> {
    match node.kind() {
        "function_item" | "struct_item" | "enum_item" | "trait_item" | "type_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = doc.source.get(name_node.start_byte()..name_node.end_byte()).unwrap_or("");
                if name == target_name {
                    let range = Range::new(
                        doc.source_map.offset_to_position(name_node.start_byte()),
                        doc.source_map.offset_to_position(name_node.end_byte()),
                    );
                    return Some(Location {
                        uri: uri.to_string(),
                        range,
                    });
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(loc) = find_ast_definition(child, doc, uri, target_name) {
            return Some(loc);
        }
    }
    None
}

fn find_first_element_occurrence(
    node: &RsxNode,
    target_name: &str,
    uri: &str,
    map: &crate::parser::SourceMap,
) -> Option<Location> {
    match node {
        RsxNode::Element {
            name,
            name_span,
            children,
            ..
        } => {
            if name == target_name {
                return Some(Location {
                    uri: uri.to_string(),
                    range: name_span.to_range(map),
                });
            }
            for child in children {
                if let Some(loc) = find_first_element_occurrence(child, target_name, uri, map) {
                    return Some(loc);
                }
            }
            None
        }
        RsxNode::Block { children, .. } => {
            for child in children {
                if let Some(loc) = find_first_element_occurrence(child, target_name, uri, map) {
                    return Some(loc);
                }
            }
            None
        }
        _ => None,
    }
}

/// Collects all references to the symbol under cursor across open documents.
pub fn find_references(
    current_uri: &str,
    documents: &HashMap<String, RsxDocument>,
    pos: Position,
) -> Vec<Location> {
    let mut locations = Vec::new();
    let current_doc = match documents.get(current_uri) {
        Some(d) => d,
        None => return locations,
    };

    let symbol = match find_symbol_at_position(current_doc, pos) {
        Some(s) => s,
        None => return locations,
    };

    match symbol {
        SymbolUnderCursor::ElementTag { name, .. } => {
            for (uri, doc) in documents {
                // 1. Collect from AST elements
                for node in &doc.nodes {
                    collect_element_references(node, &name, uri, &doc.source_map, &mut locations);
                }

                // 2. Collect from Tree-Sitter AST
                if let Some(tree) = &doc.tree {
                    collect_ast_references(tree.root_node(), doc, uri, &name, &mut locations);
                } else {
                    let fn_pattern = format!("fn {}", name);
                    for (offset, _) in doc.source.match_indices(&fn_pattern) {
                        let name_offset = offset + 3;
                        let range = Range::new(
                            doc.source_map.offset_to_position(name_offset),
                            doc.source_map.offset_to_position(name_offset + name.len()),
                        );
                        locations.push(Location {
                            uri: uri.clone(),
                            range,
                        });
                    }
                }
            }
        }
        SymbolUnderCursor::AttributeName { attr_name, .. } => {
            for (uri, doc) in documents {
                for node in &doc.nodes {
                    collect_attribute_references(node, &attr_name, uri, &doc.source_map, &mut locations);
                }
            }
        }
    }

    // Deduplicate locations
    locations.sort_by(|a, b| {
        a.uri.cmp(&b.uri)
            .then_with(|| a.range.start.line.cmp(&b.range.start.line))
            .then_with(|| a.range.start.character.cmp(&b.range.start.character))
    });
    locations.dedup_by(|a, b| a.uri == b.uri && a.range == b.range);

    locations
}

fn collect_ast_references(
    node: tree_sitter::Node,
    doc: &RsxDocument,
    uri: &str,
    target_name: &str,
    out: &mut Vec<Location>,
) {
    if node.kind() == "identifier" || node.kind() == "type_identifier" {
        let text = doc.source.get(node.start_byte()..node.end_byte()).unwrap_or("");
        if text == target_name {
            let range = Range::new(
                doc.source_map.offset_to_position(node.start_byte()),
                doc.source_map.offset_to_position(node.end_byte()),
            );
            out.push(Location {
                uri: uri.to_string(),
                range,
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_ast_references(child, doc, uri, target_name, out);
    }
}

fn collect_element_references(
    node: &RsxNode,
    target_name: &str,
    uri: &str,
    map: &crate::parser::SourceMap,
    out: &mut Vec<Location>,
) {
    match node {
        RsxNode::Element {
            name,
            name_span,
            close_span,
            children,
            ..
        } => {
            if name == target_name {
                out.push(Location {
                    uri: uri.to_string(),
                    range: name_span.to_range(map),
                });
                if let Some(cs) = close_span {
                    out.push(Location {
                        uri: uri.to_string(),
                        range: cs.to_range(map),
                    });
                }
            }
            for child in children {
                collect_element_references(child, target_name, uri, map, out);
            }
        }
        RsxNode::Block { children, .. } => {
            for child in children {
                collect_element_references(child, target_name, uri, map, out);
            }
        }
        _ => {}
    }
}

fn collect_attribute_references(
    node: &RsxNode,
    target_attr: &str,
    uri: &str,
    map: &crate::parser::SourceMap,
    out: &mut Vec<Location>,
) {
    match node {
        RsxNode::Element {
            attributes,
            children,
            ..
        } => {
            for attr in attributes {
                if attr.name == target_attr {
                    out.push(Location {
                        uri: uri.to_string(),
                        range: attr.name_span.to_range(map),
                    });
                }
            }
            for child in children {
                collect_attribute_references(child, target_attr, uri, map, out);
            }
        }
        RsxNode::Block { children, .. } => {
            for child in children {
                collect_attribute_references(child, target_attr, uri, map, out);
            }
        }
        _ => {}
    }
}

/// Computes workspace text edits to rename the symbol under cursor.
pub fn compute_rename(
    current_uri: &str,
    documents: &HashMap<String, RsxDocument>,
    pos: Position,
    new_name: &str,
) -> HashMap<String, Vec<TextEdit>> {
    let mut changes: HashMap<String, Vec<TextEdit>> = HashMap::new();
    let references = find_references(current_uri, documents, pos);

    for loc in references {
        changes
            .entry(loc.uri)
            .or_default()
            .push(TextEdit {
                range: loc.range,
                new_text: new_name.to_string(),
            });
    }

    changes
}

// ---------------------------------------------------------------------------
// Document Symbols (textDocument/documentSymbol)
// ---------------------------------------------------------------------------

/// Computes a hierarchical list of document symbols for outline views.
pub fn compute_document_symbols(doc: &RsxDocument) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();

    // 1. Collect from Tree-Sitter AST
    if let Some(tree) = &doc.tree {
        let root = tree.root_node();
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            collect_ast_symbols(child, doc, &mut symbols);
        }
    }

    // 2. Collect from RSX Element AST
    for node in &doc.nodes {
        if let Some(sym) = collect_rsx_symbol(node, doc) {
            symbols.push(sym);
        }
    }

    symbols
}

fn collect_ast_symbols(node: tree_sitter::Node, doc: &RsxDocument, out: &mut Vec<DocumentSymbol>) {
    let map = &doc.source_map;
    match node.kind() {
        "function_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = doc.source.get(name_node.start_byte()..name_node.end_byte()).unwrap_or("").to_string();
                let detail = node.child_by_field_name("parameters")
                    .map(|p| doc.source.get(p.start_byte()..p.end_byte()).unwrap_or("").to_string());
                let range = Range::new(map.offset_to_position(node.start_byte()), map.offset_to_position(node.end_byte()));
                let selection_range = Range::new(map.offset_to_position(name_node.start_byte()), map.offset_to_position(name_node.end_byte()));

                let mut children = Vec::new();
                if let Some(body) = node.child_by_field_name("body") {
                    let mut cursor = body.walk();
                    for child in body.children(&mut cursor) {
                        collect_ast_symbols(child, doc, &mut children);
                    }
                }
                out.push(DocumentSymbol {
                    name,
                    detail,
                    kind: SymbolKind::Function,
                    range,
                    selection_range,
                    children,
                });
            }
        }
        "struct_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = doc.source.get(name_node.start_byte()..name_node.end_byte()).unwrap_or("").to_string();
                let range = Range::new(map.offset_to_position(node.start_byte()), map.offset_to_position(node.end_byte()));
                let selection_range = Range::new(map.offset_to_position(name_node.start_byte()), map.offset_to_position(name_node.end_byte()));
                out.push(DocumentSymbol {
                    name,
                    detail: Some("struct".to_string()),
                    kind: SymbolKind::Struct,
                    range,
                    selection_range,
                    children: Vec::new(),
                });
            }
        }
        "enum_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = doc.source.get(name_node.start_byte()..name_node.end_byte()).unwrap_or("").to_string();
                let range = Range::new(map.offset_to_position(node.start_byte()), map.offset_to_position(node.end_byte()));
                let selection_range = Range::new(map.offset_to_position(name_node.start_byte()), map.offset_to_position(name_node.end_byte()));

                let mut children = Vec::new();
                if let Some(body) = node.child_by_field_name("body") {
                    let mut cursor = body.walk();
                    for child in body.children(&mut cursor) {
                        if child.kind() == "enum_variant" {
                            if let Some(v_name) = child.child_by_field_name("name") {
                                let v_text = doc.source.get(v_name.start_byte()..v_name.end_byte()).unwrap_or("").to_string();
                                children.push(DocumentSymbol {
                                    name: v_text,
                                    detail: None,
                                    kind: SymbolKind::EnumMember,
                                    range: Range::new(map.offset_to_position(child.start_byte()), map.offset_to_position(child.end_byte())),
                                    selection_range: Range::new(map.offset_to_position(v_name.start_byte()), map.offset_to_position(v_name.end_byte())),
                                    children: Vec::new(),
                                });
                            }
                        }
                    }
                }
                out.push(DocumentSymbol {
                    name,
                    detail: Some("enum".to_string()),
                    kind: SymbolKind::Enum,
                    range,
                    selection_range,
                    children,
                });
            }
        }
        "impl_item" => {
            let type_node = node.child_by_field_name("type");
            let trait_node = node.child_by_field_name("trait");
            let name = match (trait_node, type_node) {
                (Some(tr), Some(ty)) => format!(
                    "{} for {}",
                    doc.source.get(tr.start_byte()..tr.end_byte()).unwrap_or(""),
                    doc.source.get(ty.start_byte()..ty.end_byte()).unwrap_or("")
                ),
                (_, Some(ty)) => doc.source.get(ty.start_byte()..ty.end_byte()).unwrap_or("impl").to_string(),
                _ => "impl".to_string(),
            };
            let range = Range::new(map.offset_to_position(node.start_byte()), map.offset_to_position(node.end_byte()));
            let sel_node = type_node.or(trait_node).unwrap_or(node);
            let selection_range = Range::new(map.offset_to_position(sel_node.start_byte()), map.offset_to_position(sel_node.end_byte()));

            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    collect_ast_symbols(child, doc, &mut children);
                }
            }
            out.push(DocumentSymbol {
                name,
                detail: Some("impl".to_string()),
                kind: SymbolKind::Class,
                range,
                selection_range,
                children,
            });
        }
        "mod_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = doc.source.get(name_node.start_byte()..name_node.end_byte()).unwrap_or("").to_string();
                let range = Range::new(map.offset_to_position(node.start_byte()), map.offset_to_position(node.end_byte()));
                let selection_range = Range::new(map.offset_to_position(name_node.start_byte()), map.offset_to_position(name_node.end_byte()));
                out.push(DocumentSymbol {
                    name,
                    detail: Some("mod".to_string()),
                    kind: SymbolKind::Module,
                    range,
                    selection_range,
                    children: Vec::new(),
                });
            }
        }
        _ => {}
    }
}

fn collect_rsx_symbol(node: &RsxNode, doc: &RsxDocument) -> Option<DocumentSymbol> {
    match node {
        RsxNode::Element { name, name_span, open_span, close_span, attributes, children, .. } => {
            let map = &doc.source_map;
            let full_start = open_span.start;
            let full_end = close_span.as_ref().map(|cs| cs.end).unwrap_or(open_span.end);
            let range = Range::new(map.offset_to_position(full_start), map.offset_to_position(full_end));
            let selection_range = name_span.to_range(map);

            let class_attr = attributes.iter().find(|a| a.name == "class" || a.name == "className")
                .and_then(|a| a.value.as_ref());
            let detail = class_attr.map(|c| format!(".{}", c));

            let mut child_syms = Vec::new();
            for child in children {
                if let Some(cs) = collect_rsx_symbol(child, doc) {
                    child_syms.push(cs);
                }
            }

            Some(DocumentSymbol {
                name: format!("<{}>", name),
                detail,
                kind: SymbolKind::Class,
                range,
                selection_range,
                children: child_syms,
            })
        }
        RsxNode::Block { children, .. } => {
            for child in children {
                if let Some(sym) = collect_rsx_symbol(child, doc) {
                    return Some(sym);
                }
            }
            None
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Workspace Symbols (workspace/symbol)
// ---------------------------------------------------------------------------

/// Symbol Entry indexed per workspace document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSymbolEntry {
    pub name: String,
    pub kind: SymbolKind,
    pub location: Location,
    pub container_name: Option<String>,
}

/// Workspace Symbol Index mapping URIs to discovered symbols.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceIndex {
    pub entries: HashMap<String, Vec<WorkspaceSymbolEntry>>,
}

impl WorkspaceIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn index_document(&mut self, uri: &str, doc: &RsxDocument) {
        let mut symbols = Vec::new();
        let doc_symbols = compute_document_symbols(doc);
        flatten_document_symbols(&doc_symbols, uri, None, &mut symbols);
        self.entries.insert(uri.to_string(), symbols);
    }

    pub fn remove_document(&mut self, uri: &str) {
        self.entries.remove(uri);
    }

    pub fn search(&self, query: &str) -> Vec<SymbolInformation> {
        let q_lower = query.to_lowercase();
        let mut results = Vec::new();

        for symbols in self.entries.values() {
            for sym in symbols {
                let name_lower = sym.name.to_lowercase();
                if query.is_empty() || name_lower.contains(&q_lower) {
                    results.push(SymbolInformation {
                        name: sym.name.clone(),
                        kind: sym.kind,
                        location: sym.location.clone(),
                        container_name: sym.container_name.clone(),
                    });
                }
            }
        }

        results.sort_by(|a, b| {
            let a_exact = a.name.eq_ignore_ascii_case(query);
            let b_exact = b.name.eq_ignore_ascii_case(query);
            if a_exact != b_exact {
                return b_exact.cmp(&a_exact);
            }
            let a_prefix = a.name.to_lowercase().starts_with(&q_lower);
            let b_prefix = b.name.to_lowercase().starts_with(&q_lower);
            if a_prefix != b_prefix {
                return b_prefix.cmp(&a_prefix);
            }
            a.name.cmp(&b.name)
        });

        results
    }
}

fn flatten_document_symbols(
    symbols: &[DocumentSymbol],
    uri: &str,
    container: Option<&str>,
    out: &mut Vec<WorkspaceSymbolEntry>,
) {
    for sym in symbols {
        out.push(WorkspaceSymbolEntry {
            name: sym.name.clone(),
            kind: sym.kind,
            location: Location {
                uri: uri.to_string(),
                range: sym.range,
            },
            container_name: container.map(|s| s.to_string()),
        });
        flatten_document_symbols(&sym.children, uri, Some(&sym.name), out);
    }
}

// ---------------------------------------------------------------------------
// Semantic Tokens (textDocument/semanticTokens/full)
// ---------------------------------------------------------------------------

/// Computes semantic tokens across Tree-Sitter AST and RSX elements.
pub fn compute_semantic_tokens(doc: &RsxDocument) -> SemanticTokens {
    let mut raw_tokens: Vec<(u32, u32, u32, SemanticTokenType, u32)> = Vec::new();
    let map = &doc.source_map;

    if let Some(tree) = &doc.tree {
        collect_ast_semantic_tokens(tree.root_node(), doc, &mut raw_tokens);
    }

    for node in &doc.nodes {
        collect_rsx_semantic_tokens(node, map, &mut raw_tokens);
    }

    raw_tokens.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut data = Vec::with_capacity(raw_tokens.len() * 5);
    let mut prev_line = 0u32;
    let mut prev_char = 0u32;

    for (line, start_char, length, token_type, modifiers) in raw_tokens {
        if length == 0 {
            continue;
        }
        let delta_line = line.saturating_sub(prev_line);
        let delta_char = if delta_line == 0 {
            start_char.saturating_sub(prev_char)
        } else {
            start_char
        };

        data.push(delta_line);
        data.push(delta_char);
        data.push(length);
        data.push(token_type as u32);
        data.push(modifiers);

        prev_line = line;
        prev_char = start_char;
    }

    SemanticTokens {
        result_id: None,
        data,
    }
}

fn collect_ast_semantic_tokens(
    node: tree_sitter::Node,
    doc: &RsxDocument,
    out: &mut Vec<(u32, u32, u32, SemanticTokenType, u32)>,
) {
    let map = &doc.source_map;
    let kind = node.kind();

    let token_info = match kind {
        "fn" | "let" | "mut" | "struct" | "enum" | "impl" | "pub" | "use" | "mod" | "trait"
        | "const" | "static" | "return" | "if" | "else" | "match" | "for" | "while" | "loop"
        | "in" | "as" | "async" | "await" | "where" | "type" => {
            Some((SemanticTokenType::Keyword, 0))
        }
        "string_literal" | "raw_string_literal" => {
            Some((SemanticTokenType::String, 0))
        }
        "integer_literal" | "float_literal" => {
            Some((SemanticTokenType::Number, 0))
        }
        "line_comment" | "block_comment" => {
            Some((SemanticTokenType::Comment, 0))
        }
        "type_identifier" | "primitive_type" => {
            Some((SemanticTokenType::Type, 0))
        }
        "identifier" => {
            let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");
            if parent_kind == "function_item" {
                Some((SemanticTokenType::Function, 1)) // 1 = declaration
            } else if parent_kind == "call_expression" {
                Some((SemanticTokenType::Function, 0))
            } else if parent_kind == "parameter" {
                Some((SemanticTokenType::Parameter, 0))
            } else {
                Some((SemanticTokenType::Variable, 0))
            }
        }
        _ => None,
    };

    if let Some((tok_type, modifiers)) = token_info {
        let pos = map.offset_to_position(node.start_byte());
        let end_pos = map.offset_to_position(node.end_byte());
        let length = if end_pos.line == pos.line {
            end_pos.character.saturating_sub(pos.character)
        } else {
            (node.end_byte() - node.start_byte()) as u32
        };

        if length > 0 {
            out.push((pos.line, pos.character, length, tok_type, modifiers));
        }
    } else {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_ast_semantic_tokens(child, doc, out);
        }
    }
}

fn collect_rsx_semantic_tokens(
    node: &RsxNode,
    map: &crate::parser::SourceMap,
    out: &mut Vec<(u32, u32, u32, SemanticTokenType, u32)>,
) {
    match node {
        RsxNode::Element { name_span, close_span, attributes, children, .. } => {
            let pos = map.offset_to_position(name_span.start);
            let len = (name_span.end - name_span.start) as u32;
            out.push((pos.line, pos.character, len, SemanticTokenType::Class, 0));

            if let Some(cs) = close_span {
                let c_pos = map.offset_to_position(cs.start);
                let c_len = (cs.end - cs.start) as u32;
                out.push((c_pos.line, c_pos.character, c_len, SemanticTokenType::Class, 0));
            }

            for attr in attributes {
                let a_pos = map.offset_to_position(attr.name_span.start);
                let a_len = (attr.name_span.end - attr.name_span.start) as u32;
                out.push((a_pos.line, a_pos.character, a_len, SemanticTokenType::Property, 0));
            }

            for child in children {
                collect_rsx_semantic_tokens(child, map, out);
            }
        }
        RsxNode::Block { children, .. } => {
            for child in children {
                collect_rsx_semantic_tokens(child, map, out);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Inlay Hints (textDocument/inlayHint)
// ---------------------------------------------------------------------------

/// Computes type and parameter inlay hints within the requested document range.
pub fn compute_inlay_hints(doc: &RsxDocument, range: Range) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    if let Some(tree) = &doc.tree {
        collect_ast_inlay_hints(tree.root_node(), doc, range, &mut hints);
    }

    hints
}

fn collect_ast_inlay_hints(
    node: tree_sitter::Node,
    doc: &RsxDocument,
    target_range: Range,
    out: &mut Vec<InlayHint>,
) {
    let map = &doc.source_map;

    if node.kind() == "let_declaration" {
        let type_node = node.child_by_field_name("type");
        let value_node = node.child_by_field_name("value");
        let pattern_node = node.child_by_field_name("pattern");

        if type_node.is_none() {
            if let (Some(pat), Some(val)) = (pattern_node, value_node) {
                let inferred_type = match val.kind() {
                    "integer_literal" => Some(": i32"),
                    "float_literal" => Some(": f64"),
                    "string_literal" => Some(": &str"),
                    "boolean_literal" => Some(": bool"),
                    _ => None,
                };

                if let Some(hint_label) = inferred_type {
                    let pos = map.offset_to_position(pat.end_byte());
                    let in_range = (target_range.start.line == 0 && target_range.end.line == 0 && target_range.end.character == 0)
                        || (pos.line >= target_range.start.line && pos.line <= target_range.end.line);
                    if in_range {
                        out.push(InlayHint {
                            position: pos,
                            label: hint_label.to_string(),
                            kind: Some(InlayHintKind::Type),
                            padding_left: false,
                            padding_right: true,
                        });
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_ast_inlay_hints(child, doc, target_range, out);
    }
}
