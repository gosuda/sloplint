use crate::lang::Lang;

pub const MAX_KEY_SEGMENTS: usize = 4;

const JS_EXTENSIONS: [&str; 6] = ["ts", "tsx", "js", "jsx", "mjs", "cjs"];

pub fn library_keys(lang: Lang, path: &str) -> Vec<String> {
    let (dir, base) = split_dir(path);
    keys_for(lang, dir, stem_of(base))
}

pub fn test_keys(lang: Lang, path: &str) -> Vec<String> {
    let (dir, base) = split_dir(path);
    if lang == Lang::CSharp {
        return csharp_test_keys(dir, strip_test_affix(stem_of(base)));
    }
    keys_for(lang, dir, strip_test_affix(stem_of(base)))
}

fn csharp_test_keys(dir: &str, stem: &str) -> Vec<String> {
    let mut segs = Vec::new();
    for part in dir_segments(dir) {
        push_dotted_segment(&mut segs, strip_csharp_test_dir(part));
    }
    push_segment(&mut segs, stem);
    suffix_keys(&segs)
}

fn strip_csharp_test_dir(seg: &str) -> &str {
    if let Some(rest) = seg.strip_suffix(".Tests")
        && !rest.is_empty()
    {
        return rest;
    }
    if let Some(rest) = seg.strip_suffix(".Test")
        && !rest.is_empty()
    {
        return rest;
    }
    seg
}

fn keys_for(lang: Lang, dir: &str, stem: &str) -> Vec<String> {
    let mut segs = Vec::new();
    match lang {
        Lang::Go => {
            push_all(&mut segs, &dir_segments(dir));
            if dir.is_empty() {
                push_segment(&mut segs, stem);
            }
        }
        Lang::Rust => {
            let dirs = dir_segments(dir);
            let mut rest_from = 0;
            if let Some(i) = dirs.iter().rposition(|seg| *seg == "src") {
                if i > 0 {
                    push_segment(&mut segs, dirs[i - 1]);
                }
                rest_from = i + 1;
            }
            push_all(&mut segs, &dirs[rest_from..]);
            if stem != "mod" {
                push_segment(&mut segs, stem);
            }
        }
        Lang::Python => {
            let dirs = dir_segments(dir);
            let module_dirs = match dirs.split_first() {
                Some((&"src", rest)) => rest,
                _ => &dirs[..],
            };
            push_all(&mut segs, module_dirs);
            if stem != "__init__" {
                push_segment(&mut segs, stem);
            }
        }
        Lang::TypeScript | Lang::Tsx | Lang::JavaScript => {
            push_all(&mut segs, &dir_segments(dir));
            push_segment(&mut segs, stem);
            drop_trailing_index(&mut segs);
        }
        _ => {
            push_lang_dirs(&mut segs, lang, dir);
            push_segment(&mut segs, stem);
        }
    }
    suffix_keys(&segs)
}

pub fn import_keys(lang: Lang, importer: &str, spec: &str) -> Vec<String> {
    match lang {
        Lang::Python => {
            let mut segs = Vec::new();
            python_segments(importer, spec, &mut segs);
            suffix_keys(&segs)
        }
        Lang::Rust => rust_import_keys(importer, spec),
        Lang::TypeScript | Lang::Tsx | Lang::JavaScript => {
            let mut segs = Vec::new();
            ts_segments(importer, spec, &mut segs);
            drop_trailing_index(&mut segs);
            suffix_keys(&segs)
        }
        Lang::Java | Lang::Kotlin | Lang::Scala | Lang::CSharp => {
            let mut segs = Vec::new();
            dotted_segments(spec, &mut segs);
            suffix_keys(&segs)
        }
        _ => {
            let mut segs = Vec::new();
            generic_segments(importer, spec, &mut segs);
            suffix_keys(&segs)
        }
    }
}

fn rust_import_keys(importer: &str, spec: &str) -> Vec<String> {
    let mut full = Vec::new();
    tail_segments(spec, &[':', '/'], &mut full);
    if full.len() < 2 {
        return suffix_keys(&full);
    }
    if let Some(keys) = rust_crate_mapped_keys(importer, spec, &full) {
        return keys;
    }
    if !rust_drop_allowed(importer, spec, &full) {
        return suffix_keys(&full);
    }
    let mut keys = suffix_keys(&full);
    for key in suffix_keys(&full[1..]) {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    if full.len() > 2 && !keys.contains(&full[1]) {
        keys.push(full[1].clone());
    }
    keys
}
fn rust_crate_mapped_keys(importer: &str, spec: &str, full: &[String]) -> Option<Vec<String>> {
    if rust_root_of(spec) != "crate" {
        return None;
    }
    let crate_name = rust_crate_of(importer)?;
    if rust_seg_count(spec) != full.len() || full[0].as_str() != "crate" {
        return None;
    }
    let mut replaced = full.to_vec();
    replaced[0] = crate_name.replace('-', "_");
    let mut keys = suffix_keys(&replaced);
    for key in suffix_keys(&replaced[1..]) {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    if replaced.len() > 2 && !keys.contains(&replaced[1]) {
        keys.push(replaced[1].clone());
    }
    Some(keys)
}
fn rust_drop_allowed(importer: &str, spec: &str, full: &[String]) -> bool {
    let root = rust_root_of(spec);
    if root == "crate" {
        return rust_seg_count(spec) == full.len();
    }
    if root.is_empty() || matches!(root, "std" | "core" | "alloc" | "super" | "self") {
        return false;
    }
    if rust_seg_count(spec) != full.len() {
        return false;
    }
    rust_crate_of(importer).is_some_and(|name| rust_names_equal(root, name))
}
fn rust_root_of(spec: &str) -> &str {
    spec.trim_start_matches(':')
        .split([':', '/'])
        .next()
        .unwrap_or("")
}
fn rust_crate_of(importer: &str) -> Option<&str> {
    let rest = importer.strip_prefix("crates/")?;
    let name = rest.split('/').next()?;
    if name.is_empty() {
        return None;
    }
    Some(name)
}
fn rust_names_equal(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .all(|(x, y)| x == y || x == b'-' && y == b'_' || x == b'_' && y == b'-')
}
fn rust_seg_count(spec: &str) -> usize {
    spec.split([':', '/']).filter(|s| !s.is_empty()).count()
}

fn dotted_segments(spec: &str, segs: &mut Vec<String>) {
    tail_segments(spec, &['.', '/'], segs);
}

fn suffix_keys(segs: &[String]) -> Vec<String> {
    let mut kept: Vec<&str> = Vec::new();
    for seg in segs {
        if seg.as_str() == "__tests__" {
            continue;
        }
        if kept.is_empty() && (seg.as_str() == "tests" || seg.as_str() == "test") {
            continue;
        }
        kept.push(seg);
    }
    let total = kept.len();
    if total == 0 {
        return Vec::new();
    }
    if total == 1 {
        return vec![kept[0].to_string()];
    }
    let longest = total.min(MAX_KEY_SEGMENTS);
    let mut keys = Vec::with_capacity(longest - 1);
    for len in 2..=longest {
        keys.push(kept[total - len..].join("/"));
    }
    keys
}

fn push_all(segs: &mut Vec<String>, parts: &[&str]) {
    for part in parts {
        push_segment(segs, part);
    }
}

fn drop_trailing_index(segs: &mut Vec<String>) {
    if segs.last().is_some_and(|seg| seg == "index") {
        segs.pop();
    }
}

fn push_segment(segs: &mut Vec<String>, seg: &str) {
    if seg.is_empty() {
        return;
    }
    if seg.contains('-') {
        segs.push(seg.replace('-', "_"));
    } else {
        segs.push(seg.to_string());
    }
}
fn push_lang_dirs(segs: &mut Vec<String>, lang: Lang, dir: &str) {
    match lang {
        Lang::Java | Lang::Kotlin | Lang::Scala | Lang::CSharp => push_dotted_dirs(segs, dir),
        _ => push_all(segs, &dir_segments(dir)),
    }
}
fn push_dotted_dirs(segs: &mut Vec<String>, dir: &str) {
    for part in dir_segments(dir) {
        push_dotted_segment(segs, part);
    }
}
fn push_dotted_segment(segs: &mut Vec<String>, seg: &str) {
    for part in seg.split('.') {
        push_segment(segs, part);
    }
}

fn split_dir(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(i) => (&path[..i], &path[i + 1..]),
        None => ("", path),
    }
}

fn stem_of(base: &str) -> &str {
    match base.rfind('.') {
        Some(i) if i > 0 => &base[..i],
        _ => base,
    }
}

const TEST_SUFFIXES: [&str; 15] = [
    "_unittest",
    "_tests",
    "_test",
    "-test",
    ".test",
    ".Tests",
    ".tests",
    "_spec",
    "-spec",
    ".spec",
    "Tests",
    "Test",
    "Spec",
    "Suite",
    "IT",
];

const TEST_PREFIXES: [&str; 2] = ["test_", "test-"];

fn strip_test_affix(stem: &str) -> &str {
    let mut stem = stem;
    for suffix in TEST_SUFFIXES {
        if let Some(rest) = stem.strip_suffix(suffix) {
            if should_strip_suffix(suffix, rest) {
                stem = rest;
            }
            break;
        }
    }
    for prefix in TEST_PREFIXES {
        if let Some(rest) = stem.strip_prefix(prefix) {
            if should_strip_prefix(prefix, rest) {
                stem = rest;
            }
            break;
        }
    }
    stem
}

fn should_strip_suffix(suffix: &str, rest: &str) -> bool {
    if rest.is_empty() {
        return false;
    }
    if suffix == "IT" {
        return rest.bytes().last().is_some_and(|b| b.is_ascii_lowercase());
    }
    true
}

fn should_strip_prefix(_prefix: &str, rest: &str) -> bool {
    !rest.is_empty()
}

fn dir_segments(dir: &str) -> Vec<&str> {
    dir.split('/').filter(|seg| !seg.is_empty()).collect()
}

fn python_segments(importer: &str, spec: &str, segs: &mut Vec<String>) {
    let dots = spec.bytes().take_while(|byte| *byte == b'.').count();
    let mut rest = &spec[dots..];
    while let Some(stripped) = rest.strip_prefix('/') {
        rest = stripped;
    }
    tail_segments(rest, &['.', '/'], segs);
    if dots == 0 || segs.len() >= MAX_KEY_SEGMENTS {
        return;
    }
    let dirs = dir_segments(split_dir(importer).0);
    let keep = (dots - 1).min(dirs.len());
    let upper = dirs.len() - keep;
    let lower = upper.saturating_sub(MAX_KEY_SEGMENTS - segs.len());
    let mut resolved: Vec<String> = Vec::with_capacity(segs.len() + upper - lower);
    push_all(&mut resolved, &dirs[lower..upper]);
    resolved.append(segs);
    *segs = resolved;
}

fn generic_segments(importer: &str, spec: &str, segs: &mut Vec<String>) {
    ts_segments(importer, spec, segs);
    if spec.contains('/') {
        strip_generic_extension(segs.last_mut());
    }
}

fn strip_generic_extension(seg: Option<&mut String>) {
    let Some(seg) = seg else {
        return;
    };
    if let Some(dot) = seg.rfind('.')
        && dot > 0
    {
        seg.truncate(dot);
    }
}

fn ts_segments(importer: &str, spec: &str, segs: &mut Vec<String>) {
    if !spec.starts_with('.') {
        if spec == "@" || spec == "~" {
            return;
        }
        if let Some(rest) = spec.strip_prefix("@/").or_else(|| spec.strip_prefix("~/")) {
            if rest.is_empty() {
                return;
            }
            let mut aliased = String::from("src/");
            aliased.push_str(rest);
            tail_segments(&aliased, &['/'], segs);
            return;
        }
        tail_segments(spec, &['/'], segs);
        return;
    }
    let mut parts: Vec<&str> = dir_segments(split_dir(importer).0);
    for seg in spec.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                let _ = parts.pop();
            }
            _ => parts.push(seg),
        }
    }
    let start = parts.len().saturating_sub(MAX_KEY_SEGMENTS);
    push_all(segs, &parts[start..]);
    strip_js_extension(segs.last_mut());
}

fn strip_js_extension(seg: Option<&mut String>) {
    let Some(seg) = seg else {
        return;
    };
    if let Some(dot) = seg.rfind('.')
        && dot > 0
        && JS_EXTENSIONS.contains(&&seg[dot + 1..])
    {
        seg.truncate(dot);
    }
}

fn tail_segments(spec: &str, seps: &[char], segs: &mut Vec<String>) {
    let mut end = spec.len();
    while segs.len() < MAX_KEY_SEGMENTS {
        let start = spec[..end].rfind(seps).map_or(0, |i| i + 1);
        push_segment(segs, &spec[start..end]);
        if start == 0 {
            break;
        }
        end = start - 1;
    }
    segs.reverse();
}

#[cfg(test)]
#[path = "module_keys_tests.rs"]
mod tests;
