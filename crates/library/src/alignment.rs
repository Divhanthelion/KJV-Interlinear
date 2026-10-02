//! Reading the verse alignment tables (data/library/alignment/<id>.tsv, made by
//! kjv-import from [`crate::align`]) and answering "which verses correspond?".
//!
//! A table lists only the verses whose KJV counterpart isn't the same-numbered KJV
//! verse; every other verse corresponds to the KJV verse with the same book, chapter,
//! and number (where the KJV has it).

use std::collections::{HashMap, HashSet};

/// A verse: book code, chapter, number as printed ("16", "1-2", "0" for a title).
pub type Ref = (String, u32, String);

#[derive(Debug, Default, Clone)]
pub struct Alignment {
    /// Listed verses of this translation and their KJV counterparts (maybe none)
    to_kjv: HashMap<Ref, Vec<Ref>>,
    /// KJV verses and the listed verses of this translation that correspond to them
    from_kjv: HashMap<Ref, Vec<Ref>>,
}

fn parse_ref(book: &str, s: &str) -> Option<Ref> {
    let (c, v) = s.split_once(':')?;
    Some((book.to_string(), c.parse().ok()?, v.to_string()))
}

impl Alignment {
    /// The identity alignment (the KJV against itself).
    pub fn identity() -> Self {
        Self::default()
    }

    pub fn parse(tsv: &str) -> Result<Self, String> {
        let mut a = Self::default();
        for (n, line) in tsv.lines().enumerate() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let bad = || format!("alignment line {}: {:?}", n + 1, line);
            let f: Vec<&str> = line.split('\t').collect();
            let [book, here, there, _how, _score] = f[..] else { return Err(bad()) };
            let natives: Vec<Ref> = here.split('+').map(|r| parse_ref(book, r)).collect::<Option<_>>().ok_or_else(bad)?;
            let kjv: Vec<Ref> = if there == "-" {
                Vec::new()
            } else {
                there
                    .split('+')
                    .map(|r| {
                        let (b, cv) = r.split_once(' ')?;
                        parse_ref(b, cv)
                    })
                    .collect::<Option<_>>()
                    .ok_or_else(bad)?
            };
            for nat in &natives {
                a.to_kjv.insert(nat.clone(), kjv.clone());
                for k in &kjv {
                    a.from_kjv.entry(k.clone()).or_default().push(nat.clone());
                }
            }
        }
        for v in a.from_kjv.values_mut() {
            v.sort();
            v.dedup();
        }
        Ok(a)
    }

    /// The KJV verses `verse` of this translation corresponds to. `kjv_has(r)`: whether
    /// the KJV has verse `r` (for the same-number default).
    pub fn to_kjv(&self, verse: &Ref, kjv_has: &dyn Fn(&Ref) -> bool) -> Vec<Ref> {
        match self.to_kjv.get(verse) {
            Some(listed) => listed.clone(),
            None if kjv_has(verse) => vec![verse.clone()],
            None => Vec::new(),
        }
    }

    /// The verses of this translation that correspond to KJV verse `kjv`. `has(r)`:
    /// whether this translation has verse `r`.
    pub fn from_kjv(&self, kjv: &Ref, has: &dyn Fn(&Ref) -> bool) -> Vec<Ref> {
        let mut out: Vec<Ref> = self.from_kjv.get(kjv).cloned().unwrap_or_default();
        // The same-numbered verse, unless the table sends it elsewhere
        if !self.to_kjv.contains_key(kjv) && has(kjv) && !out.contains(kjv) {
            out.push(kjv.clone());
        }
        out.sort();
        out
    }

    /// The listed verses (for reports and tests).
    pub fn listed(&self) -> HashSet<&Ref> {
        self.to_kjv.keys().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(b: &str, c: u32, v: &str) -> Ref {
        (b.into(), c, v.into())
    }

    #[test]
    fn listed_rows_and_the_same_number_default() {
        let t = "# header\nPSA\t9:22\tPSA 10:1\tcontent\t0.35\nPSA\t50:1+50:2\tPSA 51:0\tcontent\t0.81\nPSA\t10:1\tPSA 11:1\tcontent\t0.9\nTOB\t3:10\t-\tunmatched\t0.00\n";
        let a = Alignment::parse(t).unwrap();
        let yes = |_: &Ref| true;
        assert_eq!(a.to_kjv(&r("PSA", 9, "22"), &yes), [r("PSA", 10, "1")]);
        assert_eq!(a.to_kjv(&r("PSA", 50, "2"), &yes), [r("PSA", 51, "0")]);
        assert_eq!(a.to_kjv(&r("TOB", 3, "10"), &yes), Vec::<Ref>::new());
        assert_eq!(a.to_kjv(&r("GEN", 1, "1"), &yes), [r("GEN", 1, "1")]);
        assert_eq!(a.to_kjv(&r("GEN", 1, "99"), &|_| false), Vec::<Ref>::new());
        // KJV 10:1 is this translation's 9:22; its own 10:1 is KJV 11:1, not 10:1
        assert_eq!(a.from_kjv(&r("PSA", 10, "1"), &yes), [r("PSA", 9, "22")]);
        // This translation counts the title as verses 1-2 and has no title of its own
        let no_titles = |x: &Ref| x.2 != "0";
        assert_eq!(a.from_kjv(&r("PSA", 51, "0"), &no_titles), [r("PSA", 50, "1"), r("PSA", 50, "2")]);
        assert_eq!(a.from_kjv(&r("GEN", 1, "1"), &yes), [r("GEN", 1, "1")]);
        assert!(Alignment::parse("PSA\tbad").is_err());
    }
}
