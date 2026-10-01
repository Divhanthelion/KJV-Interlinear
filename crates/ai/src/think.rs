//! Local reasoning models (Qwen, DeepSeek-R1, …) served without a reasoning parser
//! put their thinking inline as `<think>…</think>`. This splits it back out so the
//! app can show it apart from the answer. Tags may arrive split across chunks.

const OPEN: &str = "<think>";
const CLOSE: &str = "</think>";

#[derive(Default)]
pub struct ThinkSplitter {
    /// Text held back because it may be the start of a tag
    pending: String,
    thinking: bool,
    /// Any answer text seen yet (a `<think>` only counts at the start)
    answered: bool,
}

/// A piece of the reply: `(is_reasoning, text)`.
pub type Piece = (bool, String);

impl ThinkSplitter {
    pub fn push(&mut self, text: &str) -> Vec<Piece> {
        self.pending.push_str(text);
        let mut out = Vec::new();
        loop {
            if self.thinking {
                if let Some(i) = self.pending.find(CLOSE) {
                    let thought: String = self.pending.drain(..i + CLOSE.len()).collect();
                    emit(&mut out, true, &thought[..i]);
                    self.thinking = false;
                    // The answer usually starts after a blank line
                    let rest = self.pending.trim_start().to_string();
                    self.pending = rest;
                    continue;
                }
                let keep = partial_suffix(&self.pending, CLOSE);
                let thought: String = self.pending.drain(..self.pending.len() - keep).collect();
                emit(&mut out, true, &thought);
                return out;
            }
            if !self.answered {
                let trimmed = self.pending.trim_start();
                if let Some(rest) = trimmed.strip_prefix(OPEN) {
                    self.pending = rest.to_string();
                    self.thinking = true;
                    continue;
                }
                if OPEN.starts_with(trimmed) {
                    // Could still become "<think>"
                    return out;
                }
            }
            let text = std::mem::take(&mut self.pending);
            if !text.trim().is_empty() {
                self.answered = true;
            }
            emit(&mut out, false, &text);
            return out;
        }
    }

    /// End of stream: release anything held back.
    pub fn finish(&mut self) -> Vec<Piece> {
        let text = std::mem::take(&mut self.pending);
        let mut out = Vec::new();
        emit(&mut out, self.thinking, &text);
        out
    }
}

fn emit(out: &mut Vec<Piece>, reasoning: bool, text: &str) {
    if !text.is_empty() {
        out.push((reasoning, text.to_string()));
    }
}

/// Length of the longest suffix of `text` that is a proper prefix of `tag`.
fn partial_suffix(text: &str, tag: &str) -> usize {
    (1..tag.len())
        .rev()
        .find(|&n| text.len() >= n && text.is_char_boundary(text.len() - n) && tag.starts_with(&text[text.len() - n..]))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(chunks: &[&str]) -> (String, String) {
        let mut s = ThinkSplitter::default();
        let mut pieces = Vec::new();
        for c in chunks {
            pieces.extend(s.push(c));
        }
        pieces.extend(s.finish());
        let pick = |r: bool| pieces.iter().filter(|p| p.0 == r).map(|p| p.1.as_str()).collect::<String>();
        (pick(true), pick(false))
    }

    #[test]
    fn splits_tags_across_chunks() {
        let (thought, answer) = run(&["\n<thi", "nk>Let me ", "recall John 3.</th", "ink>\n\nFor God so loved"]);
        assert_eq!(thought, "Let me recall John 3.");
        assert_eq!(answer, "For God so loved");
    }

    #[test]
    fn plain_answers_pass_through_even_when_they_mention_tags() {
        let (thought, answer) = run(&["The ", "tag <think> is literal here."]);
        assert_eq!(thought, "");
        assert_eq!(answer, "The tag <think> is literal here.");
        let (_, answer) = run(&["<", "b>bold</b>"]);
        assert_eq!(answer, "<b>bold</b>");
    }

    #[test]
    fn unfinished_thinking_is_still_reasoning() {
        let (thought, answer) = run(&["<think>still going"]);
        assert_eq!((thought.as_str(), answer.as_str()), ("still going", ""));
    }
}
