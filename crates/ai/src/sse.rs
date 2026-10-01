//! Server-sent events: collects `data:` lines into payloads. Chunks can split lines
//! (and UTF-8 characters) anywhere, so bytes are buffered until a newline.

#[derive(Default)]
pub struct SseParser {
    line: Vec<u8>,
    data: Vec<String>,
}

impl SseParser {
    /// Feed bytes; returns every payload completed by them.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        for &b in bytes {
            if b == b'\n' {
                let line = std::mem::take(&mut self.line);
                self.line_done(&line, &mut out);
            } else {
                self.line.push(b);
            }
        }
        out
    }

    /// End of stream: whatever is left counts as a final event.
    pub fn finish(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.line.is_empty() {
            let line = std::mem::take(&mut self.line);
            self.line_done(&line, &mut out);
        }
        self.dispatch(&mut out);
        out
    }

    fn line_done(&mut self, line: &[u8], out: &mut Vec<String>) {
        let line = String::from_utf8_lossy(line);
        let line = line.strip_suffix('\r').unwrap_or(&line);
        if line.is_empty() {
            self.dispatch(out);
        } else if let Some(value) = line.strip_prefix("data:") {
            self.data.push(value.strip_prefix(' ').unwrap_or(value).to_string());
        }
        // `event:`, `id:`, `retry:` and `:` comments carry nothing the decoders need
    }

    fn dispatch(&mut self, out: &mut Vec<String>) {
        if !self.data.is_empty() {
            out.push(self.data.join("\n"));
            self.data.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SseParser;

    #[test]
    fn joins_split_chunks_and_multibyte_characters() {
        let mut p = SseParser::default();
        let text = "event: x\r\ndata: {\"t\":\"בְּ\"}\r\n\r\n: keep-alive\n\ndata: [DONE]\n\n";
        let bytes = text.as_bytes();
        let mut got = Vec::new();
        for chunk in bytes.chunks(3) {
            got.extend(p.push(chunk));
        }
        got.extend(p.finish());
        assert_eq!(got, vec!["{\"t\":\"בְּ\"}".to_string(), "[DONE]".to_string()]);
    }

    #[test]
    fn multi_line_data_and_unterminated_tail() {
        let mut p = SseParser::default();
        assert!(p.push(b"data: a\ndata: b\n").is_empty());
        assert_eq!(p.push(b"\ndata: c"), vec!["a\nb".to_string()]);
        assert_eq!(p.finish(), vec!["c".to_string()]);
    }
}
