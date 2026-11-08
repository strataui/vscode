//! Strata Language Server (`strata-lsp`) Stdio Binary Entrypoint.

use std::io::{self, BufRead, Read, Write};
use strata_lsp::{encode_framed_message, parse_content_length, JsonRpcMessage, LspServer};

fn main() -> io::Result<()> {
    let mut server = LspServer::new();
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let stdout = io::stdout();
    let mut writer = stdout.lock();

    let mut header_buf = String::new();

    loop {
        header_buf.clear();
        let mut content_length = None;

        // Read HTTP-style headers until empty line "\r\n" or "\n"
        loop {
            let mut line = String::new();
            let bytes_read = reader.read_line(&mut line)?;
            if bytes_read == 0 {
                // EOF reached
                return Ok(());
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                // End of header section
                break;
            }

            if let Some(len) = parse_content_length(&line) {
                content_length = Some(len);
            }
        }

        let len = match content_length {
            Some(l) => l,
            None => continue,
        };

        // Read exact payload bytes
        let mut payload_buf = vec![0u8; len];
        reader.read_exact(&mut payload_buf)?;

        let payload_str = match String::from_utf8(payload_buf) {
            Ok(s) => s,
            Err(_) => continue,
        };

        if let Some(msg) = JsonRpcMessage::parse(&payload_str) {
            if let Some(response) = server.handle_message(msg) {
                let framed = encode_framed_message(&response);
                writer.write_all(framed.as_bytes())?;
                writer.flush()?;
            }
        }

        if server.shutdown_requested {
            // Wait for exit notification or terminate gracefully
            break;
        }
    }

    Ok(())
}
