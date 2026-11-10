use crate::lang::Lang;

pub const MAX_KEY_SEGMENTS: usize = 4;

const JS_EXTENSIONS: [&str; 6] = ["ts", "tsx", "js", "jsx", "mjs", "cjs"];

pub fn library_keys(lang: Lang, path: &str) -> Vec<String> {
    let (dir, base) = split_dir(path);
    keys_for(lang, dir, stem_of(base))
}

pub fn test_keys(lang: Lang, path: &str) -> Vec<String> {
    let (dir, base) = split_dir(path);
    keys_for(lang, dir, strip_test_affix(stem_of(base)))
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
            if stem != "index" {
                push_segment(&mut segs, stem);
            }
        }
        _ => {
            push_all(&mut segs, &dir_segments(dir));
            push_segment(&mut segs, stem);
        }
    }
    suffix_keys(&segs)
}

pub fn import_keys(lang: Lang, importer: &str, spec: &str) -> Vec<String> {
    let mut segs = Vec::new();
    match lang {
        Lang::Python => python_segments(importer, spec, &mut segs),
        Lang::Rust => tail_segments(spec, &[':', '/'], &mut segs),
        Lang::TypeScript | Lang::Tsx | Lang::JavaScript => ts_segments(importer, spec, &mut segs),
        _ => generic_segments(importer, spec, &mut segs),
    }
    suffix_keys(&segs)
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

const TEST_SUFFIXES: [&str; 11] = [
    "_tests", "_test", "-test", ".test", ".Tests", "_spec", "-spec", ".spec", "Tests", "Test",
    "Spec",
];

const TEST_PREFIXES: [&str; 2] = ["test_", "test-"];

fn strip_test_affix(stem: &str) -> &str {
    let mut stem = stem;
    for suffix in TEST_SUFFIXES {
        if let Some(rest) = stem.strip_suffix(suffix) {
            if !rest.is_empty() {
                stem = rest;
            }
            break;
        }
    }
    for prefix in TEST_PREFIXES {
        if let Some(rest) = stem.strip_prefix(prefix) {
            if !rest.is_empty() {
                stem = rest;
            }
            break;
        }
    }
    stem
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
mod tests {
    use super::*;

    fn shared(lang: Lang, path: &str, spec: &str) -> bool {
        let lib = library_keys(lang, path);
        let imp = import_keys(lang, path, spec);
        lib.iter().any(|key| imp.contains(key))
    }

    #[test]
    fn huge_spec_yields_at_most_k_keys() {
        let spec = vec!["a"; 20_000].join(".");
        let keys = import_keys(Lang::Python, "tests/test_big.py", &spec);
        assert_eq!(keys.len(), MAX_KEY_SEGMENTS - 1);
    }

    #[test]
    fn deep_library_yields_at_most_k_keys() {
        let path = format!("{}/deep.py", vec!["d"; 2_000].join("/"));
        assert!(library_keys(Lang::Python, &path).len() <= MAX_KEY_SEGMENTS);
    }

    #[test]
    fn hyphen_and_underscore_segments_are_equal() {
        assert!(shared(
            Lang::Rust,
            "crates/log-core/src/tag.rs",
            "log_core::tag"
        ));
    }

    #[test]
    fn nested_rust_module_matches_its_import() {
        assert!(shared(
            Lang::Rust,
            "crates/parser/src/rules/lexer.rs",
            "parser::rules::lexer"
        ));
    }

    #[test]
    fn python_double_dot_resolves_against_importer_dir() {
        assert_eq!(
            import_keys(Lang::Python, "tests/unit/test_x.py", "..core"),
            ["core"]
        );
    }

    #[test]
    fn bare_import_covers_only_top_level_modules() {
        assert_eq!(import_keys(Lang::Python, "a/b/c.py", "utils"), ["utils"]);
        assert_eq!(
            library_keys(Lang::Python, "billing/utils.py"),
            ["billing/utils"]
        );
        assert!(!shared(Lang::Python, "billing/utils.py", "utils"));
    }

    #[test]
    fn python_src_layout_module_drops_the_src_root() {
        assert_eq!(library_keys(Lang::Python, "src/tag.py"), ["tag"]);
        assert!(shared(Lang::Python, "src/tag.py", "tag"));
        assert!(shared(Lang::Python, "src/app/user.py", "app.user"));
        assert!(!shared(Lang::Python, "vendor/src/tag.py", "tag"));
    }

    #[test]
    fn dotted_ts_basename_stays_one_segment() {
        assert!(
            library_keys(Lang::TypeScript, "src/app/user.service.ts")
                .contains(&"src/app/user.service".to_string())
        );
        assert!(shared(
            Lang::TypeScript,
            "src/app/user.service.ts",
            "../src/app/user.service"
        ));
    }

    #[test]
    fn package_marker_files_key_their_directory() {
        assert_eq!(library_keys(Lang::Python, "pkg/__init__.py"), ["pkg"]);
        assert_eq!(
            library_keys(Lang::Rust, "crates/x/src/foo/mod.rs"),
            ["x/foo"]
        );
        assert_eq!(
            library_keys(Lang::TypeScript, "src/ui/index.ts"),
            ["src/ui"]
        );
    }

    #[test]
    fn go_import_suffix_covers_nested_package() {
        let lib = library_keys(Lang::Go, "internal/tag/add.go");
        let imp = import_keys(
            Lang::Go,
            "cmd/app/app_test.go",
            "example.com/m/internal/tag",
        );
        assert_eq!(lib, ["internal/tag"]);
        assert!(imp.contains(&"internal/tag".to_string()));
    }

    #[test]
    fn test_keys_strip_each_affix_form() {
        let cases = [
            (Lang::CSharp, "src/WidgetTests.cs", "src/Widget"),
            (Lang::Java, "src/ServiceTest.java", "src/Service"),
            (Lang::Scala, "src/serviceSpec.scala", "src/service"),
            (Lang::Rust, "widget_tests.rs", "widget"),
            (Lang::Rust, "widget_test.rs", "widget"),
            (Lang::Bash, "widget-test.sh", "widget"),
            (Lang::JavaScript, "src/service.test.js", "src/service"),
            (Lang::TypeScript, "src/service.spec.ts", "src/service"),
            (Lang::Ocaml, "src/service.test.ml", "src/service"),
            (Lang::PowerShell, "Deploy.Tests.ps1", "Deploy"),
            (Lang::Ruby, "widget_spec.rb", "widget"),
            (Lang::Bash, "widget-spec.sh", "widget"),
            (Lang::Python, "pkg/test_widget.py", "pkg/widget"),
            (Lang::R, "test-service.R", "service"),
        ];
        for (lang, path, want) in cases {
            assert_eq!(test_keys(lang, path), [want], "affix in {path}");
        }
    }

    #[test]
    fn test_keys_never_strip_to_an_empty_stem() {
        assert_eq!(test_keys(Lang::Java, "Test.java"), ["Test"]);
        assert_eq!(test_keys(Lang::Python, "pkg/tests.py"), ["pkg/tests"]);
    }

    #[test]
    fn library_keys_keep_test_like_stems() {
        assert_eq!(library_keys(Lang::Scala, "src/Spec.scala"), ["src/Spec"]);
        assert_eq!(
            library_keys(Lang::Java, "src/ServiceTest.java"),
            ["src/ServiceTest"]
        );
    }
}
