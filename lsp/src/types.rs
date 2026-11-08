//! Language Server Protocol (LSP 3.17) Data Types in Pure Rust.

/// Position in a text document expressed as zero-based line and character offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// A range in a text document expressed as (start, end) positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub const fn from_coords(start_line: u32, start_char: u32, end_line: u32, end_char: u32) -> Self {
        Self {
            start: Position::new(start_line, start_char),
            end: Position::new(end_line, end_char),
        }
    }
}

/// Represents a location inside a resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

/// The severity level of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

/// A diagnostic message representing an error, warning, or hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    pub code: Option<String>,
    pub source: Option<String>,
    pub message: String,
}

impl Diagnostic {
    pub fn error(range: Range, message: impl Into<String>) -> Self {
        Self {
            range,
            severity: DiagnosticSeverity::Error,
            code: None,
            source: Some("strata-lsp".to_string()),
            message: message.into(),
        }
    }

    pub fn warning(range: Range, message: impl Into<String>) -> Self {
        Self {
            range,
            severity: DiagnosticSeverity::Warning,
            code: None,
            source: Some("strata-lsp".to_string()),
            message: message.into(),
        }
    }
}

/// Classification of a completion item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompletionItemKind {
    Text = 1,
    Method = 2,
    Function = 3,
    Constructor = 4,
    Field = 5,
    Variable = 6,
    Class = 7,
    Interface = 8,
    Module = 9,
    Property = 10,
    Unit = 11,
    Value = 12,
    Enum = 13,
    Keyword = 14,
    Snippet = 15,
    Color = 16,
    File = 17,
    Reference = 18,
    Folder = 19,
    EnumMember = 20,
    Constant = 21,
    Struct = 22,
    Event = 23,
    Operator = 24,
    TypeParameter = 25,
}

/// An autocomplete item suggested to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
}

impl CompletionItem {
    pub fn new(label: impl Into<String>, kind: CompletionItemKind) -> Self {
        Self {
            label: label.into(),
            kind,
            detail: None,
            documentation: None,
            insert_text: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.documentation = Some(doc.into());
        self
    }

    pub fn with_insert(mut self, insert: impl Into<String>) -> Self {
        self.insert_text = Some(insert.into());
        self
    }
}

/// Content returned when hovering over a symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hover {
    pub contents: String,
    pub range: Option<Range>,
}

/// RGBA floating-point color representation (0.0 to 1.0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorRgba {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

/// Gutter color representation returned by `textDocument/documentColor`.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorInformation {
    pub range: Range,
    pub color: ColorRgba,
}

/// Text edit to update a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

/// Formatted color replacement string returned by `textDocument/colorPresentation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorPresentation {
    pub label: String,
    pub text_edit: Option<TextEdit>,
}

/// Symbol kinds in LSP 3.17.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    String = 15,
    Number = 16,
    Boolean = 17,
    Array = 18,
    Object = 19,
    Key = 20,
    Null = 21,
    EnumMember = 22,
    Struct = 23,
    Event = 24,
    Operator = 25,
    TypeParameter = 26,
}

/// A document symbol representing programming language symbols in an outline hierarchy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSymbol {
    pub name: String,
    pub detail: Option<String>,
    pub kind: SymbolKind,
    pub range: Range,
    pub selection_range: Range,
    pub children: Vec<DocumentSymbol>,
}

/// Symbol information representing a symbol across the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolInformation {
    pub name: String,
    pub kind: SymbolKind,
    pub location: Location,
    pub container_name: Option<String>,
}

/// An individual content change event in textDocument/didChange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDocumentContentChangeEvent {
    pub range: Option<Range>,
    pub range_length: Option<u32>,
    pub text: String,
}

/// Classification of an inlay hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InlayHintKind {
    Type = 1,
    Parameter = 2,
}

/// An inlay hint that is displayed inline with the editor source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlayHint {
    pub position: Position,
    pub label: String,
    pub kind: Option<InlayHintKind>,
    pub padding_left: bool,
    pub padding_right: bool,
}

/// Semantic token types in LSP 3.17.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SemanticTokenType {
    Type = 0,
    Class = 1,
    Enum = 2,
    Interface = 3,
    Struct = 4,
    TypeParameter = 5,
    Parameter = 6,
    Variable = 7,
    Property = 8,
    EnumMember = 9,
    Function = 10,
    Method = 11,
    Keyword = 12,
    Modifier = 13,
    Comment = 14,
    String = 15,
    Number = 16,
    Regexp = 17,
    Operator = 18,
}

/// Encoded semantic tokens payload for LSP 3.17.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemanticTokens {
    pub result_id: Option<String>,
    pub data: Vec<u32>,
}

/// Server capabilities declared during LSP initialization handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerCapabilities {
    pub text_document_sync: u8, // 1 = Full, 2 = Incremental
    pub completion_provider: bool,
    pub hover_provider: bool,
    pub document_color_provider: bool,
    pub definition_provider: bool,
    pub references_provider: bool,
    pub rename_provider: bool,
    pub document_symbol_provider: bool,
    pub workspace_symbol_provider: bool,
    pub semantic_tokens_provider: bool,
    pub inlay_hint_provider: bool,
}

impl Default for ServerCapabilities {
    fn default() -> Self {
        Self {
            text_document_sync: 2, // Incremental
            completion_provider: true,
            hover_provider: true,
            document_color_provider: true,
            definition_provider: true,
            references_provider: true,
            rename_provider: true,
            document_symbol_provider: true,
            workspace_symbol_provider: true,
            semantic_tokens_provider: true,
            inlay_hint_provider: true,
        }
    }
}
