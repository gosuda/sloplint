//! Path normalization, identifier splitting, and static-literal chunk measurement.

/// Joins path components with `/`, returning `None` when a component is not UTF-8.
pub fn normalize_rel_path(path: &std::path::Path) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for component in path.components() {
        parts.push(component.as_os_str().to_str()?);
    }
    Some(parts.join("/"))
}

/// Splits an identifier on non-alphanumerics, on lower-to-upper boundaries, and on letter-digit boundaries, lowercasing each word.
pub fn identifier_words(s: &str) -> impl Iterator<Item = String> {
    s.split(|c: char| !c.is_alphanumeric()).flat_map(split_word)
}

fn split_word(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && is_boundary(chars[i - 1], c) {
            words.push(current.to_lowercase());
            current.clear();
        }
        current.push(c);
    }
    if !current.is_empty() {
        words.push(current.to_lowercase());
    }
    words
}

fn is_boundary(prev: char, next: char) -> bool {
    prev.is_ascii_digit() != next.is_ascii_digit() || (prev.is_lowercase() && next.is_uppercase())
}

/// Returns the longest literal run, in characters, across the static segments of a format string: placeholders (`{...}`, printf specs), doubled braces as literals, and backslash escapes break the run.
pub fn longest_static_chunk(segments: &[&str]) -> u32 {
    segments
        .iter()
        .map(|segment| longest_in_segment(segment))
        .max()
        .unwrap_or(0)
}

struct Scan {
    best: u32,
    run: u32,
    i: usize,
}

impl Scan {
    fn step(&mut self, chars: &[char]) {
        let c = chars[self.i];
        if c == '\\' {
            self.break_run();
            self.i = (self.i + 2).min(chars.len());
        } else if c == '{' {
            self.brace(chars);
        } else if c == '}' && chars.get(self.i + 1) == Some(&'}') {
            self.run += 1;
            self.i += 2;
        } else if c == '%' {
            self.percent(chars);
        } else {
            self.run += 1;
            self.i += 1;
        }
    }

    fn brace(&mut self, chars: &[char]) {
        if chars.get(self.i + 1) == Some(&'{') {
            self.run += 1;
            self.i += 2;
            return;
        }
        self.break_run();
        self.i += 1;
        while self.i < chars.len() && chars[self.i] != '}' {
            self.i += 1;
        }
        self.i = (self.i + 1).min(chars.len());
    }

    fn percent(&mut self, chars: &[char]) {
        if chars.get(self.i + 1) == Some(&'%') {
            self.run += 1;
            self.i += 2;
            return;
        }
        if let Some(width) = printf_width(&chars[self.i + 1..]) {
            self.break_run();
            self.i += 1 + width;
            return;
        }
        self.run += 1;
        self.i += 1;
    }

    fn break_run(&mut self) {
        self.best = self.best.max(self.run);
        self.run = 0;
    }
}

fn longest_in_segment(segment: &str) -> u32 {
    let chars: Vec<char> = segment.chars().collect();
    let mut scan = Scan {
        best: 0,
        run: 0,
        i: 0,
    };
    while scan.i < chars.len() {
        scan.step(&chars);
    }
    scan.best.max(scan.run)
}

fn printf_width(chars: &[char]) -> Option<usize> {
    let mut i = 0usize;
    while i < chars.len() && matches!(chars[i], '-' | '+' | ' ' | '#' | '0') {
        i += 1;
    }
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
    }
    (i < chars.len() && chars[i].is_ascii_alphabetic()).then_some(i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::path::Path;

    #[test]
    fn chunks_break_at_placeholders_and_escapes() {
        assert_eq!(longest_static_chunk(&[]), 0);
        assert_eq!(longest_static_chunk(&["hello world"]), 11);
        assert_eq!(longest_static_chunk(&["failed: {err}"]), 8);
        assert_eq!(longest_static_chunk(&["code {{x}} done"]), 13);
        assert_eq!(longest_static_chunk(&["%s of %d items"]), 6);
        assert_eq!(longest_static_chunk(&["100%% done"]), 9);
        assert_eq!(longest_static_chunk(&["a\\nb"]), 1);
        assert_eq!(longest_static_chunk(&["%5.2f|tail"]), 5);
        assert_eq!(longest_static_chunk(&["50% off", "plain"]), 5);
    }

    #[test]
    fn identifiers_split_on_case_and_digit_boundaries() {
        let words: Vec<String> = identifier_words("processOrder-x86_count").collect();
        assert_eq!(words, ["process", "order", "x", "86", "count"]);
        let words: Vec<String> = identifier_words("__init__").collect();
        assert_eq!(words, ["init"]);
    }

    proptest! {
                        #[test]
        fn chunk_never_exceeds_length(s in ".*") {
            let chunk = usize::try_from(longest_static_chunk(&[s.as_str()])).unwrap_or(usize::MAX);
            prop_assert!(chunk <= s.chars().count());
        }

                #[test]
        fn chunk_equals_length_without_specials(
            s in any::<String>().prop_filter("no specials", |s| !s.contains(['{', '}', '%', char::from(92u8)])),
        ) {
            let chunk = usize::try_from(longest_static_chunk(&[s.as_str()])).unwrap_or(usize::MAX);
            prop_assert_eq!(chunk, s.chars().count());
        }

        #[test]
        fn normalize_joins_slashes_and_is_idempotent(
            components in proptest::collection::vec("[a-z][a-z0-9_]{0,8}", 0..6),
        ) {
            let joined = components.join("/");
            let once = normalize_rel_path(Path::new(&joined)).unwrap();
            prop_assert!(!once.contains('\\'));
            let twice = normalize_rel_path(Path::new(&once)).unwrap();
            prop_assert_eq!(once, twice);
        }
    }
}
