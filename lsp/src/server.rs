//! LSP 3.17 Language Server lifecycle coordinator and request handler.

use std::collections::HashMap;
use crate::color::{find_document_colors, format_color_presentation};
use crate::completion::compute_completions;
use crate::diagnostics::validate_document;
use crate::hover::resolve_hover;
use crate::jsonrpc::{extract_json_number, extract_json_string, JsonRpcMessage};
use crate::navigation::{
    compute_document_symbols, compute_inlay_hints, compute_rename, compute_semantic_tokens,
    find_definition, find_references, WorkspaceIndex,
};
use crate::parser::RsxDocument;
use crate::types::{ColorRgba, Position, Range, ServerCapabilities};

/// State of an active Strata LSP server session.
pub struct LspServer {
    pub documents: HashMap<String, RsxDocument>,
    pub workspace_index: WorkspaceIndex,
    pub initialized: bool,
    pub shutdown_requested: bool,
}

impl Default for LspServer {
    fn default() -> Self {
        Self::new()
    }
}

impl LspServer {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            workspace_index: WorkspaceIndex::new(),
            initialized: false,
            shutdown_requested: false,
        }
    }

    /// Handles an incoming JSON-RPC message, optionally producing a framed response or notification.
    pub fn handle_message(&mut self, msg: JsonRpcMessage) -> Option<String> {
        match msg {
            JsonRpcMessage::Request { id, method, params_json } => {
                let res = self.handle_request(id, &method, &params_json);
                Some(res)
            }
            JsonRpcMessage::Notification { method, params_json } => {
                self.handle_notification(&method, &params_json)
            }
            JsonRpcMessage::Response { .. } => None,
            JsonRpcMessage::Error { .. } => None,
        }
    }

    fn handle_request(&mut self, id: u64, method: &str, params: &str) -> String {
        match method {
            "initialize" => {
                self.initialized = true;
                let caps = ServerCapabilities::default();
                let result_json = format!(
                    "{{\"capabilities\":{{\"textDocumentSync\":{},\"completionProvider\":{{\"triggerCharacters\":[\"<\",\" \",\"-\",\"\\\"\",\":\"]}},\"hoverProvider\":{},\"colorProvider\":{},\"definitionProvider\":{},\"referencesProvider\":{},\"renameProvider\":{},\"documentSymbolProvider\":{},\"workspaceSymbolProvider\":{},\"inlayHintProvider\":{},\"semanticTokensProvider\":{{\"legend\":{{\"tokenTypes\":[\"type\",\"class\",\"enum\",\"interface\",\"struct\",\"typeParameter\",\"parameter\",\"variable\",\"property\",\"enumMember\",\"function\",\"method\",\"keyword\",\"modifier\",\"comment\",\"string\",\"number\",\"regexp\",\"operator\"],\"tokenModifiers\":[\"declaration\",\"definition\",\"readonly\",\"static\",\"deprecated\",\"defaultLibrary\"]}},\"full\":true}}}}}}",
                    caps.text_document_sync, caps.hover_provider, caps.document_color_provider,
                    caps.definition_provider, caps.references_provider, caps.rename_provider,
                    caps.document_symbol_provider, caps.workspace_symbol_provider, caps.inlay_hint_provider
                );
                JsonRpcMessage::format_response(id, &result_json)
            }
            "shutdown" => {
                self.shutdown_requested = true;
                JsonRpcMessage::format_response(id, "null")
            }
            "textDocument/completion" => {
                let uri = extract_uri_from_params(params).unwrap_or_default();
                let line = extract_json_number(params, "line").unwrap_or(0) as u32;
                let character = extract_json_number(params, "character").unwrap_or(0) as u32;

                let items = if let Some(doc) = self.documents.get(&uri) {
                    compute_completions(doc, Position::new(line, character))
                } else {
                    Vec::new()
                };

                let mut items_json = String::from("[");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        items_json.push(',');
                    }
                    items_json.push_str(&format!(
                        "{{\"label\":\"{}\",\"kind\":{},\"detail\":{},\"documentation\":{}}}",
                        item.label,
                        item.kind as u8,
                        item.detail.as_ref().map(|d| format!("\"{}\"", d.replace('"', "\\\""))).unwrap_or_else(|| "null".to_string()),
                        item.documentation.as_ref().map(|d| format!("\"{}\"", d.replace('"', "\\\""))).unwrap_or_else(|| "null".to_string())
                    ));
                }
                items_json.push(']');

                JsonRpcMessage::format_response(id, &items_json)
            }
            "textDocument/hover" => {
                let uri = extract_uri_from_params(params).unwrap_or_default();
                let line = extract_json_number(params, "line").unwrap_or(0) as u32;
                let character = extract_json_number(params, "character").unwrap_or(0) as u32;

                if let Some(doc) = self.documents.get(&uri) {
                    if let Some(hover) = resolve_hover(doc, Position::new(line, character)) {
                        let res = format!(
                            "{{\"contents\":{{\"kind\":\"markdown\",\"value\":\"{}\"}}}}",
                            hover.contents.replace('"', "\\\"").replace('\n', "\\n")
                        );
                        return JsonRpcMessage::format_response(id, &res);
                    }
                }

                JsonRpcMessage::format_response(id, "null")
            }
            "textDocument/documentColor" => {
                let uri = extract_uri_from_params(params).unwrap_or_default();
                let colors = if let Some(doc) = self.documents.get(&uri) {
                    find_document_colors(doc)
                } else {
                    Vec::new()
                };

                let mut colors_json = String::from("[");
                for (i, info) in colors.iter().enumerate() {
                    if i > 0 {
                        colors_json.push(',');
                    }
                    colors_json.push_str(&format!(
                        "{{\"range\":{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}},\"color\":{{\"red\":{:.3},\"green\":{:.3},\"blue\":{:.3},\"alpha\":{:.3}}}}}",
                        info.range.start.line, info.range.start.character,
                        info.range.end.line, info.range.end.character,
                        info.color.red, info.color.green, info.color.blue, info.color.alpha
                    ));
                }
                colors_json.push(']');

                JsonRpcMessage::format_response(id, &colors_json)
            }
            "textDocument/colorPresentation" => {
                let r = extract_json_number(params, "red").unwrap_or(0) as f32 / 255.0;
                let g = extract_json_number(params, "green").unwrap_or(0) as f32 / 255.0;
                let b = extract_json_number(params, "blue").unwrap_or(0) as f32 / 255.0;
                let a = extract_json_number(params, "alpha").unwrap_or(255) as f32 / 255.0;

                let color = ColorRgba { red: r, green: g, blue: b, alpha: a };
                let range = Range::default();
                let presentations = format_color_presentation(color, range);

                let mut pres_json = String::from("[");
                for (i, pres) in presentations.iter().enumerate() {
                    if i > 0 {
                        pres_json.push(',');
                    }
                    pres_json.push_str(&format!("{{\"label\":\"{}\"}}", pres.label));
                }
                pres_json.push(']');

                JsonRpcMessage::format_response(id, &pres_json)
            }
            "textDocument/definition" => {
                let uri = extract_uri_from_params(params).unwrap_or_default();
                let line = extract_json_number(params, "line").unwrap_or(0) as u32;
                let character = extract_json_number(params, "character").unwrap_or(0) as u32;

                if let Some(loc) = find_definition(&uri, &self.documents, Position::new(line, character)) {
                    let res = format!(
                        "{{\"uri\":\"{}\",\"range\":{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}}}}",
                        loc.uri,
                        loc.range.start.line, loc.range.start.character,
                        loc.range.end.line, loc.range.end.character
                    );
                    JsonRpcMessage::format_response(id, &res)
                } else {
                    JsonRpcMessage::format_response(id, "null")
                }
            }
            "textDocument/references" => {
                let uri = extract_uri_from_params(params).unwrap_or_default();
                let line = extract_json_number(params, "line").unwrap_or(0) as u32;
                let character = extract_json_number(params, "character").unwrap_or(0) as u32;

                let locs = find_references(&uri, &self.documents, Position::new(line, character));
                let mut locs_json = String::from("[");
                for (i, loc) in locs.iter().enumerate() {
                    if i > 0 {
                        locs_json.push(',');
                    }
                    locs_json.push_str(&format!(
                        "{{\"uri\":\"{}\",\"range\":{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}}}}",
                        loc.uri,
                        loc.range.start.line, loc.range.start.character,
                        loc.range.end.line, loc.range.end.character
                    ));
                }
                locs_json.push(']');

                JsonRpcMessage::format_response(id, &locs_json)
            }
            "textDocument/rename" => {
                let uri = extract_uri_from_params(params).unwrap_or_default();
                let line = extract_json_number(params, "line").unwrap_or(0) as u32;
                let character = extract_json_number(params, "character").unwrap_or(0) as u32;
                let new_name = extract_json_string(params, "newName").unwrap_or_default();

                let changes = compute_rename(&uri, &self.documents, Position::new(line, character), &new_name);
                let mut changes_json = String::from("{\"changes\":{");
                let mut first_file = true;
                for (doc_uri, edits) in changes {
                    if !first_file {
                        changes_json.push(',');
                    }
                    first_file = false;
                    changes_json.push_str(&format!("\"{}\":[", doc_uri));
                    for (j, edit) in edits.iter().enumerate() {
                        if j > 0 {
                            changes_json.push(',');
                        }
                        changes_json.push_str(&format!(
                            "{{\"range\":{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}},\"newText\":\"{}\"}}",
                            edit.range.start.line, edit.range.start.character,
                            edit.range.end.line, edit.range.end.character,
                            edit.new_text.replace('"', "\\\"")
                        ));
                    }
                    changes_json.push(']');
                }
                changes_json.push_str("}}");

                JsonRpcMessage::format_response(id, &changes_json)
            }
            "textDocument/documentSymbol" => {
                let uri = extract_uri_from_params(params).unwrap_or_default();
                if let Some(doc) = self.documents.get(&uri) {
                    let symbols = compute_document_symbols(doc);
                    let res = serialize_document_symbols(&symbols);
                    JsonRpcMessage::format_response(id, &res)
                } else {
                    JsonRpcMessage::format_response(id, "[]")
                }
            }
            "workspace/symbol" => {
                let query = extract_json_string(params, "query").unwrap_or_default();
                let symbols = self.workspace_index.search(&query);
                let res = serialize_workspace_symbols(&symbols);
                JsonRpcMessage::format_response(id, &res)
            }
            "textDocument/semanticTokens/full" => {
                let uri = extract_uri_from_params(params).unwrap_or_default();
                if let Some(doc) = self.documents.get(&uri) {
                    let tokens = compute_semantic_tokens(doc);
                    let mut data_json = String::from("{\"data\":[");
                    for (i, val) in tokens.data.iter().enumerate() {
                        if i > 0 {
                            data_json.push(',');
                        }
                        data_json.push_str(&val.to_string());
                    }
                    data_json.push_str("]}");
                    JsonRpcMessage::format_response(id, &data_json)
                } else {
                    JsonRpcMessage::format_response(id, "{\"data\":[]}")
                }
            }
            "textDocument/inlayHint" => {
                let uri = extract_uri_from_params(params).unwrap_or_default();
                if let Some(doc) = self.documents.get(&uri) {
                    let range = extract_range_from_params(params).unwrap_or(Range::from_coords(0, 0, u32::MAX, u32::MAX));
                    let hints = compute_inlay_hints(doc, range);
                    let res = serialize_inlay_hints(&hints);
                    JsonRpcMessage::format_response(id, &res)
                } else {
                    JsonRpcMessage::format_response(id, "[]")
                }
            }
            _ => JsonRpcMessage::format_error(Some(id), -32601, &format!("Method '{}' not found", method)),
        }
    }

    fn handle_notification(&mut self, method: &str, params: &str) -> Option<String> {
        match method {
            "initialized" => None,
            "textDocument/didOpen" => {
                let uri = extract_uri_from_params(params)?;
                let text = extract_json_string(params, "text").unwrap_or_default();
                let doc = RsxDocument::parse(&text);
                self.workspace_index.index_document(&uri, &doc);
                let diags = validate_document(&doc);
                self.documents.insert(uri.clone(), doc);
                Some(format_diagnostics_notification(&uri, &diags))
            }
            "textDocument/didChange" => {
                let uri = extract_uri_from_params(params)?;

                // Check for incremental sync contentChanges
                if let Some(changes) = parse_content_changes(params) {
                    if let Some(doc) = self.documents.get_mut(&uri) {
                        doc.apply_incremental_edits(&changes);
                        self.workspace_index.index_document(&uri, doc);
                        let diags = validate_document(doc);
                        return Some(format_diagnostics_notification(&uri, &diags));
                    }
                }

                // Fallback: full document replacement
                let text = extract_json_string(params, "text").unwrap_or_default();
                let doc = RsxDocument::parse(&text);
                self.workspace_index.index_document(&uri, &doc);
                let diags = validate_document(&doc);
                self.documents.insert(uri.clone(), doc);
                Some(format_diagnostics_notification(&uri, &diags))
            }
            "textDocument/didClose" => {
                if let Some(uri) = extract_uri_from_params(params) {
                    self.documents.remove(&uri);
                }
                None
            }
            _ => None,
        }
    }
}

fn extract_uri_from_params(params: &str) -> Option<String> {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(params) {
        if let Some(uri) = val.get("textDocument").and_then(|td| td.get("uri")).and_then(|u| u.as_str()) {
            return Some(uri.to_string());
        }
        if let Some(uri) = val.get("uri").and_then(|u| u.as_str()) {
            return Some(uri.to_string());
        }
    }
    extract_json_string(params, "uri")
}

fn extract_range_from_params(params: &str) -> Option<Range> {
    let val: serde_json::Value = serde_json::from_str(params).ok()?;
    let r = val.get("range")?;
    let start = r.get("start")?;
    let end = r.get("end")?;
    let start_line = start.get("line")?.as_u64()? as u32;
    let start_char = start.get("character")?.as_u64()? as u32;
    let end_line = end.get("line")?.as_u64()? as u32;
    let end_char = end.get("character")?.as_u64()? as u32;
    Some(Range::from_coords(start_line, start_char, end_line, end_char))
}

fn parse_content_changes(params: &str) -> Option<Vec<crate::types::TextDocumentContentChangeEvent>> {
    let val: serde_json::Value = serde_json::from_str(params).ok()?;
    let changes_arr = val.get("contentChanges")?.as_array()?;
    let mut out = Vec::new();

    for item in changes_arr {
        let text = item.get("text")?.as_str()?.to_string();
        let range = if let Some(r) = item.get("range") {
            let start = r.get("start")?;
            let end = r.get("end")?;
            let start_line = start.get("line")?.as_u64()? as u32;
            let start_char = start.get("character")?.as_u64()? as u32;
            let end_line = end.get("line")?.as_u64()? as u32;
            let end_char = end.get("character")?.as_u64()? as u32;
            Some(Range::from_coords(start_line, start_char, end_line, end_char))
        } else {
            None
        };
        let range_length = item.get("rangeLength").and_then(|rl| rl.as_u64()).map(|n| n as u32);
        out.push(crate::types::TextDocumentContentChangeEvent {
            range,
            range_length,
            text,
        });
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn serialize_document_symbols(symbols: &[crate::types::DocumentSymbol]) -> String {
    let mut out = String::from("[");
    for (i, sym) in symbols.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"name\":\"{}\",\"kind\":{},\"detail\":{},\"range\":{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}},\"selectionRange\":{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}},\"children\":{}}}",
            sym.name.replace('"', "\\\""),
            sym.kind as u8,
            sym.detail.as_ref().map(|d| format!("\"{}\"", d.replace('"', "\\\""))).unwrap_or_else(|| "null".to_string()),
            sym.range.start.line, sym.range.start.character,
            sym.range.end.line, sym.range.end.character,
            sym.selection_range.start.line, sym.selection_range.start.character,
            sym.selection_range.end.line, sym.selection_range.end.character,
            serialize_document_symbols(&sym.children)
        ));
    }
    out.push(']');
    out
}

fn serialize_workspace_symbols(symbols: &[crate::types::SymbolInformation]) -> String {
    let mut out = String::from("[");
    for (i, sym) in symbols.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"name\":\"{}\",\"kind\":{},\"location\":{{\"uri\":\"{}\",\"range\":{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}}}},\"containerName\":{}}}",
            sym.name.replace('"', "\\\""),
            sym.kind as u8,
            sym.location.uri,
            sym.location.range.start.line, sym.location.range.start.character,
            sym.location.range.end.line, sym.location.range.end.character,
            sym.container_name.as_ref().map(|c| format!("\"{}\"", c.replace('"', "\\\""))).unwrap_or_else(|| "null".to_string())
        ));
    }
    out.push(']');
    out
}

fn serialize_inlay_hints(hints: &[crate::types::InlayHint]) -> String {
    let mut out = String::from("[");
    for (i, hint) in hints.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"position\":{{\"line\":{},\"character\":{}}},\"label\":\"{}\",\"kind\":{},\"paddingLeft\":{},\"paddingRight\":{}}}",
            hint.position.line, hint.position.character,
            hint.label.replace('"', "\\\""),
            hint.kind.map(|k| k as u8).unwrap_or(1),
            hint.padding_left,
            hint.padding_right
        ));
    }
    out.push(']');
    out
}

fn format_diagnostics_notification(uri: &str, diagnostics: &[crate::types::Diagnostic]) -> String {
    let mut diags_json = String::from("[");
    for (i, diag) in diagnostics.iter().enumerate() {
        if i > 0 {
            diags_json.push(',');
        }
        diags_json.push_str(&format!(
            "{{\"range\":{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}},\"severity\":{},\"source\":\"{}\",\"message\":\"{}\"}}",
            diag.range.start.line,
            diag.range.start.character,
            diag.range.end.line,
            diag.range.end.character,
            diag.severity as u8,
            diag.source.as_deref().unwrap_or("strata-lsp"),
            diag.message.replace('"', "\\\"")
        ));
    }
    diags_json.push(']');

    let params = format!("{{\"uri\":\"{}\",\"diagnostics\":{}}}", uri, diags_json);
    JsonRpcMessage::format_notification("textDocument/publishDiagnostics", &params)
}
