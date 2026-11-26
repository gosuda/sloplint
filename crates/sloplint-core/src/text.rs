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
    longest_static_chunk_with_escape(segments, b'\\')
}

pub fn longest_static_chunk_with_escape(segments: &[&str], escape: u8) -> u32 {
    segments
        .iter()
        .map(|segment| longest_in_segment(segment, escape))
        .max()
        .unwrap_or(0)
}

/// Longest static run inside a LoGOs `$"..."` literal body (the text between
/// `$"` and the closing quote), using the same hole rules as the
/// tree-sitter-logos external scanner: `{expr}` holes nest, and quotes,
/// comments, escapes and nested `$"..."` literals inside a hole never end
/// it early. Only the literal runs between holes are measured.
pub fn longest_static_chunk_interp(inner: &str) -> u32 {
    longest_static_chunk(&interp_literal_runs(inner))
}

fn interp_literal_runs(inner: &str) -> Vec<&str> {
    let bytes = inner.as_bytes();
    let mut runs = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                if bytes.get(i + 1) == Some(&b'{') {
                    i += 2;
                    continue;
                }
                if start < i {
                    runs.push(&inner[start..i]);
                }
                i = skip_interp_hole(bytes, i + 1);
                start = i;
            }
            b'\\' => {
                i += 1;
                if i < bytes.len() {
                    i += utf8_len(bytes[i]);
                }
            }
            _ => {
                i += utf8_len(bytes[i]);
            }
        }
    }
    if start < inner.len() {
        runs.push(&inner[start..]);
    }
    runs
}

fn skip_interp_hole(bytes: &[u8], mut i: usize) -> usize {
    let mut nesting = 1u32;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                nesting += 1;
                i += 1;
            }
            b'}' => {
                nesting -= 1;
                i += 1;
                if nesting == 0 {
                    return i;
                }
            }
            b'/' => {
                if bytes.get(i + 1) == Some(&b'/') {
                    i += 2;
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += utf8_len(bytes[i]);
                    }
                } else if bytes.get(i + 1) == Some(&b'*') {
                    i += 2;
                    let mut prev = 0u8;
                    while i < bytes.len() {
                        let c = bytes[i];
                        i += 1;
                        if prev == b'*' && c == b'/' {
                            break;
                        }
                        prev = c;
                    }
                } else {
                    i += 1;
                }
            }
            b'"' | b'\'' | b'`' => {
                i = skip_quoted_bytes(bytes, i);
            }
            b'$' => {
                if bytes.get(i + 1) == Some(&b'"') {
                    i = skip_interp_body(bytes, i + 2);
                } else {
                    i += 1;
                }
            }
            b'\\' => {
                i += 1;
                if i < bytes.len() {
                    i += utf8_len(bytes[i]);
                }
            }
            _ => {
                i += utf8_len(bytes[i]);
            }
        }
    }
    i
}

fn skip_quoted_bytes(bytes: &[u8], mut i: usize) -> usize {
    let quote = bytes[i];
    i += 1;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\\' && quote != b'`' {
            i += 1;
            if i < bytes.len() {
                i += utf8_len(bytes[i]);
            }
            continue;
        }
        i += utf8_len(c);
        if c == quote {
            break;
        }
    }
    i
}

fn skip_interp_body(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return i + 1,
            b'\\' => {
                i += 1;
                if i < bytes.len() {
                    i += utf8_len(bytes[i]);
                }
            }
            b'{' => {
                if bytes.get(i + 1) == Some(&b'{') {
                    i += 2;
                } else {
                    i = skip_interp_hole(bytes, i + 1);
                }
            }
            _ => {
                i += utf8_len(bytes[i]);
            }
        }
    }
    i
}

struct Scan {
    best: u32,
    run: u32,
    i: usize,
}

impl Scan {
    fn step(&mut self, bytes: &[u8], escape: u8) {
        match bytes[self.i] {
            byte if byte == escape => self.escape(bytes),
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

fn longest_in_segment(segment: &str, escape: u8) -> u32 {
    let bytes = segment.as_bytes();
    let mut scan = Scan {
        best: 0,
        run: 0,
        i: 0,
    };
    while scan.i < bytes.len() {
        scan.step(bytes, escape);
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
    fn powershell_chunks_keep_backslashes_and_break_at_backticks() {
        assert_eq!(
            longest_static_chunk_with_escape(&[r"\bin\deploy\"], b'`'),
            12
        );
        assert_eq!(longest_static_chunk_with_escape(&["ab`ncd"], b'`'), 2);
        assert_eq!(longest_static_chunk_with_escape(&["ab``cd"], b'`'), 2);
    }
    #[test]
    fn interp_chunks_measure_only_literal_runs() {
        assert_eq!(longest_static_chunk_interp("service started: {name}"), 17);
        assert_eq!(longest_static_chunk_interp("{n /* } explanation */}"), 0);
        assert_eq!(longest_static_chunk_interp("{[]int{1, 2}} tail"), 5);
        assert_eq!(longest_static_chunk_interp("{f(\"}\")}!"), 1);
        assert_eq!(longest_static_chunk_interp("code {{x}} done"), 13);
        assert_eq!(longest_static_chunk_interp("{$\"a {x} b\"} tail"), 5);
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
