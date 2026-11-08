//! Zero-Dependency JSON-RPC 2.0 Framing, Parser, and Serializer for LSP.

/// Extracts Content-Length from an HTTP-style header line.
pub fn parse_content_length(header: &str) -> Option<usize> {
    for line in header.lines() {
        let line_trimmed = line.trim();
        if let Some(rest) = line_trimmed.strip_prefix("Content-Length:") {
            if let Ok(val) = rest.trim().parse::<usize>() {
                return Some(val);
            }
        }
    }
    None
}

/// Encloses a JSON string in standard LSP Content-Length framing.
pub fn encode_framed_message(payload: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{}", payload.len(), payload)
}

/// JSON-RPC 2.0 Message representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonRpcMessage {
    Request {
        id: u64,
        method: String,
        params_json: String,
    },
    Notification {
        method: String,
        params_json: String,
    },
    Response {
        id: u64,
        result_json: String,
    },
    Error {
        id: Option<u64>,
        code: i64,
        message: String,
    },
}

impl JsonRpcMessage {
    /// Parses a JSON-RPC 2.0 payload string into a typed message.
    pub fn parse(payload: &str) -> Option<Self> {
        let trimmed = payload.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            return None;
        }

        let method = extract_json_string(trimmed, "method");
        let id_val = extract_json_number(trimmed, "id");

        if let Some(m) = method {
            let params = extract_json_raw(trimmed, "params").unwrap_or_else(|| "{}".to_string());
            if let Some(id) = id_val {
                Some(Self::Request {
                    id: id as u64,
                    method: m,
                    params_json: params,
                })
            } else {
                Some(Self::Notification {
                    method: m,
                    params_json: params,
                })
            }
        } else if let Some(id) = id_val {
            if let Some(res) = extract_json_raw(trimmed, "result") {
                Some(Self::Response {
                    id: id as u64,
                    result_json: res,
                })
            } else if let Some(err) = extract_json_raw(trimmed, "error") {
                let code = extract_json_number(&err, "code").unwrap_or(-32603);
                let msg = extract_json_string(&err, "message").unwrap_or_else(|| "Unknown error".to_string());
                Some(Self::Error {
                    id: Some(id as u64),
                    code,
                    message: msg,
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Serializes a successful response into JSON string.
    pub fn format_response(id: u64, result_json: &str) -> String {
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":{}}}",
            id, result_json
        )
    }

    /// Serializes a notification into JSON string.
    pub fn format_notification(method: &str, params_json: &str) -> String {
        format!(
            "{{\"jsonrpc\":\"2.0\",\"method\":\"{}\",\"params\":{}}}",
            method, params_json
        )
    }

    /// Serializes an error response into JSON string.
    pub fn format_error(id: Option<u64>, code: i64, message: &str) -> String {
        let id_str = match id {
            Some(v) => v.to_string(),
            None => "null".to_string(),
        };
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{},\"error\":{{\"code\":{},\"message\":\"{}\"}}}}",
            id_str, code, message.replace('"', "\\\"")
        )
    }
}

// ---------------------------------------------------------------------------
// Zero-Dependency JSON Extraction Helpers
// ---------------------------------------------------------------------------

pub fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let key_pos = json.find(&pattern)?;
    let after_key = &json[key_pos + pattern.len()..];
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();

    if !after_colon.starts_with('"') {
        return None;
    }

    let mut out = String::new();
    let chars = after_colon[1..].chars();
    let mut escape = false;

    for ch in chars {
        if escape {
            match ch {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                _ => out.push(ch),
            }
            escape = false;
        } else if ch == '\\' {
            escape = true;
        } else if ch == '"' {
            return Some(out);
        } else {
            out.push(ch);
        }
    }

    None
}

pub fn extract_json_number(json: &str, key: &str) -> Option<i64> {
    let pattern = format!("\"{}\"", key);
    let key_pos = json.find(&pattern)?;
    let after_key = &json[key_pos + pattern.len()..];
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();

    let mut num_str = String::new();
    for ch in after_colon.chars() {
        if ch.is_ascii_digit() || ch == '-' {
            num_str.push(ch);
        } else if !num_str.is_empty() {
            break;
        } else if ch.is_whitespace() {
            continue;
        } else {
            return None;
        }
    }

    num_str.parse::<i64>().ok()
}

pub fn extract_json_raw(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let key_pos = json.find(&pattern)?;
    let after_key = &json[key_pos + pattern.len()..];
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();

    if after_colon.starts_with('{') {
        // Find balanced brace
        let mut depth = 0;
        let mut end = 0;
        let mut in_string = false;
        let mut escape = false;

        for (i, ch) in after_colon.char_indices() {
            if escape {
                escape = false;
            } else if ch == '\\' && in_string {
                escape = true;
            } else if ch == '"' {
                in_string = !in_string;
            } else if !in_string {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
            }
        }

        if end > 0 {
            return Some(after_colon[0..end].to_string());
        }
    } else if after_colon.starts_with('[') {
        // Find balanced bracket
        let mut depth = 0;
        let mut end = 0;
        let mut in_string = false;
        let mut escape = false;

        for (i, ch) in after_colon.char_indices() {
            if escape {
                escape = false;
            } else if ch == '\\' && in_string {
                escape = true;
            } else if ch == '"' {
                in_string = !in_string;
            } else if !in_string {
                if ch == '[' {
                    depth += 1;
                } else if ch == ']' {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
            }
        }

        if end > 0 {
            return Some(after_colon[0..end].to_string());
        }
    } else {
        // Primitive literal (number, boolean, null, string)
        let mut end = 0;
        let mut in_string = false;
        let mut escape = false;

        for (i, ch) in after_colon.char_indices() {
            if escape {
                escape = false;
            } else if ch == '\\' && in_string {
                escape = true;
            } else if ch == '"' {
                in_string = !in_string;
                if !in_string {
                    end = i + 1;
                    break;
                }
            } else if !in_string && (ch == ',' || ch == '}' || ch == ']' || ch.is_whitespace()) {
                end = i;
                break;
            }
        }

        if end > 0 {
            return Some(after_colon[0..end].trim().to_string());
        }
    }

    None
}
