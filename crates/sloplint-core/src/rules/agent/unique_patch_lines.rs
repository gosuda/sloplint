use std::borrow::Cow;

pub(crate) const STRONG_LINE_CHARS: usize = 8;

pub(crate) fn normalize_line(line: &str) -> Cow<'_, str> {
    let trimmed = line.trim();
    let mut after_space = false;
    let already_normal = trimmed.chars().all(|c| {
        let accepted = !c.is_whitespace() || (c == ' ' && !after_space);
        after_space = c == ' ';
        accepted
    });
    if already_normal {
        Cow::Borrowed(trimmed)
    } else {
        Cow::Owned(trimmed.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}

pub(crate) fn is_comment_line(trimmed: &str) -> bool {
    if trimmed.starts_with("#[") {
        return false;
    }
    trimmed == "*"
        || trimmed.starts_with("* ")
        || [
            "//", "#", "--", ";", "%", "/*", "*/", "<!--", "\"\"\"", "'''",
        ]
        .iter()
        .any(|marker| trimmed.starts_with(marker))
}

pub(crate) fn is_strong_line(line: &str) -> bool {
    line.chars()
        .filter(|c| !c.is_whitespace())
        .nth(STRONG_LINE_CHARS - 1)
        .is_some()
}

pub(crate) fn is_substantive(trimmed: &str) -> bool {
    !is_field_init(trimmed)
        && !trimmed.starts_with(':')
        && !is_closer(trimmed)
        && !is_data_row(trimmed)
        && !is_assert_like(trimmed)
}

fn is_field_init(trimmed: &str) -> bool {
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    let mut rest = chars.as_str();
    while let Some(tail) = rest.strip_prefix(|c: char| c.is_ascii_alphanumeric() || c == '_') {
        rest = tail;
    }
    rest.trim_start_matches(' ').starts_with(':')
}

fn is_closer(trimmed: &str) -> bool {
    matches!(
        trimmed,
        "}" | "}," | "})" | "});" | "};" | "]" | "]," | "];" | ")" | ");" | "end"
    )
}

fn is_data_row(trimmed: &str) -> bool {
    let Some(first) = trimmed.chars().next() else {
        return false;
    };
    (first == '{'
        || first == '['
        || first == '"'
        || first == '\''
        || first == '-'
        || first.is_ascii_digit())
        && !trimmed.chars().any(|c| c == '(' || c == '=' || c == ';')
}

fn is_assert_like(trimmed: &str) -> bool {
    trimmed.contains("assert")
        || trimmed.starts_with("t.Fatal")
        || trimmed.starts_with("t.Error")
        || trimmed.starts_with("Fatal")
        || trimmed.starts_with("panic")
        || trimmed.starts_with("require.")
        || trimmed.starts_with("expect(")
}
