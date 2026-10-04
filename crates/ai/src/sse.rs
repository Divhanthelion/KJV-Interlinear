//! Server-sent events (the WHATWG `text/event-stream` format): collects `data` lines
//! into payloads. Chunks can split lines (and UTF-8 characters) anywhere, so bytes are
//! buffered until a line ends. Lines end with LF, CRLF, or a bare CR; a byte-order
//! mark at the very start is skipped. An event counts only once the blank line after
//! it arrives: a stream cut off mid-event loses that event, as the format says.

/// Longest line, and longest event, accepted. Real events are a few kilobytes; this
/// keeps a broken or hostile server from filling memory.
const LIMIT: usize = 1 << 20;

#[derive(Default)]
pub struct SseParser {
    line: Vec<u8>,
    data: Vec<String>,
    /// Bytes in `data`, newlines included
    data_len: usize,
    /// The first line has been read (it's the one that may start with a BOM)
    started: bool,
    /// The last byte was a CR: an LF right after it ends the same line
    after_cr: bool,
}

impl SseParser {
    /// Feed bytes; returns every payload completed by them. A line or event over 1 MB
    /// is an error.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for &b in bytes {
            if std::mem::take(&mut self.after_cr) && b == b'\n' {
                continue;
            }
            if b == b'\n' || b == b'\r' {
                self.after_cr = b == b'\r';
                let line = std::mem::take(&mut self.line);
                self.line_done(&line, &mut out)?;
            } else {
                if self.line.len() >= LIMIT {
                    return Err("The server sent a line over 1 MB long; that isn't a chat stream.".into());
                }
                self.line.push(b);
            }
        }
        Ok(out)
    }

    fn line_done(&mut self, line: &[u8], out: &mut Vec<String>) -> Result<(), String> {
        let line = String::from_utf8_lossy(line);
        let mut line: &str = &line;
        if !std::mem::replace(&mut self.started, true) {
            line = line.strip_prefix('\u{FEFF}').unwrap_or(line);
        }
        if line.is_empty() {
            self.dispatch(out);
            return Ok(());
        }
        // `field: value`, or a bare `field` with an empty value
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        if field == "data" {
            self.data_len += value.len() + 1;
            if self.data_len > LIMIT {
                return Err("The server sent an event over 1 MB long; that isn't a chat stream.".into());
            }
            self.data.push(value.to_string());
        }
        // `event`, `id`, `retry`, and `:` comments carry nothing the decoders need
        Ok(())
    }

    fn dispatch(&mut self, out: &mut Vec<String>) {
        let data = self.data.join("\n");
        self.data.clear();
        self.data_len = 0;
        // No data lines, or only empty ones (a bare `data`): nothing for the decoders
        if !data.is_empty() {
            out.push(data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(chunks: &[&[u8]]) -> Vec<String> {
        let mut p = SseParser::default();
        chunks.iter().flat_map(|c| p.push(c).unwrap()).collect()
    }

    #[test]
    fn joins_split_chunks_and_multibyte_characters() {
        let mut p = SseParser::default();
        let text = "event: x\r\ndata: {\"t\":\"בְּ\"}\r\n\r\n: keep-alive\n\ndata: [DONE]\n\n";
        let mut got = Vec::new();
        for chunk in text.as_bytes().chunks(3) {
            got.extend(p.push(chunk).unwrap());
        }
        assert_eq!(got, vec!["{\"t\":\"בְּ\"}".to_string(), "[DONE]".to_string()]);
    }

    #[test]
    fn multi_line_data_and_an_unterminated_tail() {
        let mut p = SseParser::default();
        assert!(p.push(b"data: a\ndata: b\n").unwrap().is_empty());
        assert_eq!(p.push(b"\ndata: c").unwrap(), vec!["a\nb".to_string()]);
        // The stream ends here: "c" never got its blank line, so it isn't an event
        assert!(p.push(b"\n").unwrap().is_empty());
    }

    #[test]
    fn byte_order_mark_is_skipped_even_when_split() {
        assert_eq!(parse(&[b"\xEF\xBB", b"\xBFdata: a\n\n"]), vec!["a".to_string()]);
        // Only at the very start
        assert_eq!(parse(&[b"data: a\n\n\xEF\xBB\xBFdata: b\n\n"]), vec!["a".to_string()]);
    }

    #[test]
    fn bare_cr_and_crlf_split_across_chunks_end_lines() {
        assert_eq!(parse(&[b"data: a\rdata: b\r\r"]), vec!["a\nb".to_string()]);
        assert_eq!(parse(&[b"data: a\r", b"\n\r", b"\ndata: b\r\n\r\n"]), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn bare_data_lines_add_nothing_on_their_own() {
        assert!(parse(&[b"data\n\n"]).is_empty());
        assert_eq!(parse(&[b"data: a\ndata\ndata: b\n\n"]), vec!["a\n\nb".to_string()]);
        // Not a data field: `database: x`, `datum: y`
        assert!(parse(&[b"database: x\ndatum: y\n\n"]).is_empty());
    }

    #[test]
    fn oversized_lines_and_events_are_errors() {
        let mut p = SseParser::default();
        let big = vec![b'x'; LIMIT + 1];
        assert!(p.push(b"data: ").unwrap().is_empty());
        assert!(p.push(&big).unwrap_err().contains("1 MB"));

        let mut p = SseParser::default();
        let line = format!("data: {}\n", "y".repeat(LIMIT / 2));
        assert!(p.push(line.as_bytes()).unwrap().is_empty());
        assert!(p.push(line.as_bytes()).unwrap_err().contains("event over 1 MB"));
    }
}
