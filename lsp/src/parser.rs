//! Source-Mapped RSX AST Parser with Line/Column Coordinate Resolution.

use crate::types::{Diagnostic, DiagnosticSeverity, Position, Range};

/// A byte-offset span within the document source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Converts this byte span into an LSP Range using the provided SourceMap.
    pub fn to_range(&self, map: &SourceMap) -> Range {
        Range::new(map.offset_to_position(self.start), map.offset_to_position(self.end))
    }
}

/// Source coordinate map indexing newline offsets for O(log N) coordinate lookups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMap {
    source: String,
    line_starts: Vec<usize>,
    total_len: usize,
}

impl SourceMap {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            source: source.to_string(),
            line_starts,
            total_len: source.len(),
        }
    }

    /// Converts a 0-indexed byte offset into an LSP (line, character) position,
    /// where character column is in UTF-16 code units per the LSP specification.
    pub fn offset_to_position(&self, offset: usize) -> Position {
        let clamped = offset.min(self.total_len);
        let line_idx = match self.line_starts.binary_search(&clamped) {
            Ok(exact) => exact,
            Err(insertion) => insertion.saturating_sub(1),
        };

        let start_of_line = self.line_starts[line_idx];
        let end_of_line = self
            .line_starts
            .get(line_idx + 1)
            .copied()
            .unwrap_or(self.total_len);
        let actual_end = clamped.min(end_of_line);

        // Clamp to character boundaries to prevent indexing panic
        let mut valid_end = actual_end;
        while valid_end > start_of_line && !self.source.is_char_boundary(valid_end) {
            valid_end -= 1;
        }

        let slice = &self.source[start_of_line..valid_end];
        let utf16_col: usize = slice.chars().map(|c| c.len_utf16()).sum();

        Position::new(line_idx as u32, utf16_col as u32)
    }

    /// Converts an LSP (line, character) position into a 0-indexed byte offset,
    /// properly translating UTF-16 code units to UTF-8 character boundaries.
    pub fn position_to_offset(&self, pos: Position) -> usize {
        let line_idx = (pos.line as usize).min(self.line_starts.len().saturating_sub(1));
        let line_start = self.line_starts[line_idx];
        let next_line_start = self
            .line_starts
            .get(line_idx + 1)
            .copied()
            .unwrap_or(self.total_len);

        let line_str = &self.source[line_start..next_line_start];
        let target_utf16 = pos.character as usize;
        let mut current_utf16 = 0;
        let mut byte_offset = line_start;

        for ch in line_str.chars() {
            if current_utf16 >= target_utf16 {
                break;
            }
            current_utf16 += ch.len_utf16();
            byte_offset += ch.len_utf8();
        }

        byte_offset.min(self.total_len)
    }
}

/// An attribute on an RSX element (e.g. `class="flex p-4"`, `onclick={...}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsxAttribute {
    pub name: String,
    pub name_span: Span,
    pub value: Option<String>,
    pub value_span: Option<Span>,
    pub is_expression: bool,
}

/// A node within the RSX syntax tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RsxNode {
    Element {
        name: String,
        name_span: Span,
        is_self_closing: bool,
        open_span: Span,
        close_span: Option<Span>,
        attributes: Vec<RsxAttribute>,
        children: Vec<RsxNode>,
    },
    Text {
        content: String,
        span: Span,
    },
    Block {
        expression: String,
        span: Span,
        children: Vec<RsxNode>,
    },
}

/// A parse error detected during AST scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
    pub severity: DiagnosticSeverity,
}

impl ParseError {
    pub fn to_diagnostic(&self, map: &SourceMap) -> Diagnostic {
        Diagnostic {
            range: self.span.to_range(map),
            severity: self.severity,
            code: None,
            source: Some("strata-lsp".to_string()),
            message: self.message.clone(),
        }
    }
}

/// High-level parsed RSX document.
#[derive(Debug, Clone)]
pub struct RsxDocument {
    pub source: String,
    pub source_map: SourceMap,
    pub nodes: Vec<RsxNode>,
    pub errors: Vec<ParseError>,
    pub tree: Option<tree_sitter::Tree>,
}

impl PartialEq for RsxDocument {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source && self.nodes == other.nodes && self.errors == other.errors
    }
}

impl Eq for RsxDocument {}

impl RsxDocument {
    /// Parses an RSX text string into a document representation.
    pub fn parse(source: &str) -> Self {
        Self::parse_with_old_tree(source, None)
    }

    /// Parses an RSX text string with optional previous Tree-Sitter tree for incremental parsing.
    pub fn parse_with_old_tree(source: &str, old_tree: Option<&tree_sitter::Tree>) -> Self {
        let source_map = SourceMap::new(source);
        let mut parser = ParserState::new(source);
        let nodes = parser.parse_nodes();
        let mut errors = parser.errors;

        let mut ts_parser = tree_sitter::Parser::new();
        let tree = if ts_parser.set_language(&tree_sitter_rust::LANGUAGE.into()).is_ok() {
            ts_parser.parse(source, old_tree)
        } else {
            None
        };

        validate_rsx_blocks(&nodes, &mut errors);

        Self {
            source: source.to_string(),
            source_map,
            nodes,
            errors,
            tree,
        }
    }

    /// Incrementally updates the document with differential text edits.
    pub fn apply_incremental_edits(
        &mut self,
        changes: &[crate::types::TextDocumentContentChangeEvent],
    ) {
        let mut ts_parser = tree_sitter::Parser::new();
        let has_ts = ts_parser.set_language(&tree_sitter_rust::LANGUAGE.into()).is_ok();

        for change in changes {
            if let Some(range) = change.range {
                let start_byte = self.source_map.position_to_offset(range.start);
                let old_end_byte = self.source_map.position_to_offset(range.end);
                let new_end_byte = start_byte + change.text.len();

                let start_position = tree_sitter::Point {
                    row: range.start.line as usize,
                    column: range.start.character as usize,
                };
                let old_end_position = tree_sitter::Point {
                    row: range.end.line as usize,
                    column: range.end.character as usize,
                };

                let mut new_row = start_position.row;
                let mut new_col = start_position.column;
                for b in change.text.bytes() {
                    if b == b'\n' {
                        new_row += 1;
                        new_col = 0;
                    } else {
                        new_col += 1;
                    }
                }
                let new_end_position = tree_sitter::Point {
                    row: new_row,
                    column: new_col,
                };

                if let Some(tree) = &mut self.tree {
                    let input_edit = tree_sitter::InputEdit {
                        start_byte,
                        old_end_byte,
                        new_end_byte,
                        start_position,
                        old_end_position,
                        new_end_position,
                    };
                    tree.edit(&input_edit);
                }

                let clamped_start = start_byte.min(self.source.len());
                let clamped_end = old_end_byte.min(self.source.len()).max(clamped_start);
                self.source.replace_range(clamped_start..clamped_end, &change.text);
            } else {
                self.source = change.text.clone();
                self.tree = None;
            }
        }

        self.source_map = SourceMap::new(&self.source);
        let mut parser_state = ParserState::new(&self.source);
        self.nodes = parser_state.parse_nodes();
        self.errors = parser_state.errors;

        if has_ts {
            self.tree = ts_parser.parse(&self.source, self.tree.as_ref());
        }

        validate_rsx_blocks(&self.nodes, &mut self.errors);
    }
}

/// Validates Rust expressions inside RSX blocks and attribute bindings.
fn validate_rsx_blocks(nodes: &[RsxNode], errors: &mut Vec<ParseError>) {
    for node in nodes {
        match node {
            RsxNode::Block { expression, span, children } => {
                if children.is_empty() && !expression.trim().is_empty() {
                    let expr_start = span.start + 1; // skip leading '{'
                    let mut expr_errors = validate_rust_expression(expression, expr_start);
                    errors.append(&mut expr_errors);
                }
                validate_rsx_blocks(children, errors);
            }
            RsxNode::Element { children, attributes, .. } => {
                for attr in attributes {
                    if attr.is_expression {
                        if let Some(val) = &attr.value {
                            if let Some(val_span) = attr.value_span {
                                let mut expr_errors = validate_rust_expression(val, val_span.start);
                                errors.append(&mut expr_errors);
                            }
                        }
                    }
                }
                validate_rsx_blocks(children, errors);
            }
            _ => {}
        }
    }
}

/// Validates a Rust expression using Tree-Sitter, emitting ParseError for syntax errors.
pub fn validate_rust_expression(expr: &str, expr_start: usize) -> Vec<ParseError> {
    let mut errors = Vec::new();
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return errors;
    }

    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&tree_sitter_rust::LANGUAGE.into()).is_err() {
        return errors;
    }

    let prefix = "let _ = { ";
    let suffix = " };";
    let wrapped = format!("{}{}{}", prefix, expr, suffix);
    let prefix_len = prefix.len();

    if let Some(tree) = parser.parse(&wrapped, None) {
        if tree.root_node().has_error() {
            collect_syntax_errors(&tree.root_node(), expr, expr_start, prefix_len, &mut errors);
        }
    }

    errors
}

fn collect_syntax_errors(
    node: &tree_sitter::Node,
    expr: &str,
    expr_start: usize,
    prefix_len: usize,
    out: &mut Vec<ParseError>,
) {
    if node.is_error() || node.is_missing() {
        let sb = node.start_byte();
        let eb = node.end_byte();
        if sb >= prefix_len && sb < prefix_len + expr.len() {
            let rel_start = sb - prefix_len;
            let rel_end = (eb.saturating_sub(prefix_len)).min(expr.len()).max(rel_start + 1);
            let bad_snippet = expr.get(rel_start..rel_end).unwrap_or("syntax error");
            out.push(ParseError {
                message: format!("Syntax error in RSX expression: unexpected '{}'", bad_snippet),
                span: Span::new(expr_start + rel_start, expr_start + rel_end),
                severity: DiagnosticSeverity::Error,
            });
            return;
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.has_error() {
            collect_syntax_errors(&child, expr, expr_start, prefix_len, out);
        }
    }
}

// ---------------------------------------------------------------------------
// Recursive-Descent Lexer & Parser Implementation
// ---------------------------------------------------------------------------

struct ParserState<'a> {
    source: &'a str,
    bytes: &'a [u8],
    cursor: usize,
    errors: Vec<ParseError>,
}

impl<'a> ParserState<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            cursor: 0,
            errors: Vec::new(),
        }
    }

    fn is_eof(&self) -> bool {
        self.cursor >= self.bytes.len()
    }

    fn skip_whitespace(&mut self) {
        while !self.is_eof() && self.bytes[self.cursor].is_ascii_whitespace() {
            self.cursor += 1;
        }
    }

    fn parse_nodes(&mut self) -> Vec<RsxNode> {
        let mut nodes = Vec::new();
        while !self.is_eof() {
            self.skip_whitespace();
            if self.is_eof() {
                break;
            }

            if self.bytes[self.cursor] == b'<' {
                if self.cursor + 1 < self.bytes.len() && self.bytes[self.cursor + 1] == b'/' {
                    // Stray closing tag
                    break;
                }
                if let Some(elem) = self.parse_element() {
                    nodes.push(elem);
                } else {
                    self.cursor += 1;
                }
            } else if self.bytes[self.cursor] == b'{' {
                if let Some(block) = self.parse_block() {
                    nodes.push(block);
                } else {
                    self.cursor += 1;
                }
            } else {
                let text = self.parse_text();
                if !text.content.trim().is_empty() {
                    nodes.push(RsxNode::Text {
                        content: text.content,
                        span: text.span,
                    });
                }
            }
        }
        nodes
    }

    fn parse_element(&mut self) -> Option<RsxNode> {
        let open_start = self.cursor;
        self.cursor += 1; // skip '<'
        self.skip_whitespace();

        // Tag Name
        let name_start = self.cursor;
        while !self.is_eof()
            && (self.bytes[self.cursor].is_ascii_alphanumeric()
                || self.bytes[self.cursor] == b'_'
                || self.bytes[self.cursor] == b'-'
                || self.bytes[self.cursor] == b':')
        {
            self.cursor += 1;
        }
        let name_end = self.cursor;
        if name_start == name_end {
            self.errors.push(ParseError {
                message: "Expected tag name after '<'".to_string(),
                span: Span::new(open_start, self.cursor),
                severity: DiagnosticSeverity::Error,
            });
            return None;
        }
        let tag_name = self.source[name_start..name_end].to_string();
        let name_span = Span::new(name_start, name_end);

        // Attributes
        let mut attributes = Vec::new();
        let mut is_self_closing = false;

        loop {
            self.skip_whitespace();
            if self.is_eof() {
                self.errors.push(ParseError {
                    message: format!("Unclosed tag '<{}>'", tag_name),
                    span: Span::new(open_start, self.cursor),
                    severity: DiagnosticSeverity::Error,
                });
                return None;
            }

            if self.bytes[self.cursor] == b'/' && self.cursor + 1 < self.bytes.len() && self.bytes[self.cursor + 1] == b'>' {
                is_self_closing = true;
                self.cursor += 2;
                break;
            } else if self.bytes[self.cursor] == b'>' {
                self.cursor += 1;
                break;
            } else {
                if let Some(attr) = self.parse_attribute() {
                    attributes.push(attr);
                } else {
                    self.cursor += 1;
                }
            }
        }

        let open_span = Span::new(open_start, self.cursor);

        // Void elements in HTML that don't need closing
        let is_void_element = matches!(
            tag_name.to_ascii_lowercase().as_str(),
            "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr"
        );

        if is_self_closing || is_void_element {
            return Some(RsxNode::Element {
                name: tag_name,
                name_span,
                is_self_closing: true,
                open_span,
                close_span: None,
                attributes,
                children: Vec::new(),
            });
        }

        // Parse Children
        let children = self.parse_nodes();

        // Closing Tag
        self.skip_whitespace();
        let mut close_span = None;

        if !self.is_eof() && self.bytes[self.cursor] == b'<' && self.cursor + 1 < self.bytes.len() && self.bytes[self.cursor + 1] == b'/' {
            let close_start = self.cursor;
            self.cursor += 2; // skip '</'
            self.skip_whitespace();

            let cname_start = self.cursor;
            while !self.is_eof()
                && (self.bytes[self.cursor].is_ascii_alphanumeric()
                    || self.bytes[self.cursor] == b'_'
                    || self.bytes[self.cursor] == b'-'
                    || self.bytes[self.cursor] == b':')
            {
                self.cursor += 1;
            }
            let cname = &self.source[cname_start..self.cursor];

            self.skip_whitespace();
            if !self.is_eof() && self.bytes[self.cursor] == b'>' {
                self.cursor += 1;
            }

            let close_end = self.cursor;
            close_span = Some(Span::new(close_start, close_end));

            if cname != tag_name {
                self.errors.push(ParseError {
                    message: format!("Mismatched closing tag: expected '</{}>', found '</{}>'", tag_name, cname),
                    span: Span::new(close_start, close_end),
                    severity: DiagnosticSeverity::Error,
                });
            }
        } else {
            self.errors.push(ParseError {
                message: format!("Unclosed tag '<{}>': missing closing '</{}>'", tag_name, tag_name),
                span: open_span,
                severity: DiagnosticSeverity::Error,
            });
        }

        Some(RsxNode::Element {
            name: tag_name,
            name_span,
            is_self_closing: false,
            open_span,
            close_span,
            attributes,
            children,
        })
    }

    fn parse_attribute(&mut self) -> Option<RsxAttribute> {
        let name_start = self.cursor;
        while !self.is_eof()
            && (self.bytes[self.cursor].is_ascii_alphanumeric()
                || self.bytes[self.cursor] == b'_'
                || self.bytes[self.cursor] == b'-'
                || self.bytes[self.cursor] == b':')
        {
            self.cursor += 1;
        }
        let name_end = self.cursor;
        if name_start == name_end {
            return None;
        }
        let name = self.source[name_start..name_end].to_string();
        let name_span = Span::new(name_start, name_end);

        self.skip_whitespace();
        if self.is_eof() || self.bytes[self.cursor] != b'=' {
            // Boolean attribute without value
            return Some(RsxAttribute {
                name,
                name_span,
                value: None,
                value_span: None,
                is_expression: false,
            });
        }

        self.cursor += 1; // skip '='
        self.skip_whitespace();

        if self.is_eof() {
            return None;
        }

        if self.bytes[self.cursor] == b'"' || self.bytes[self.cursor] == b'\'' {
            let quote = self.bytes[self.cursor];
            self.cursor += 1;
            let val_start = self.cursor;
            while !self.is_eof() && self.bytes[self.cursor] != quote {
                self.cursor += 1;
            }
            let val_end = self.cursor;
            if !self.is_eof() {
                self.cursor += 1; // skip closing quote
            }
            let value = self.source[val_start..val_end].to_string();
            let value_span = Some(Span::new(val_start, val_end));

            Some(RsxAttribute {
                name,
                name_span,
                value: Some(value),
                value_span,
                is_expression: false,
            })
        } else if self.bytes[self.cursor] == b'{' {
            self.cursor += 1;
            let val_start = self.cursor;
            let mut depth = 1;
            while !self.is_eof() && depth > 0 {
                if self.bytes[self.cursor] == b'{' {
                    depth += 1;
                } else if self.bytes[self.cursor] == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                self.cursor += 1;
            }
            let val_end = self.cursor;
            if !self.is_eof() {
                self.cursor += 1; // skip '}'
            }
            let value = self.source[val_start..val_end].to_string();
            let value_span = Some(Span::new(val_start, val_end));

            Some(RsxAttribute {
                name,
                name_span,
                value: Some(value),
                value_span,
                is_expression: true,
            })
        } else {
            None
        }
    }

    fn parse_block(&mut self) -> Option<RsxNode> {
        let start = self.cursor;
        self.cursor += 1; // skip '{'
        let mut depth = 1;
        let expr_start = self.cursor;

        while !self.is_eof() && depth > 0 {
            if self.bytes[self.cursor] == b'{' {
                depth += 1;
            } else if self.bytes[self.cursor] == b'}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            self.cursor += 1;
        }

        let expr_end = self.cursor;
        if !self.is_eof() {
            self.cursor += 1; // skip '}'
        }
        let end = self.cursor;

        let expression = self.source[expr_start..expr_end].to_string();

        let mut children = Vec::new();
        let mut inner = ParserState::new(self.source);
        inner.cursor = expr_start;
        while inner.cursor < expr_end {
            inner.skip_whitespace();
            if inner.cursor >= expr_end {
                break;
            }
            if inner.bytes[inner.cursor] == b'<' {
                if inner.cursor + 1 < expr_end && inner.bytes[inner.cursor + 1] == b'/' {
                    break;
                }
                if let Some(elem) = inner.parse_element() {
                    children.push(elem);
                } else {
                    inner.cursor += 1;
                }
            } else if inner.bytes[inner.cursor] == b'{' {
                if let Some(blk) = inner.parse_block() {
                    children.push(blk);
                } else {
                    inner.cursor += 1;
                }
            } else {
                inner.cursor += 1;
            }
        }

        Some(RsxNode::Block {
            expression,
            span: Span::new(start, end),
            children,
        })
    }

    fn parse_text(&mut self) -> ParsedText {
        let start = self.cursor;
        while !self.is_eof() && self.bytes[self.cursor] != b'<' && self.bytes[self.cursor] != b'{' {
            self.cursor += 1;
        }
        let end = self.cursor;
        ParsedText {
            content: self.source[start..end].to_string(),
            span: Span::new(start, end),
        }
    }
}

struct ParsedText {
    content: String,
    span: Span,
}
