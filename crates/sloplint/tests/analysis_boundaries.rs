#[path = "analysis_boundaries/boundary_caps.rs"]
mod boundary_caps;
#[path = "analysis_boundaries/cli_contract.rs"]
mod cli_contract;

use clap::Parser;
use sloplint::cli::{Cli, Command};
use sloplint::{Report, RunError, RunOptions, run};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::finding::Severity;
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
        std::fs::create_dir_all(d).unwrap_or_else(|e| panic!("mkdir failed: {e}"));
    }
    std::fs::write(&p, text).unwrap_or_else(|e| panic!("write failed {rel}: {e}"));
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
            .map(|s| {
                RulePattern::parse(s).unwrap_or_else(|e| panic!("filter parse failed: {s}: {e}"))
            })
            .collect(),
    )
}

/// Prints the capability skip, or fails when SLOPLINT_REQUIRE_FS_CAPS demands the capability,
/// so a runner that silently drops the capability coverage turns red in CI.
#[cfg(unix)]
fn skip_unless_fs_caps_required(context: &str, error: &std::io::Error) {
    if std::env::var_os("SLOPLINT_REQUIRE_FS_CAPS").is_some_and(|value| !value.is_empty()) {
        panic!("SLOPLINT_REQUIRE_FS_CAPS is set but the capability is missing: {context}: {error}");
    }
    eprintln!("SKIP (capability unavailable) {context}: {error}");
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
        std::fs::create_dir_all(home.join(name)).unwrap_or_else(|e| panic!("mkdir failed: {e}"));
    }
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(root)
        .args(["check", "--strictness", "standard", "--fail-on", "never"])
        .args(args)
        .env("XDG_CACHE_HOME", home.join("xdg"))
        .env("HOME", home.join("home"))
        .env("LOCALAPPDATA", home.join("local"))
        .output()
        .unwrap_or_else(|e| panic!("binary run failed: {e}"));
    assert!(
        out.status.success(),
        "bin failed: {:?}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

fn run_ok(o: &RunOptions) -> Report {
    run(o).unwrap_or_else(|e| panic!("run failed: {o:?}: {e}"))
}

// AA-S5: line extent start/end/end+1/end+2 and file-level suppression.
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
        "[[custom_rules]]\nid = \"custom/order-hit\"\nlanguage = \"rust\"\nseverity = \"minor\"\nmessage = \"order hit\"\nwhy = \"determinism probe\"\nfix = \"none\"\nquery = \"(call_expression) @match\"\n",
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
        let text =
            String::from_utf8(bytes.clone()).unwrap_or_else(|e| panic!("utf8 decode failed: {e}"));
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
    put(
        &root,
        "sloplint.toml",
        "[rules.\"complexity/nesting-depth\"]\nlevel = \"major\"\n",
    );
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
        .unwrap_or_else(|e| panic!("metadata failed: {e}"))
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
        assert_eq!(n.severity, Severity::Major);
    } else {
        let pe = r
            .findings
            .iter()
            .find(|f| &*f.rule == "meta/parse-error")
            .unwrap_or_else(|| panic!("deep input needs a finding or parse-error"));
        assert_eq!(&*pe.path, "src/deep.rs");
    }
}

// AA-R5: minor_at/major_at/critical_at/blocker_at line boundaries with real option keys.
#[test]
fn boundary_file_budget_line_thresholds() {
    let (_t, root) = tmp();
    put(
        &root,
        "sloplint.toml",
        "[rules.\"agent/file-token-budget\"]\nlevel = \"major\"\nminor_at = 8\nmajor_at = 10\ncritical_at = 15\nblocker_at = 20\n",
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
    assert_eq!(sev(7), None, "7 lines below minor_at reports nothing");
    assert_eq!(sev(8), Some(Severity::Minor), "8 lines hits minor_at");
    assert_eq!(sev(9), Some(Severity::Minor), "9 lines stays minor");
    assert_eq!(sev(10), Some(Severity::Major), "10 lines hits major_at");
    assert_eq!(sev(11), Some(Severity::Major), "11 lines stays major");
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
        "[rules.\"agent/file-token-budget\"]\nlevel = \"major\"\nminor_at = 1\nmajor_at = 2\ncritical_at = 3\nblocker_at = 4\n",
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
    assert_eq!(one.severity, Severity::Minor, "1 line hits minor_at");
    assert_eq!(one.line, 1);
    assert_eq!(one.column, 1);
    let two = r
        .findings
        .iter()
        .find(|f| &*f.path == "two.rs")
        .unwrap_or_else(|| panic!("two.rs must report"));
    assert_eq!(two.severity, Severity::Major, "2 lines hit major_at");
    assert_eq!(two.line, 1);
    assert_eq!(two.column, 1);
}

#[test]
fn boundary_cli_jobs_parsing() {
    let jobs_of = |args: &[&str]| -> Option<usize> {
        let cli =
            Cli::try_parse_from(args).unwrap_or_else(|e| panic!("cli parse failed {args:?}: {e}"));
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
        assert!(
            Cli::try_parse_from(["sloplint", "check", "-j", bad]).is_err(),
            "-j {bad} must be rejected"
        );
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

/// A capability skip must fail when SLOPLINT_REQUIRE_FS_CAPS demands the capability, and stay
/// a printed skip otherwise, so a runner that silently drops the coverage turns red in CI.
#[cfg(unix)]
#[test]
fn fs_capability_skip_fails_when_required() {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe failed: {e}"));
    let probe = |required: bool| {
        let mut command = std::process::Command::new(&exe);
        command
            .args([
                "fs_capability_gate_probe",
                "--exact",
                "--ignored",
                "--test-threads=1",
            ])
            .env_remove("SLOPLINT_REQUIRE_FS_CAPS");
        if required {
            command.env("SLOPLINT_REQUIRE_FS_CAPS", "1");
        }
        command
            .output()
            .unwrap_or_else(|e| panic!("probe spawn failed: {e}"))
    };
    let required = probe(true);
    assert!(
        !required.status.success(),
        "a missing filesystem capability must fail when SLOPLINT_REQUIRE_FS_CAPS is set: {:?}: {}",
        required.status,
        String::from_utf8_lossy(&required.stderr)
    );
    let tolerated = probe(false);
    assert!(
        tolerated.status.success(),
        "without SLOPLINT_REQUIRE_FS_CAPS the capability skip must stay graceful: {:?}: {}",
        tolerated.status,
        String::from_utf8_lossy(&tolerated.stderr)
    );
}

/// The gate probe behind [`fs_capability_skip_fails_when_required`]: the capability is absent,
/// so the gate must skip unless the environment demands the capability. Ignored so the armed
/// gate only runs through the regression harness, never in the suite with CI's env set.
#[cfg(unix)]
#[test]
#[ignore = "runs via fs_capability_skip_fails_when_required"]
fn fs_capability_gate_probe() {
    let error = std::io::Error::from_raw_os_error(92);
    skip_unless_fs_caps_required("filesystem cannot represent the 0xFF name", &error);
}
