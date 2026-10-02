//! SWORD module configuration (`mods.d/<name>.conf`).

/// The `key=value` lines of a module `.conf`, in file order. Repeated keys are kept
/// (`GlobalOptionFilter`, `History_*`, `Feature`, ...).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Conf {
    pub entries: Vec<(String, String)>,
}

impl Conf {
    /// Parses a conf file's text.
    ///
    /// - The first `[Section]` line names the module; it is not an entry.
    /// - Blank lines and lines whose first non-blank character is `#` are ignored.
    /// - A line ending in `\` continues on the next line. The backslash is removed and
    ///   the lines are joined with `\n`; continuation lines are never comments.
    /// - Keys and values are trimmed of surrounding whitespace; the value is otherwise
    ///   untouched (RTF such as `\par` is not interpreted).
    pub fn parse(text: &str) -> Conf {
        Conf::parse_with_section(text).1
    }

    /// Like [`Conf::parse`], also returning the module name from the `[Section]` line.
    pub fn parse_with_section(text: &str) -> (Option<String>, Conf) {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut section = None;
        let mut entries: Vec<(String, String)> = Vec::new();
        // The entry being continued across lines.
        let mut pending: Option<(String, String)> = None;
        for raw in text.split('\n') {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            if let Some((_, value)) = pending.as_mut() {
                // Continuation line: data, never a comment or a new key.
                value.push('\n');
                match line.strip_suffix('\\') {
                    Some(rest) => value.push_str(rest),
                    None => {
                        value.push_str(line);
                        let (k, v) = pending.take().unwrap();
                        entries.push((k, v.trim().to_string()));
                    }
                }
                continue;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                if section.is_none() {
                    section = Some(trimmed[1..trimmed.len() - 1].to_string());
                }
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue; // not a key=value line
            };
            let key = key.trim().to_string();
            match value.strip_suffix('\\') {
                Some(rest) => pending = Some((key, rest.to_string())),
                None => entries.push((key, value.trim().to_string())),
            }
        }
        if let Some((k, v)) = pending {
            entries.push((k, v.trim().to_string())); // file ended mid-continuation
        }
        (section, Conf { entries })
    }

    /// The first value for `key` (keys are case-sensitive, as in SWORD).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Every value for `key`, in file order.
    pub fn get_all(&self, key: &str) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entries_repeats_comments_and_continuations() {
        let text = [
            "\u{feff}## header",
            "[Mod]",
            "# comment",
            "Description = A thing ",
            r"About=line one \par \",
            r"# not a comment \",
            "line two",
            "Filter=a",
            "Filter=b",
            "",
            "Empty=",
            "",
        ]
        .join("\r\n");
        let (name, conf) = Conf::parse_with_section(&text);
        assert_eq!(name.as_deref(), Some("Mod"));
        assert_eq!(conf.get("Description"), Some("A thing"));
        assert_eq!(
            conf.get("About"),
            Some("line one \\par \n# not a comment \nline two")
        );
        assert_eq!(conf.get_all("Filter"), vec!["a", "b"]);
        assert_eq!(conf.get("Empty"), Some(""));
        assert_eq!(conf.get("Nope"), None);
        let keys: Vec<_> = conf.entries.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["Description", "About", "Filter", "Filter", "Empty"]);
    }

    #[test]
    fn value_may_contain_equals_signs() {
        let conf = Conf::parse("[X]\nTextSource=http://a/b?c=d\n");
        assert_eq!(conf.get("TextSource"), Some("http://a/b?c=d"));
    }

    #[test]
    fn unterminated_continuation_keeps_the_text() {
        let conf = Conf::parse("[X]\nAbout=abc \\");
        assert_eq!(conf.get("About"), Some("abc"));
    }
}
