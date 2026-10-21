#[path = "adversarial_boundaries/cli_contract.rs"]
mod cli_contract;

use clap::Parser;
use sloplint::cli::{Cli, Command};
use sloplint::{NameDecision, Report, RunError, RunOptions, Scopes, decide_file_name, run};
use sloplint_core::config::{Config, Mode, Strictness};
use sloplint_core::finding::Severity;
use sloplint_core::lang::Lang;
use sloplint_core::{RuleFilter, RulePattern};
use std::path::{Path, PathBuf};

fn tmp() -> (tempfile::TempDir, PathBuf) {
    let t = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let r = t.path().to_path_buf();
    (t, r)
}

fn put(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).unwrap_or_else(|e| panic!("mkdir: {e}"));
    }
    std::fs::write(&p, text).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

fn base(root: &Path) -> RunOptions {
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        use_cache: false,
        ..RunOptions::default()
    }
}

fn filt(ids: &[&str]) -> RuleFilter {
    RuleFilter(
        ids.iter()
            .map(|s| RulePattern::parse(s).unwrap_or_else(|e| panic!("filter {s}: {e}")))
            .collect(),
    )
}

fn bin_cold(root: &Path, home: &Path, format: &str, jobs: usize) -> Vec<u8> {
    let js = jobs.to_string();
    bin_in(root, home, &["--format", format, "-j", &js, "--no-cache"])
}

fn bin_warm(root: &Path, home: &Path, format: &str, jobs: usize) -> Vec<u8> {
    let js = jobs.to_string();
    bin_in(root, home, &["--format", format, "-j", &js])
}

/// Runs the shipped binary in `root` with its per-user cache and home directories inside
/// `home`, so concurrent tests never share a cache and none touches the runner's real one.
fn bin_in(root: &Path, home: &Path, args: &[&str]) -> Vec<u8> {
    for name in ["xdg", "home", "local"] {
        std::fs::create_dir_all(home.join(name)).unwrap_or_else(|e| panic!("mkdir: {e}"));
    }
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(root)
        .args(["check", "--strictness", "standard", "--fail-on", "never"])
        .args(args)
        .env("XDG_CACHE_HOME", home.join("xdg"))
        .env("HOME", home.join("home"))
        .env("LOCALAPPDATA", home.join("local"))
        .output()
        .unwrap_or_else(|e| panic!("bin: {e}"));
    assert!(
        out.status.success(),
        "bin failed: {:?}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

fn run_ok(o: &RunOptions) -> Report {
    run(o).unwrap_or_else(|e| panic!("run {o:?}: {e}"))
}

// AA-I11, AA-R1: limit-1 skipped, limit and limit+1 analyzed.
#[test]
fn boundary_byte_cap_limit_boundaries() {
    let (_t, root) = tmp();
    let body = "pub fn handle() {}\n";
    put(&root, "src/a.rs", body);
    let len = std::fs::metadata(root.join("src/a.rs"))
        .unwrap_or_else(|e| panic!("meta: {e}"))
        .len();
    for (limit, files, skipped, hits) in [(len - 1, 0, 1, 0), (len, 1, 0, 1), (len + 1, 1, 0, 1)] {
        put(
            &root,
            "sloplint.toml",
            &format!("[project]\nmax_file_bytes = {limit}\n"),
        );
        let r = run_ok(&RunOptions {
            strictness: Some(Strictness::Standard),
            mode: Some(Mode::Threshold),
            rule_filter: filt(&["grep/generic-name", "meta/*"]),
            ..base(&root)
        });
        assert_eq!(r.summary.files, files, "files at limit {limit}");
        assert_eq!(r.summary.skipped, skipped, "skipped at limit {limit}");
        let g: Vec<_> = r
            .findings
            .iter()
            .filter(|f| &*f.rule == "grep/generic-name")
            .collect();
        assert_eq!(g.len(), hits, "generic-name count at limit {limit}");
        if hits == 1 {
            assert_eq!(&*g[0].path, "src/a.rs");
            assert_eq!(g[0].line, 1);
            assert_eq!(g[0].severity, Severity::Warn);
        }
    }
}

// AA-I12, AA-R1: zero cap disables analysis; non-UTF8 content is a counted skip.
#[test]
fn boundary_zero_cap_and_non_utf8_skip() {
    let (_t, root) = tmp();
    put(&root, "src/a.rs", "pub fn handle() {}\n");
    put(&root, "sloplint.toml", "[project]\nmax_file_bytes = 0\n");
    let r = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name", "meta/*"]),
        ..base(&root)
    });
    assert_eq!(r.summary.files, 0, "zero cap analyzes nothing");
    assert_eq!(r.summary.skipped, 1, "zero cap counts the skip");
    assert!(r.findings.is_empty(), "zero cap reports nothing");
    let (_t2, root2) = tmp();
    put(&root2, "src/a.rs", "pub fn handle() {}\n");
    std::fs::write(root2.join("src/bad.rs"), [0xffu8, 0xfe, 0x41]).unwrap();
    let r2 = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name", "meta/*"]),
        ..base(&root2)
    });
    assert_eq!(r2.summary.files, 1, "only the valid file is analyzed");
    assert_eq!(r2.summary.skipped, 1, "non-UTF8 content is a counted skip");
    assert!(
        r2.findings.iter().all(|f| &*f.path != "src/bad.rs"),
        "non-UTF8 file reports nothing"
    );
    assert!(
        r2.findings
            .iter()
            .any(|f| &*f.rule == "grep/generic-name" && &*f.path == "src/a.rs" && f.line == 1),
        "valid file still reports"
    );
}

// AA-I15: a non-UTF-8 file name is a counted skip; the valid sibling still reports.
#[cfg(unix)]
#[test]
fn boundary_non_utf8_filename_is_skipped() {
    use std::os::unix::ffi::OsStrExt;

    let (_t, root) = tmp();
    put(&root, "src/a.rs", "pub fn handle() {}\n");
    let raw = std::ffi::OsStr::from_bytes(b"src/bad\xff.rs");
    if let Err(e) = std::fs::write(root.join(raw), b"pub fn handle() {}\n") {
        const EILSEQ: i32 = 92;
        assert_eq!(e.raw_os_error(), Some(EILSEQ), "write non-UTF-8 name: {e}");
        eprintln!(
            "SKIP boundary_non_utf8_filename_is_skipped: filesystem cannot represent the 0xFF name: {e}"
        );
        return;
    }
    let r = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name", "meta/*"]),
        ..base(&root)
    });
    assert_eq!(r.summary.files, 1, "only the valid file is analyzed");
    assert_eq!(r.summary.skipped, 1, "non-UTF-8 name is a counted skip");
    let generic: Vec<_> = r
        .findings
        .iter()
        .filter(|f| &*f.rule == "grep/generic-name")
        .collect();
    assert_eq!(generic.len(), 1, "exactly one generic-name finding");
    assert_eq!(
        &*generic[0].path, "src/a.rs",
        "finding cites the valid path"
    );
    assert_eq!(generic[0].line, 1);
    assert_eq!(generic[0].severity, Severity::Warn);
    assert!(
        r.findings.iter().all(|f| &*f.path == "src/a.rs"),
        "no finding may cite the unnamed file: {:?}",
        r.findings.iter().map(|f| &*f.path).collect::<Vec<_>>()
    );
}

#[test]
fn boundary_non_utf8_skip_decision_is_hermetic() {
    let config = Config::default();
    let valid = Path::new("src/bad\u{FFFD}.rs");
    let NameDecision::Analyze { rel, lang } = decide_file_name(valid, &config) else {
        panic!("valid U+FFFD name must analyze");
    };
    assert_eq!(rel, "src/bad\u{FFFD}.rs");
    assert_eq!(lang, Lang::Rust);
    let whole = Scopes::default();
    assert!(whole.contains(valid));
    let src = Scopes::from_relative([PathBuf::from("src")]);
    assert!(src.contains(valid));
    let other = Scopes::from_relative([PathBuf::from("other")]);
    assert!(!other.contains(valid));
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let bad = Path::new(std::ffi::OsStr::from_bytes(b"src/bad\xff.rs"));
        assert!(
            matches!(decide_file_name(bad, &config), NameDecision::Skipped),
            "invalid-byte name is a counted skip"
        );
        assert!(whole.contains(bad));
        assert!(src.contains(bad));
        assert!(!other.contains(bad));
        let explicit = Scopes::from_relative([valid.to_path_buf()]);
        assert!(explicit.contains(valid));
        assert!(
            !explicit.contains(bad),
            "lossy-equal names share no explicit file scope"
        );
    }
}

// AA-I14, AA-X9: component boundary, root scope, nonexistent and symlink-escape errors.
#[test]
fn boundary_scope_component_and_path_errors() {
    let (_tmpd, bx) = tmp();
    let root = bx.join("proj");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("srcx")).unwrap();
    put(&root, "src/a.rs", "pub fn handle() {}\n");
    put(&root, "srcx/a.rs", "pub fn handle() {}\n");
    let whole = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name"]),
        ..base(&root)
    });
    assert_eq!(whole.findings.len(), 2, "whole root sees both files");
    let scoped = run_ok(&RunOptions {
        paths: vec![root.join("src")],
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name"]),
        ..base(&root)
    });
    assert_eq!(scoped.findings.len(), 1, "src must not match srcx");
    assert_eq!(&*scoped.findings[0].path, "src/a.rs");
    assert_eq!(scoped.findings[0].line, 1);
    let whole_src = whole
        .findings
        .iter()
        .find(|f| &*f.path == "src/a.rs")
        .unwrap_or_else(|| panic!("missing src/a.rs"));
    assert_eq!(scoped.findings[0].rule, whole_src.rule);
    assert_eq!(scoped.findings[0].severity, whole_src.severity);
    let at_root = run_ok(&RunOptions {
        paths: vec![root.clone()],
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name"]),
        ..base(&root)
    });
    assert_eq!(
        at_root.findings.len(),
        whole.findings.len(),
        "root scope equals whole root"
    );
    let Err(e) = run(&RunOptions {
        paths: vec![root.join("missing.rs")],
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        ..base(&root)
    }) else {
        panic!("nonexistent path must fail");
    };
    assert!(matches!(e, RunError::PathNotFound { .. }), "got {e}");
    assert_eq!(e.exit_code(), 2);
    assert!(
        e.to_string().contains("missing.rs"),
        "error names path: {e}"
    );
    #[cfg(unix)]
    {
        let outside = bx.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        let Err(le) = run(&RunOptions {
            paths: vec![root.join("link")],
            strictness: Some(Strictness::Standard),
            mode: Some(Mode::Threshold),
            ..base(&root)
        }) else {
            panic!("symlink escape must fail");
        };
        assert!(matches!(le, RunError::PathOutsideRoot { .. }), "got {le}");
        assert_eq!(le.exit_code(), 2);
    }
}

// AA-S5: line extent start/end/end+1/end+2 and file-level suppression.
#[test]
fn boundary_suppression_line_extent_and_file_level() {
    let (_t, root) = tmp();
    put(
        &root,
        "src/extent.rs",
        "pub fn handle() {}\n// sloplint-allow(grep/generic-name): keep\npub fn handle() {}\npub fn handle() {}\n",
    );
    put(
        &root,
        "src/file.rs",
        "// sloplint-allow-file(grep/generic-name): whole file\npub fn handle() {}\npub fn handle() {}\n",
    );
    let r = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name", "meta/*"]),
        ..base(&root)
    });
    let mut lines: Vec<u32> = r
        .findings
        .iter()
        .filter(|f| &*f.rule == "grep/generic-name" && &*f.path == "src/extent.rs")
        .map(|f| f.line)
        .collect();
    lines.sort();
    assert_eq!(lines, vec![1, 4], "line 3 suppressed as end+1, line 4 not");
    assert!(
        r.findings
            .iter()
            .all(|f| &*f.path != "src/file.rs" || &*f.rule != "grep/generic-name"),
        "file-level suppresses the whole file"
    );
    assert!(
        r.findings
            .iter()
            .all(|f| &*f.rule != "meta/unused-suppression"),
        "both suppressions were used: {:?}",
        r.findings.iter().map(|f| &*f.rule).collect::<Vec<_>>()
    );
}

// AA-X7: unknown exact id, unknown family, and meta/* family are invalid.
#[test]
fn boundary_suppression_unknown_and_meta_family() {
    let (_t, root) = tmp();
    put(
        &root,
        "src/lib.rs",
        "// sloplint-allow(no/such-rule): reason\npub fn handle() {}\n// sloplint-allow(bogus/*): reason\npub fn handle() {}\n// sloplint-allow(meta/*): reason\npub fn handle() {}\n",
    );
    let r = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name", "meta/*"]),
        ..base(&root)
    });
    for line in [1u32, 3, 5] {
        let inv = r
            .findings
            .iter()
            .find(|f| &*f.rule == "meta/invalid-suppression" && f.line == line)
            .unwrap_or_else(|| panic!("missing invalid at line {line}"));
        assert_eq!(&*inv.path, "src/lib.rs");
        assert!(
            inv.message.contains("unknown rule"),
            "line {line}: {}",
            inv.message
        );
    }
    let mut glines: Vec<u32> = r
        .findings
        .iter()
        .filter(|f| &*f.rule == "grep/generic-name")
        .map(|f| f.line)
        .collect();
    glines.sort();
    assert_eq!(glines, vec![2, 4, 6], "invalid suppressions hide nothing");
}

#[test]
fn boundary_exact_meta_and_multiline_reason() {
    let (_t, root) = tmp();
    put(
        &root,
        "src/broken.rs",
        "// sloplint-allow(meta/parse-error): documented\nfn broken( {\n",
    );
    put(
        &root,
        "src/multi.rs",
        "/* sloplint-allow(grep/generic-name):\nkeep */\npub fn handle() {}\n",
    );
    let r = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name", "meta/*"]),
        ..base(&root)
    });
    let pe = r
        .findings
        .iter()
        .find(|f| &*f.rule == "meta/parse-error" && &*f.path == "src/broken.rs")
        .unwrap_or_else(|| panic!("parse-error hidden by exact meta suppression"));
    assert_eq!(pe.line, 2, "parse-error stays on the broken line");
    let inv = r
        .findings
        .iter()
        .find(|f| &*f.rule == "meta/invalid-suppression" && &*f.path == "src/multi.rs")
        .unwrap_or_else(|| panic!("multiline reason must be missing"));
    assert_eq!(inv.line, 1);
    assert!(
        inv.message.contains("missing reason"),
        "got {}",
        inv.message
    );
    assert!(
        r.findings
            .iter()
            .any(|f| &*f.rule == "grep/generic-name" && &*f.path == "src/multi.rs" && f.line == 3),
        "leaked reason must not suppress"
    );
}

// AA-O2, AA-O6: agent and JSON bytes stable across jobs and cache, with custom hits.
#[test]
fn boundary_output_bytes_stable() {
    let (_t, root) = tmp();
    put(
        &root,
        "sloplint.toml",
        "[[custom_rules]]\nid = \"custom/order-hit\"\nlanguage = \"rust\"\nseverity = \"warn\"\nmessage = \"order hit\"\nwhy = \"determinism probe\"\nfix = \"none\"\nquery = \"(call_expression) @match\"\n",
    );
    put(
        &root,
        "src/a.rs",
        "pub fn handle() { foo(); }\nfn foo() {}\n",
    );
    put(
        &root,
        "src/b.rs",
        "pub fn handle() { foo(); }\nfn foo() {}\n",
    );
    put(
        &root,
        "src/c.rs",
        "pub fn handle() { foo(); }\nfn foo() {}\n",
    );
    let (_home_dir, home) = tmp();
    let a1 = bin_cold(&root, &home, "agent", 1);
    let a2 = bin_cold(&root, &home, "agent", 2);
    let a8 = bin_cold(&root, &home, "agent", 8);
    let j1 = bin_cold(&root, &home, "json", 1);
    let j2 = bin_cold(&root, &home, "json", 2);
    let j8 = bin_cold(&root, &home, "json", 8);
    for (bytes, what) in [(&a1, "agent"), (&j1, "json")] {
        let text = String::from_utf8(bytes.clone()).unwrap_or_else(|e| panic!("utf8: {e}"));
        assert!(
            text.contains("custom/order-hit"),
            "{what} lacks custom ordering"
        );
        assert!(text.contains("src/a.rs"), "{what} lacks expected path");
    }
    assert_eq!(a1, a2, "shipped agent bytes jobs 1 vs 2");
    assert_eq!(a1, a8, "shipped agent bytes jobs 1 vs 8");
    assert_eq!(j1, j2, "shipped json bytes jobs 1 vs 2");
    assert_eq!(j1, j8, "shipped json bytes jobs 1 vs 8");
    let wa1 = bin_warm(&root, &home, "agent", 1);
    let wa8 = bin_warm(&root, &home, "agent", 8);
    let wj1 = bin_warm(&root, &home, "json", 1);
    let wj8 = bin_warm(&root, &home, "json", 8);
    let wa1b = bin_warm(&root, &home, "agent", 1);
    assert_eq!(a1, wa1, "cold vs warm shipped agent bytes");
    assert_eq!(j1, wj8, "cold vs warm shipped json bytes");
    assert_eq!(wa1, wa8, "warm jobs 1 vs 8 shipped agent bytes");
    assert_eq!(wa1, wa1b, "warm repeat shipped agent bytes");
    assert_eq!(wj1, wj8, "warm jobs 1 vs 8 shipped json bytes");
}

// AA-R3: bounded deep nesting yields a nesting finding or a parse error.
#[test]
fn boundary_deep_nesting_bounded() {
    let (_t, root) = tmp();
    let depth = 10_000usize;
    let mut src = String::from("fn deep() {\n");
    for i in 0..depth {
        src.push_str(&format!("if a{i} {{\n"));
    }
    src.push_str("do_it();\n");
    for _ in 0..depth {
        src.push_str("}\n");
    }
    src.push_str("}\n");
    put(&root, "src/deep.rs", &src);
    let bytes = std::fs::metadata(root.join("src/deep.rs"))
        .unwrap_or_else(|e| panic!("meta: {e}"))
        .len();
    assert!(
        bytes < 2_097_152,
        "input stays below default max_file_bytes: {bytes}"
    );
    let r = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["complexity/nesting-depth", "meta/*"]),
        jobs: 2,
        ..base(&root)
    });
    assert_eq!(r.summary.skipped, 0, "bounded input must be analyzed");
    if let Some(n) = r
        .findings
        .iter()
        .find(|f| &*f.rule == "complexity/nesting-depth")
    {
        assert_eq!(&*n.path, "src/deep.rs");
        assert_eq!(n.line, 1);
        assert_eq!(n.severity, Severity::Deny);
        assert!(n.message.contains("nesting depth"), "got {}", n.message);
    } else {
        let pe = r
            .findings
            .iter()
            .find(|f| &*f.rule == "meta/parse-error")
            .unwrap_or_else(|| panic!("deep input needs a finding or parse-error"));
        assert_eq!(&*pe.path, "src/deep.rs");
    }
}

// AA-R5: warn_at/deny_at line boundaries N-1/N/N+1 with real option keys.
#[test]
fn boundary_file_budget_line_thresholds() {
    let (_t, root) = tmp();
    put(
        &root,
        "sloplint.toml",
        "[rules.\"agent/file-token-budget\"]\nwarn_at = 8\ndeny_at = 10\n",
    );
    for n in [7u32, 8, 9, 10, 11] {
        let repeat = usize::try_from(n).unwrap_or_else(|e| panic!("u32 fits usize: {e}"));
        put(
            &root,
            &format!("src/a{n}.rs"),
            &"fn f() {}\n".repeat(repeat),
        );
    }
    let r = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["agent/file-token-budget"]),
        ..base(&root)
    });
    let sev = |n: u32| {
        r.findings
            .iter()
            .find(|f| *f.path == format!("src/a{n}.rs"))
            .map(|f| f.severity)
    };
    assert_eq!(sev(7), None, "7 lines below warn_at reports nothing");
    assert_eq!(sev(8), Some(Severity::Warn), "8 lines hits warn_at");
    assert_eq!(sev(9), Some(Severity::Warn), "9 lines stays warn");
    assert_eq!(sev(10), Some(Severity::Deny), "10 lines hits deny_at");
    assert_eq!(sev(11), Some(Severity::Deny), "11 lines stays deny");
    for n in [8u32, 9, 10, 11] {
        let f = r
            .findings
            .iter()
            .find(|f| *f.path == format!("src/a{n}.rs"))
            .unwrap_or_else(|| panic!("missing a{n}"));
        assert_eq!(f.line, 1);
        assert_eq!(f.column, 1);
    }
}

// AA-R5: zero-line and missing-final-newline files count exact lines.
#[test]
fn boundary_file_budget_empty_and_no_final_newline() {
    let (_t, root) = tmp();
    put(&root, "empty.rs", "");
    put(&root, "one.rs", "fn first() {}");
    put(&root, "two.rs", "fn first() {}\nfn second() {}");
    put(
        &root,
        "sloplint.toml",
        "[rules.\"agent/file-token-budget\"]\nwarn_at = 1\ndeny_at = 2\n",
    );
    let r = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["agent/file-token-budget"]),
        ..base(&root)
    });
    assert_eq!(r.summary.files, 3, "all three files are analyzed");
    assert_eq!(r.summary.skipped, 0, "no file is skipped");
    let empty = r.findings.iter().find(|f| &*f.path == "empty.rs");
    assert!(empty.is_none(), "0 lines stays clean: {empty:?}");
    assert_eq!(r.findings.len(), 2, "only one.rs and two.rs report");
    let one = r
        .findings
        .iter()
        .find(|f| &*f.path == "one.rs")
        .unwrap_or_else(|| panic!("one.rs must report"));
    assert_eq!(one.severity, Severity::Warn, "1 line hits warn_at");
    assert_eq!(one.line, 1);
    assert_eq!(one.column, 1);
    let two = r
        .findings
        .iter()
        .find(|f| &*f.path == "two.rs")
        .unwrap_or_else(|| panic!("two.rs must report"));
    assert_eq!(two.severity, Severity::Deny, "2 lines hit deny_at");
    assert_eq!(two.line, 1);
    assert_eq!(two.column, 1);
}

// AA-I13: jobs parser accepts 1..=512 and names the range on error.
#[test]
fn boundary_cli_jobs_parsing() {
    let jobs_of = |args: &[&str]| -> Option<usize> {
        let cli = Cli::try_parse_from(args).unwrap_or_else(|e| panic!("parse {args:?}: {e}"));
        match cli.command {
            Some(Command::Check(c)) => c.run.jobs,
            Some(_) => panic!("unexpected subcommand for {args:?}"),
            None => cli.check.run.jobs,
        }
    };
    assert_eq!(jobs_of(&["sloplint", "check", "-j", "1"]), Some(1));
    assert_eq!(jobs_of(&["sloplint", "check", "-j", "2"]), Some(2));
    assert_eq!(jobs_of(&["sloplint", "check", "-j", "511"]), Some(511));
    assert_eq!(jobs_of(&["sloplint", "check", "-j", "512"]), Some(512));
    assert_eq!(jobs_of(&["sloplint", "-j", "2"]), Some(2));
    for bad in ["0", "513"] {
        let e = Cli::try_parse_from(["sloplint", "check", "-j", bad]).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("job count"), "jobs error names jobs: {msg}");
        assert!(msg.contains("1 to 512"), "jobs error names range: {msg}");
        assert!(msg.contains(bad), "jobs error echoes value: {msg}");
    }
}

// AA-T5: two concurrent cached runs equal the cold control; later warm stays identical.
#[test]
fn boundary_concurrent_cache_runs_match_cold() {
    use std::sync::{Arc, Barrier};
    let (_t, root) = tmp();
    let (_home_dir, home) = tmp();
    put(
        &root,
        "src/a.rs",
        "pub fn handle() {}\npub fn run_task() {}\n",
    );
    put(&root, "src/b.rs", "pub fn handle() {}\nfn foo() {}\n");
    let control = bin_cold(&root, &home, "agent", 2);
    let barrier = Arc::new(Barrier::new(2));
    std::thread::scope(|s| {
        let outs: Vec<_> = (0..2)
            .map(|_| {
                s.spawn(|| {
                    barrier.wait();
                    bin_warm(&root, &home, "agent", 2)
                })
            })
            .collect();
        for h in outs {
            let bytes = h.join().unwrap_or_else(|_| panic!("worker panicked"));
            assert_eq!(bytes, control, "concurrent shipped agent bytes");
        }
    });
    let warm = bin_warm(&root, &home, "agent", 2);
    assert_eq!(warm, control, "post-race warm shipped agent bytes");
}
