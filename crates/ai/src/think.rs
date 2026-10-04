//! Local reasoning models (Qwen, DeepSeek-R1, …) served without a reasoning parser
//! put their thinking inline as `<think>…</think>`. This splits it back out so the
//! app can show it apart from the answer. Tags may arrive split across chunks.
//!
//! Some chat templates (DeepSeek-R1's, QwQ's, Qwen3 thinking models') open the think
//! block in the prompt, so the reply starts mid-thought and only `</think>` shows.
//! Text given out before that closing tag turns out to have been reasoning: the
//! splitter says so with [`Piece::TextWasReasoning`], and the page moves it.

const OPEN: &str = "<think>";
const CLOSE: &str = "</think>";

#[derive(Default)]
pub struct ThinkSplitter {
    /// Text held back because it may be the start of a tag
    pending: String,
    thinking: bool,
    /// Answer text (not just whitespace) given out yet: a `<think>` only counts at the start
    answered: bool,
    /// Any `Text` given out yet, whitespace included
    gave_text: bool,
    /// A think block has ended, or the answer showed a literal `<think>`: from now on
    /// tags are just text
    settled: bool,
    /// The last few characters given out as text, to catch a literal `<think>` that
    /// arrived split across chunks
    tail: String,
}

/// A piece of the reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Reasoning(String),
    /// Everything given out as `Text` so far was reasoning (a `</think>` with no
    /// `<think>` before it)
    TextWasReasoning,
}

impl ThinkSplitter {
    pub fn push(&mut self, text: &str) -> Vec<Piece> {
        self.pending.push_str(text);
        let mut out = Vec::new();
        loop {
            if self.thinking {
                if let Some(i) = self.pending.find(CLOSE) {
                    let thought: String = self.pending.drain(..i + CLOSE.len()).collect();
                    reasoning(&mut out, &thought[..i]);
                    self.thinking = false;
                    self.end_block();
                    continue;
                }
                let keep = partial_suffix(&self.pending, CLOSE);
                let thought: String = self.pending.drain(..self.pending.len() - keep).collect();
                reasoning(&mut out, &thought);
                return out;
            }
            if self.settled {
                let text = std::mem::take(&mut self.pending);
                self.give_text(&mut out, text);
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
            // A `<think>` partway into the answer is literal, and so is any `</think>`
            // after it (the answer is talking about the tags)
            let seen = format!("{}{}", self.tail, self.pending);
            if let Some(open) = seen.find(OPEN)
                && seen.find(CLOSE).is_none_or(|close| open < close)
            {
                self.settled = true;
                continue;
            }
            // A closing tag with no opening one: the template opened the block, and
            // everything before the tag was reasoning
            if let Some(i) = self.pending.find(CLOSE) {
                let thought: String = self.pending.drain(..i + CLOSE.len()).collect();
                if self.gave_text {
                    out.push(Piece::TextWasReasoning);
                }
                reasoning(&mut out, &thought[..i]);
                self.end_block();
                continue;
            }
            let keep = partial_suffix(&self.pending, CLOSE);
            let text: String = self.pending.drain(..self.pending.len() - keep).collect();
            self.give_text(&mut out, text);
            return out;
        }
    }

    /// End of stream: release anything held back.
    pub fn finish(&mut self) -> Vec<Piece> {
        let text = std::mem::take(&mut self.pending);
        let mut out = Vec::new();
        if self.thinking {
            reasoning(&mut out, &text);
        } else if !text.is_empty() {
            out.push(Piece::Text(text));
        }
        out
    }

    fn end_block(&mut self) {
        self.settled = true;
        // The answer usually starts after a blank line
        self.pending = self.pending.trim_start().to_string();
    }

    fn give_text(&mut self, out: &mut Vec<Piece>, text: String) {
        if text.is_empty() {
            return;
        }
        if !self.settled {
            let seen = format!("{}{}", self.tail, text);
            let mut start = seen.len().saturating_sub(OPEN.len());
            while !seen.is_char_boundary(start) {
                start -= 1;
            }
            self.tail = seen[start..].to_string();
        }
        if !text.trim().is_empty() {
            self.answered = true;
        }
        self.gave_text = true;
        out.push(Piece::Text(text));
    }
}

fn reasoning(out: &mut Vec<Piece>, text: &str) {
    if !text.is_empty() {
        out.push(Piece::Reasoning(text.to_string()));
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

    fn pieces(chunks: &[&str]) -> Vec<Piece> {
        let mut s = ThinkSplitter::default();
        let mut pieces = Vec::new();
        for c in chunks {
            pieces.extend(s.push(c));
        }
        pieces.extend(s.finish());
        pieces
    }

    /// (reasoning, answer) as the page ends up showing them
    fn run(chunks: &[&str]) -> (String, String) {
        let (mut thought, mut answer) = (String::new(), String::new());
        for piece in pieces(chunks) {
            match piece {
                Piece::Text(t) => answer.push_str(&t),
                Piece::Reasoning(t) => thought.push_str(&t),
                Piece::TextWasReasoning => thought.push_str(&std::mem::take(&mut answer)),
            }
        }
        (thought, answer)
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
        // Both tags, in an answer that's explaining them
        let (thought, answer) = run(&["Models wrap reasoning in <thi", "nk> and </think> tags."]);
        assert_eq!((thought.as_str(), answer.as_str()), ("", "Models wrap reasoning in <think> and </think> tags."));
        // After a real think block, a closing tag is just text
        let (thought, answer) = run(&["<think>Hm.</think>Write </think> to close it."]);
        assert_eq!((thought.as_str(), answer.as_str()), ("Hm.", "Write </think> to close it."));
    }

    #[test]
    fn unfinished_thinking_is_still_reasoning() {
        let (thought, answer) = run(&["<think>still going"]);
        assert_eq!((thought.as_str(), answer.as_str()), ("still going", ""));
    }

    #[test]
    fn closing_tag_without_opening_marks_what_came_before_as_reasoning() {
        // DeepSeek-R1 style: the template opened the block, so only `</think>` arrives
        let chunks = ["Okay, the user asks", " about John 11.</th", "ink>\n\nJesus wept."];
        assert_eq!(
            pieces(&chunks),
            vec![
                Piece::Text("Okay, the user asks".into()),
                Piece::Text(" about John 11.".into()),
                Piece::TextWasReasoning,
                Piece::Text("Jesus wept.".into()),
            ]
        );
        let (thought, answer) = run(&chunks);
        assert_eq!((thought.as_str(), answer.as_str()), ("Okay, the user asks about John 11.", "Jesus wept."));

        // All in one chunk: nothing went out as text, so nothing needs moving
        assert_eq!(
            pieces(&["Short thought.</think>\n\nAnswer."]),
            vec![Piece::Reasoning("Short thought.".into()), Piece::Text("Answer.".into())]
        );
        // A later `</think>` in the answer is literal
        let (thought, answer) = run(&["Thinking.</think>Answer with </think> in it."]);
        assert_eq!((thought.as_str(), answer.as_str()), ("Thinking.", "Answer with </think> in it."));
    }
}
