//! Real-time diagnostics engine detecting tag mismatches, unclosed elements, and invalid attributes.

use std::collections::HashSet;
use crate::parser::{RsxDocument, RsxNode};
use crate::types::Diagnostic;

/// Runs diagnostic analysis on a parsed RSX document, returning LSP diagnostics.
pub fn validate_document(doc: &RsxDocument) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // 1. Gather all syntax and structural parse errors
    for err in &doc.errors {
        diagnostics.push(err.to_diagnostic(&doc.source_map));
    }

    // 2. Validate element attributes recursively
    for node in &doc.nodes {
        validate_node(node, doc, &mut diagnostics);
    }

    // Sort diagnostics by range start position
    diagnostics.sort_by(|a, b| {
        a.range
            .start
            .line
            .cmp(&b.range.start.line)
            .then_with(|| a.range.start.character.cmp(&b.range.start.character))
    });

    diagnostics
}

fn validate_node(node: &RsxNode, doc: &RsxDocument, diagnostics: &mut Vec<Diagnostic>) {
    if let RsxNode::Element {
        name: _,
        attributes,
        children,
        ..
    } = node
    {
        // Check for duplicate attributes
        let mut seen_attrs = HashSet::new();
        for attr in attributes {
            if !seen_attrs.insert(&attr.name) {
                let range = attr.name_span.to_range(&doc.source_map);
                diagnostics.push(Diagnostic::warning(
                    range,
                    format!("Duplicate attribute '{}' detected on element", attr.name),
                ));
            }

            // Check for empty or malformed class attributes
            if attr.name == "class" || attr.name == "className" {
                if let Some(val) = &attr.value {
                    validate_tailwind_classes(val, attr.value_span, doc, diagnostics);
                }
            }
        }

        // Recurse children
        for child in children {
            validate_node(child, doc, diagnostics);
        }
    } else if let RsxNode::Block { children, .. } = node {
        for child in children {
            validate_node(child, doc, diagnostics);
        }
    }
}

fn validate_tailwind_classes(
    classes: &str,
    span: Option<crate::parser::Span>,
    doc: &RsxDocument,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(s) = span {
        let mut current_offset = s.start;
        for token in classes.split_whitespace() {
            // Find token in slice
            if let Some(pos) = doc.source[current_offset..s.end].find(token) {
                let token_start = current_offset + pos;
                let token_end = token_start + token.len();
                current_offset = token_end;

                if is_obviously_invalid_tailwind(token) {
                    let token_range = crate::parser::Span::new(token_start, token_end).to_range(&doc.source_map);
                    diagnostics.push(Diagnostic::warning(
                        token_range,
                        format!("Unrecognized utility class '{}'", token),
                    ));
                }
            }
        }
    }
}

/// Fast heuristic checking if a utility class is completely unrecognized.
fn is_obviously_invalid_tailwind(token: &str) -> bool {
    let stripped = token
        .strip_prefix("hover:")
        .or_else(|| token.strip_prefix("focus:"))
        .or_else(|| token.strip_prefix("active:"))
        .or_else(|| token.strip_prefix("dark:"))
        .or_else(|| token.strip_prefix("sm:"))
        .or_else(|| token.strip_prefix("md:"))
        .or_else(|| token.strip_prefix("lg:"))
        .or_else(|| token.strip_prefix("xl:"))
        .unwrap_or(token);

    // Arbitrary value support e.g. w-[420px], bg-[#123456]
    if stripped.contains('[') && stripped.ends_with(']') {
        return false;
    }

    // Common prefixes
    const KNOWN_PREFIXES: &[&str] = &[
        "flex", "grid", "col-", "row-", "items-", "justify-", "self-", "gap-",
        "w-", "h-", "min-w-", "max-w-", "min-h-", "max-h-",
        "p-", "px-", "py-", "pt-", "pb-", "pl-", "pr-",
        "m-", "mx-", "my-", "mt-", "mb-", "ml-", "mr-",
        "text-", "font-", "leading-", "tracking-",
        "bg-", "border-", "rounded-", "shadow-", "opacity-",
        "overflow-", "z-", "cursor-", "select-", "transition-", "duration-", "delay-", "ease-",
        "relative", "absolute", "fixed", "sticky", "static", "block", "inline-block", "inline", "hidden",
        "truncate", "animate-", "border", "rounded", "shadow", "underline"
    ];

    !KNOWN_PREFIXES.iter().any(|prefix| stripped.starts_with(prefix))
}
