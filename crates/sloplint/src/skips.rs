use std::path::Path;

use sloplint_core::baseline::SkippedEntry;

use crate::scopes::Scopes;

pub(crate) fn collect(
    pipe_skipped: Vec<crate::pipeline::SkippedFile>,
    discovery_skipped: &[(std::path::PathBuf, &'static str)],
    scopes: Option<&Scopes>,
) -> Vec<SkippedEntry> {
    let mut out = Vec::with_capacity(pipe_skipped.len() + discovery_skipped.len());
    for (rel, reason) in pipe_skipped {
        if scopes.is_none_or(|scopes: &Scopes| scopes.contains(Path::new(&rel))) {
            out.push(entry(&escaped_valid(&rel), reason));
        }
    }
    for (rel, reason) in discovery_skipped {
        if scopes.is_none_or(|scopes: &Scopes| scopes.contains(rel.as_path())) {
            out.push(entry(&rel_string(rel), reason));
        }
    }
    out.sort();
    out.dedup();
    out
}

fn entry(path: &str, reason: &str) -> SkippedEntry {
    SkippedEntry {
        path: Box::from(path),
        reason: Box::from(reason),
    }
}

fn rel_string(path: &Path) -> String {
    if let Some(rel) = sloplint_core::text::normalize_rel_path(path) {
        return escaped_valid(&rel);
    }
    escaped_raw(path.as_os_str().as_encoded_bytes())
}

fn escaped_valid(text: &str) -> String {
    text.replace('\\', "\\\\")
}

fn escaped_raw(raw: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(raw.len());
    for chunk in raw.utf8_chunks() {
        out.push_str(&chunk.valid().replace('\\', "\\\\"));
        for &byte in chunk.invalid() {
            out.push_str("\\x");
            out.push(HEX[usize::from(byte >> 4)] as char);
            out.push(HEX[usize::from(byte & 15)] as char);
        }
    }
    out
}
