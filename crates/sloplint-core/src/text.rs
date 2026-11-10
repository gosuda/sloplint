pub fn normalize_rel_path(path: &std::path::Path) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for component in path.components() {
        parts.push(component.as_os_str().to_str()?);
    }
    Some(parts.join("/"))
}

pub fn identifier_words(s: &str) -> impl Iterator<Item = String> {
    let mut chars = s.chars();
    let mut current = String::new();
    let mut prev: Option<char> = None;
    let mut done = false;
    std::iter::from_fn(move || {
        for c in chars.by_ref() {
            if !c.is_alphanumeric() {
                if !current.is_empty() {
                    prev = None;
                    let out = current.to_lowercase();
                    current.clear();
                    return Some(out);
                }
                prev = None;
                continue;
            }
            if let Some(p) = prev
                && is_boundary(p, c)
            {
                let out = current.to_lowercase();
                current.clear();
                current.push(c);
                prev = Some(c);
                return Some(out);
            }
            current.push(c);
            prev = Some(c);
        }
        if done || current.is_empty() {
            return None;
        }
        done = true;
        Some(std::mem::take(&mut current).to_lowercase())
    })
}

fn is_boundary(prev: char, next: char) -> bool {
    prev.is_ascii_digit() != next.is_ascii_digit() || (prev.is_lowercase() && next.is_uppercase())
}

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
    fn step(&mut self, bytes: &[u8]) {
        match bytes[self.i] {
            b'\\' => self.escape(bytes),
            b'{' => self.brace(bytes),
            b'}' if bytes.get(self.i + 1) == Some(&b'}') => {
                self.run += 1;
                self.i += 2;
            }
            b'%' => self.percent(bytes),
            b => {
                self.run += 1;
                self.i += utf8_len(b);
            }
        }
    }

    fn escape(&mut self, bytes: &[u8]) {
        self.break_run();
        self.i += 1;
        if self.i < bytes.len() {
            self.i = (self.i + utf8_len(bytes[self.i])).min(bytes.len());
        }
    }

    fn brace(&mut self, bytes: &[u8]) {
        if bytes.get(self.i + 1) == Some(&b'{') {
            self.run += 1;
            self.i += 2;
            return;
        }
        self.break_run();
        self.i += 1;
        while self.i < bytes.len() && bytes[self.i] != b'}' {
            self.i += 1;
        }
        if self.i < bytes.len() {
            self.i += 1;
        }
    }

    fn percent(&mut self, bytes: &[u8]) {
        if bytes.get(self.i + 1) == Some(&b'%') {
            self.run += 1;
            self.i += 2;
            return;
        }
        if let Some(width) = printf_width(&bytes[self.i + 1..]) {
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
    let bytes = segment.as_bytes();
    let mut scan = Scan {
        best: 0,
        run: 0,
        i: 0,
    };
    while scan.i < bytes.len() {
        scan.step(bytes);
    }
    scan.best.max(scan.run)
}

fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first & 0xE0 == 0xC0 {
        2
    } else if first & 0xF0 == 0xE0 {
        3
    } else {
        4
    }
}

fn printf_width(bytes: &[u8]) -> Option<usize> {
    let mut i = 0usize;
    while i < bytes.len() && matches!(bytes[i], b'-' | b'+' | b' ' | b'#' | b'0') {
        i += 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    (i < bytes.len() && bytes[i].is_ascii_alphabetic()).then_some(i + 1)
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
        let words: Vec<String> = identifier_words("ΟΔΟΣ").collect();
        assert_eq!(
            words,
            ["οδος"],
            "whole-word lowercase must apply final sigma"
        );
        let words: Vec<String> = identifier_words("ΟΔΟΣ2").collect();
        assert_eq!(
            words,
            ["οδος", "2"],
            "digit boundary must split after final sigma"
        );
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
