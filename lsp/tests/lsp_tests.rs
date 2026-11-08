//! Integration and Unit Tests for Strata Language Server (strata-lsp).

use strata_lsp::*;

#[test]
fn test_jsonrpc_framing_and_parsing() {
    let header = "Content-Length: 42\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n";
    assert_eq!(parse_content_length(header), Some(42));

    let payload = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}";
    let framed = encode_framed_message(payload);
    assert!(framed.starts_with("Content-Length: "));
    assert!(framed.contains("\r\n\r\n"));

    let msg = JsonRpcMessage::parse(payload).expect("Failed to parse JSON-RPC request");
    match msg {
        JsonRpcMessage::Request { id, method, .. } => {
            assert_eq!(id, 1);
            assert_eq!(method, "initialize");
        }
        _ => panic!("Expected Request variant"),
    }
}

#[test]
fn test_rsx_ast_parser_and_source_map() {
    let source = "<div class=\"flex p-4\" id=\"main\">\n    <span class=\"text-white\">{\"Hello Strata\"}</span>\n</div>";
    let doc = RsxDocument::parse(source);

    assert_eq!(doc.errors.len(), 0, "Balanced RSX must have 0 parse errors");
    assert_eq!(doc.nodes.len(), 1);

    if let RsxNode::Element { name, attributes, children, .. } = &doc.nodes[0] {
        assert_eq!(name, "div");
        assert_eq!(attributes.len(), 2);
        assert_eq!(attributes[0].name, "class");
        assert_eq!(attributes[0].value.as_deref(), Some("flex p-4"));
        assert_eq!(attributes[1].name, "id");
        assert_eq!(attributes[1].value.as_deref(), Some("main"));

        assert_eq!(children.len(), 1);
        if let RsxNode::Element { name: child_name, .. } = &children[0] {
            assert_eq!(child_name, "span");
        } else {
            panic!("Expected child Element span");
        }
    } else {
        panic!("Expected root Element node");
    }

    // Verify SourceMap line/character coordinate translation
    let pos_line0 = doc.source_map.offset_to_position(5);
    assert_eq!(pos_line0.line, 0);
    assert_eq!(pos_line0.character, 5);

    let offset_line1 = source.find("span").unwrap();
    let pos_line1 = doc.source_map.offset_to_position(offset_line1);
    assert_eq!(pos_line1.line, 1);
}

#[test]
fn test_diagnostics_unclosed_and_mismatched_tags() {
    // 1. Mismatched tag test
    let mismatched = "<div class=\"box\"></span>";
    let doc_mismatch = RsxDocument::parse(mismatched);
    let diags_mismatch = validate_document(&doc_mismatch);
    assert!(!diags_mismatch.is_empty(), "Must emit diagnostic for mismatched closing tag");
    assert!(diags_mismatch.iter().any(|d| d.message.contains("Mismatched closing tag")));

    // 2. Unclosed tag test
    let unclosed = "<div><button>Click";
    let doc_unclosed = RsxDocument::parse(unclosed);
    let diags_unclosed = validate_document(&doc_unclosed);
    assert!(!diags_unclosed.is_empty(), "Must emit diagnostic for unclosed tag");

    // 3. Duplicate attribute test
    let duplicate_attr = "<div class=\"p-4\" class=\"p-8\"></div>";
    let doc_dup = RsxDocument::parse(duplicate_attr);
    let diags_dup = validate_document(&doc_dup);
    assert!(diags_dup.iter().any(|d| d.message.contains("Duplicate attribute 'class'")));
}

#[test]
fn test_tailwind_autocomplete_and_hover() {
    // Completion test inside class attribute
    let doc = RsxDocument::parse("<div class=\"p-");
    let pos = Position::new(0, 14); // after "p-"
    let completions = compute_completions(&doc, pos);

    assert!(!completions.is_empty(), "Must suggest Tailwind utilities for p-");
    assert!(completions.iter().any(|c| c.label == "p-4"));
    assert!(completions.iter().any(|c| c.label == "px-4"));

    // Tag completion test
    let doc_tag = RsxDocument::parse("<Canv");
    let pos_tag = Position::new(0, 5);
    let tag_completions = compute_completions(&doc_tag, pos_tag);
    assert!(tag_completions.iter().any(|c| c.label == "Canvas3D"));

    // Hover test on Tailwind class
    let doc_hover = RsxDocument::parse("<div class=\"p-4\"></div>");
    let hover_pos = Position::new(0, 13); // on "p-4"
    let hover = resolve_hover(&doc_hover, hover_pos).expect("Expected hover result");
    assert!(hover.contents.contains("padding: 1rem;"));
}

#[test]
fn test_document_color_provider() {
    let source = "<div class=\"bg-[#08090a] text-white\">\n    <button class=\"bg-emerald-500\"></button>\n</div>";
    let doc = RsxDocument::parse(source);
    let colors = find_document_colors(&doc);

    assert!(!colors.is_empty(), "Must discover colors in document");

    // Check hex color #08090a
    let hex_color = colors.iter().find(|c| {
        let r = (c.color.red * 255.0).round() as u8;
        let g = (c.color.green * 255.0).round() as u8;
        let b = (c.color.blue * 255.0).round() as u8;
        r == 8 && g == 9 && b == 10
    });
    assert!(hex_color.is_some(), "Must detect #08090a hex color");

    // Check Tailwind color bg-emerald-500
    let emerald = colors.iter().find(|c| {
        let g = (c.color.green * 255.0).round() as u8;
        g == 185
    });
    assert!(emerald.is_some(), "Must detect bg-emerald-500 class");

    // Presentation format
    let presentations = format_color_presentation(
        ColorRgba { red: 0.031, green: 0.035, blue: 0.039, alpha: 1.0 },
        Range::default(),
    );
    assert!(presentations.iter().any(|p| p.label.starts_with('#')));
}

#[test]
fn test_lsp_server_lifecycle_and_stdio() {
    let mut server = LspServer::new();

    // 1. Initialize Request
    let init_req = JsonRpcMessage::Request {
        id: 1,
        method: "initialize".to_string(),
        params_json: "{}".to_string(),
    };
    let init_res = server.handle_message(init_req).expect("Expected initialize response");
    assert!(init_res.contains("\"capabilities\""));
    assert!(init_res.contains("\"completionProvider\""));
    assert!(server.initialized);

    // 2. textDocument/didOpen Notification
    let did_open = JsonRpcMessage::Notification {
        method: "textDocument/didOpen".to_string(),
        params_json: "{\"uri\":\"file:///test.rsx\",\"text\":\"<div class=\\\"flex\\\"><span></span></div>\"}".to_string(),
    };
    let diag_notif = server.handle_message(did_open).expect("Expected diagnostics notification");
    assert!(diag_notif.contains("textDocument/publishDiagnostics"));

    // 3. textDocument/completion Request
    let comp_req = JsonRpcMessage::Request {
        id: 2,
        method: "textDocument/completion".to_string(),
        params_json: "{\"uri\":\"file:///test.rsx\",\"line\":0,\"character\":5}".to_string(),
    };
    let comp_res = server.handle_message(comp_req).expect("Expected completion response");
    assert!(comp_res.contains("\"result\":["));

    // 4. Shutdown Request
    let shutdown_req = JsonRpcMessage::Request {
        id: 3,
        method: "shutdown".to_string(),
        params_json: "{}".to_string(),
    };
    let shutdown_res = server.handle_message(shutdown_req).expect("Expected shutdown response");
    assert!(shutdown_res.contains("\"result\":null"));
    assert!(server.shutdown_requested);
}

#[test]
fn test_multibyte_utf8_emoji_character_boundary_safety() {
    // String containing 4-byte emoji (🦀), 3-byte CJK (你好), 2-byte Cyrillic (Привет), followed by hex and tailwind colors
    let source = "<!-- 🦀 Rusty Crab 🚀 -->\n<div class=\"#ff0000 bg-black\">\n    <span>你好 мир #00ff00 text-white</span>\n</div>";
    let doc = RsxDocument::parse(source);

    // Color extraction must not panic on multibyte UTF-8 boundaries
    let colors = find_document_colors(&doc);
    assert_eq!(colors.len(), 4, "Must discover all 4 color tokens without boundary panics");

    // Coordinate conversion round-trip around emojis
    let emoji_offset = source.find('🦀').unwrap();
    let pos = doc.source_map.offset_to_position(emoji_offset);
    assert_eq!(pos.line, 0);
    // '<!-- ' is 5 chars. '🦀' starts at UTF-16 code unit 5
    assert_eq!(pos.character, 5);

    let back_offset = doc.source_map.position_to_offset(pos);
    assert_eq!(back_offset, emoji_offset);
}

#[test]
fn test_navigation_definition_references_and_rename() {
    let mut server = LspServer::new();

    let doc_source = r#"
pub fn Header(props: HeaderProps) -> Rsx {
    <div class="header">
        <Header />
    </div>
}
"#;

    // 1. Open document
    let did_open = JsonRpcMessage::Notification {
        method: "textDocument/didOpen".to_string(),
        params_json: format!("{{\"uri\":\"file:///app.rsx\",\"text\":\"{}\"}}", doc_source.replace('\n', "\\n").replace('"', "\\\"")),
    };
    server.handle_message(did_open);

    // 2. Definition lookup on `<Header />` (line 3, char 9)
    let def_req = JsonRpcMessage::Request {
        id: 10,
        method: "textDocument/definition".to_string(),
        params_json: "{\"uri\":\"file:///app.rsx\",\"line\":3,\"character\":10}".to_string(),
    };
    let def_res = server.handle_message(def_req).expect("Expected definition response");
    assert!(def_res.contains("\"uri\":\"file:///app.rsx\""));
    assert!(def_res.contains("\"line\":1")); // fn Header is at line 1

    // 3. Find References on `Header`
    let refs_req = JsonRpcMessage::Request {
        id: 11,
        method: "textDocument/references".to_string(),
        params_json: "{\"uri\":\"file:///app.rsx\",\"line\":3,\"character\":10}".to_string(),
    };
    let refs_res = server.handle_message(refs_req).expect("Expected references response");
    assert!(refs_res.contains("\"file:///app.rsx\""));

    // 4. Rename Refactoring: Rename Header -> MainHeader
    let rename_req = JsonRpcMessage::Request {
        id: 12,
        method: "textDocument/rename".to_string(),
        params_json: "{\"uri\":\"file:///app.rsx\",\"line\":3,\"character\":10,\"newName\":\"MainHeader\"}".to_string(),
    };
    let rename_res = server.handle_message(rename_req).expect("Expected rename response");
    assert!(rename_res.contains("\"newText\":\"MainHeader\""));
    assert!(rename_res.contains("\"changes\":{\"file:///app.rsx\":["));
}

#[test]
fn test_tree_sitter_ast_expression_syntax_diagnostics_and_recovery() {
    let mut server = LspServer::new();

    // 1. Valid expressions inside RSX blocks
    let valid_source = "<div class=\"p-4\">\n    <span>{ count + 1 }</span>\n    <button onclick={ || count + 1 }>Click</button>\n</div>";
    let valid_doc = RsxDocument::parse(valid_source);
    let valid_diags = validate_document(&valid_doc);
    assert!(valid_diags.is_empty(), "Valid Rust expressions in RSX must emit 0 diagnostics");

    // 2. Invalid syntax inside block expression `{ 1 + * }`
    let invalid_block = "<div class=\"p-4\">\n    <span>{ 1 + * }</span>\n</div>";
    let invalid_doc = RsxDocument::parse(invalid_block);
    let diags = validate_document(&invalid_doc);
    assert!(!diags.is_empty(), "Must emit diagnostic for syntax error inside RSX block expression");
    assert!(diags.iter().any(|d| d.message.contains("Syntax error in RSX expression")));
    let err_diag = diags.iter().find(|d| d.message.contains("Syntax error in RSX expression")).unwrap();
    assert_eq!(err_diag.range.start.line, 1);

    // 3. Document symbols request
    let open_msg = JsonRpcMessage::Notification {
        method: "textDocument/didOpen".to_string(),
        params_json: format!("{{\"uri\":\"file:///view.rsx\",\"text\":\"{}\"}}", valid_source.replace('\n', "\\n").replace('"', "\\\"")),
    };
    server.handle_message(open_msg);

    let sym_req = JsonRpcMessage::Request {
        id: 20,
        method: "textDocument/documentSymbol".to_string(),
        params_json: "{\"uri\":\"file:///view.rsx\"}".to_string(),
    };
    let sym_res = server.handle_message(sym_req).expect("Expected documentSymbol response");
    assert!(sym_res.contains("\"name\":\"<div>\""));
    assert!(sym_res.contains("\"name\":\"<span>\""));
    assert!(sym_res.contains("\"name\":\"<button>\""));
}

#[test]
fn test_incremental_document_synchronization_differential_edits() {
    let mut server = LspServer::new();

    // 1. Verify capability declares Incremental sync (2)
    let init_req = JsonRpcMessage::Request {
        id: 1,
        method: "initialize".to_string(),
        params_json: "{}".to_string(),
    };
    let init_res = server.handle_message(init_req).unwrap();
    assert!(init_res.contains("\"textDocumentSync\":2"));

    // 2. Open document
    let uri = "file:///src/editor.rsx";
    let initial_text = "<div>Hello</div>";
    server.handle_message(JsonRpcMessage::Notification {
        method: "textDocument/didOpen".to_string(),
        params_json: format!("{{\"uri\":\"{}\",\"text\":\"{}\"}}", uri, initial_text),
    });

    let doc = server.documents.get(uri).expect("Document must exist in server state");
    assert_eq!(doc.source, "<div>Hello</div>");

    // 3. Send incremental edit replacing "Hello" (cols 5..10) with "World"
    let did_change = JsonRpcMessage::Notification {
        method: "textDocument/didChange".to_string(),
        params_json: format!(
            "{{\"textDocument\":{{\"uri\":\"{}\",\"version\":2}},\"contentChanges\":[{{\"range\":{{\"start\":{{\"line\":0,\"character\":5}},\"end\":{{\"line\":0,\"character\":10}}}},\"rangeLength\":5,\"text\":\"World\"}}]}}",
            uri
        ),
    };
    server.handle_message(did_change);

    let updated_doc = server.documents.get(uri).expect("Document must exist");
    assert_eq!(updated_doc.source, "<div>World</div>", "Incremental edit must update buffer");

    // 4. Send second incremental edit inserting a broken expression `{ 1 + * }`
    let did_change_err = JsonRpcMessage::Notification {
        method: "textDocument/didChange".to_string(),
        params_json: format!(
            "{{\"textDocument\":{{\"uri\":\"{}\",\"version\":3}},\"contentChanges\":[{{\"range\":{{\"start\":{{\"line\":0,\"character\":5}},\"end\":{{\"line\":0,\"character\":10}}}},\"rangeLength\":5,\"text\":\"{{ 1 + * }}\"}}]}}",
            uri
        ),
    };
    let diag_notif = server.handle_message(did_change_err).expect("Must emit diagnostics notification");
    assert!(diag_notif.contains("Syntax error in RSX expression"));
}

#[test]
fn test_workspace_symbol_search_and_cross_file_references() {
    let mut server = LspServer::new();

    let button_file = r#"
pub struct ButtonProps {
    pub label: String,
}

pub fn Button(props: ButtonProps) -> Rsx {
    <button class="btn">{ props.label }</button>
}
"#;

    let app_file = r#"
pub fn App() -> Rsx {
    <div class="container">
        <Button label="Submit" />
    </div>
}
"#;

    // Open both files in server
    server.handle_message(JsonRpcMessage::Notification {
        method: "textDocument/didOpen".to_string(),
        params_json: format!("{{\"uri\":\"file:///src/button.rs\",\"text\":\"{}\"}}", button_file.replace('\n', "\\n").replace('"', "\\\"")),
    });

    server.handle_message(JsonRpcMessage::Notification {
        method: "textDocument/didOpen".to_string(),
        params_json: format!("{{\"uri\":\"file:///src/app.rs\",\"text\":\"{}\"}}", app_file.replace('\n', "\\n").replace('"', "\\\"")),
    });

    // 1. Search workspace symbols for "Button"
    let ws_req = JsonRpcMessage::Request {
        id: 30,
        method: "workspace/symbol".to_string(),
        params_json: "{\"query\":\"Button\"}".to_string(),
    };
    let ws_res = server.handle_message(ws_req).expect("Expected workspace/symbol response");
    assert!(ws_res.contains("\"name\":\"Button\""));
    assert!(ws_res.contains("\"name\":\"ButtonProps\""));

    // 2. Cross-file references for `Button` from app.rs (line 3, char 10)
    let refs_req = JsonRpcMessage::Request {
        id: 31,
        method: "textDocument/references".to_string(),
        params_json: "{\"uri\":\"file:///src/app.rs\",\"line\":3,\"character\":10}".to_string(),
    };
    let refs_res = server.handle_message(refs_req).expect("Expected references response");
    assert!(refs_res.contains("file:///src/button.rs"), "Must discover definition in button.rs");
    assert!(refs_res.contains("file:///src/app.rs"), "Must discover reference in app.rs");
}

#[test]
fn test_semantic_tokens_and_inlay_hints_provider() {
    let mut server = LspServer::new();

    let doc_source = r#"
pub fn Widget() -> Rsx {
    let count = 42;
    <div class="p-4 bg-black">
        <span>{ count }</span>
    </div>
}
"#;

    server.handle_message(JsonRpcMessage::Notification {
        method: "textDocument/didOpen".to_string(),
        params_json: format!("{{\"uri\":\"file:///widget.rs\",\"text\":\"{}\"}}", doc_source.replace('\n', "\\n").replace('"', "\\\"")),
    });

    // 1. Semantic Tokens
    let sem_req = JsonRpcMessage::Request {
        id: 40,
        method: "textDocument/semanticTokens/full".to_string(),
        params_json: "{\"uri\":\"file:///widget.rs\"}".to_string(),
    };
    let sem_res = server.handle_message(sem_req).expect("Expected semantic tokens response");
    assert!(sem_res.contains("\"data\":["));
    // Tokens data should not be empty
    assert!(!sem_res.contains("\"data\":[]"));

    // 2. Inlay Hints
    let hint_req = JsonRpcMessage::Request {
        id: 41,
        method: "textDocument/inlayHint".to_string(),
        params_json: "{\"uri\":\"file:///widget.rs\"}".to_string(),
    };
    let hint_res = server.handle_message(hint_req).expect("Expected inlay hints response");
    assert!(hint_res.contains(": i32"), "Must suggest inferred type hint : i32 for let count = 42");
}

