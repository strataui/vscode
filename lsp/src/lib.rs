//! Strata Language Server Protocol (LSP 3.17) implementation in pure Rust.

pub mod types;
pub mod jsonrpc;
pub mod parser;
pub mod diagnostics;
pub mod completion;
pub mod hover;
pub mod color;
pub mod navigation;
pub mod server;

pub use types::*;
pub use jsonrpc::{encode_framed_message, parse_content_length, JsonRpcMessage};
pub use parser::{ParseError, RsxAttribute, RsxDocument, RsxNode, SourceMap, Span};
pub use diagnostics::validate_document;
pub use completion::compute_completions;
pub use hover::resolve_hover;
pub use color::{find_document_colors, format_color_presentation};
pub use navigation::{
    compute_document_symbols, compute_inlay_hints, compute_rename, compute_semantic_tokens,
    find_definition, find_references, find_symbol_at_position, SymbolUnderCursor,
    WorkspaceIndex, WorkspaceSymbolEntry,
};
pub use server::LspServer;
