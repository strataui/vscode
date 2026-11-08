# Strata VS Code Tooling & Language Server

Language Server Protocol (LSP 3.17), tree-sitter grammars, syntax highlighting, and editor tooling for Strata & RSX.

## Overview

This repository provides editor tooling and language intelligence for developers building user interfaces with Strata and RSX. It houses both the pure-Rust `strata-lsp` language server binary and the official Visual Studio Code editor extension.

## Features

- **Pure-Rust Language Server (`strata-lsp`):** Implements LSP 3.17 over standard JSON-RPC 2.0.
- **Tree-Sitter Rust & RSX Parsing:** AST-driven code intelligence using `tree-sitter-rust`.
- **Live Diagnostics:** Real-time syntax errors, unclosed tags, and unknown Tailwind utility class warnings.
- **Autocomplete & IntelliSense:** Context-aware completion for RSX tags, properties, events, and Tailwind utility classes.
- **Hover Tooltips:** Documentation and CSS property previews on hover.
- **Color Decorators:** Inline color decorators for hex colors and Tailwind color classes.
- **Syntax Highlighting:** TextMate grammar definitions for `.rsx` files and embedded RSX macros.

## Repository Workspace

- `lsp/`: The pure-Rust language server implementation (`strata-lsp`).
- `syntaxes/`: TextMate grammar specifications for RSX.
- `language-configuration.json`: Brackets, auto-closing pairs, and indentation rules.
- `package.json`: VS Code extension manifest and language registration.

## Verification & Testing

```bash
cargo check --workspace --all-targets
cargo test --workspace
```

## License

Licensed under the MIT License. See [LICENSE](file:///home/usr/Projects/strataui/vscode/LICENSE) for details.
