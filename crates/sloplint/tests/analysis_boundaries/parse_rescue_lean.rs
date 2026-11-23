//! Per-command parse recovery for Lean: one unparseable command must not
//! redact the file. The generic span rescue gives up when the only fault is
//! a hidden `MISSING` (no reachable `ERROR`) or when recovery wraps the
//! file in an `ERROR` node (even an `ERROR` root); the Lean command rescue
//! then blanks whole column-0 command groups and re-analyzes the rest.
//!
//! Each test names the Batteries shape it pins: `MyNat/Inequality.lean`
//! (hidden `MISSING` from `(.≤.)`), the braced `cases-with` block, and the
//! `for`-loop plus `if`-without-`else` shape that breaks the `module` rule
//! into an `ERROR` root.

use super::{base, put, run_ok, tmp};

/// Hidden `MISSING` with healthy commands on both sides: the parse error
/// points at the bad command, not the end of the file, and the neighbors
/// are still analyzed.
#[test]
fn boundary_lean_hidden_missing_keeps_neighbor_findings() {
    let (_t, root) = tmp();
    put(
        &root,
        "src/inequality.lean",
        "import Lean\n\
         \n\
         open Lean Meta\n\
         \n\
         def good (x : Nat) : Nat := x + 1\n\
         \n\
         theorem le_mid : MyNat.le = (.≤.) := rfl\n\
         \n\
         def after (y : Nat) : Nat := y * 2\n\
         \n\
         open Foo.Bar\n",
    );
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Strict),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["grep/no-wildcard-import", "meta/*"]),
        ..base(&root)
    });
    let pe = r
        .findings
        .iter()
        .find(|f| &*f.rule == "meta/parse-error" && &*f.path == "src/inequality.lean")
        .unwrap_or_else(|| panic!("hidden MISSING must still be reported: {:?}", r.findings));
    assert_eq!(
        pe.line, 7,
        "the parse error must point at the bad command, not end-of-file"
    );
    let wildcards: Vec<_> = r
        .findings
        .iter()
        .filter(|f| &*f.rule == "grep/no-wildcard-import" && &*f.path == "src/inequality.lean")
        .collect();
    assert_eq!(
        wildcards.len(),
        3,
        "both `open` commands must still be reported: {:?}",
        r.findings
    );
    assert!(
        wildcards.iter().all(|f| f.line == 3 || f.line == 11),
        "no finding may move onto the blanked command: {:?}",
        wildcards
    );
}

/// Braced tactic block whose error recovery swallows the following command:
/// the prefix is still analyzed, the error is reported at the bad command,
/// and the swallowed command stays silent (its lines are tainted).
#[test]
fn boundary_lean_braced_block_keeps_prefix_and_taints_swallowed_command() {
    let (_t, root) = tmp();
    put(
        &root,
        "src/braced.lean",
        "import Lean\n\
         \n\
         open Lean Meta\n\
         \n\
         def good (x : Nat) : Nat := x + 1\n\
         \n\
         example (h : A ∨ B) : True := by\n\
         \x20 constructor\n\
         \x20 {\n\
         \x20   cases h with\n\
         \x20   | inl a => trivial\n\
         \x20   | inr b => trivial\n\
         \x20 }\n\
         \n\
         open Foo.Bar\n",
    );
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Strict),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["grep/no-wildcard-import", "meta/*"]),
        ..base(&root)
    });
    let pe = r
        .findings
        .iter()
        .find(|f| &*f.rule == "meta/parse-error" && &*f.path == "src/braced.lean")
        .unwrap_or_else(|| panic!("the braced block must still be reported: {:?}", r.findings));
    assert_eq!(pe.line, 7, "the parse error must point at the bad command");
    let wildcards: Vec<_> = r
        .findings
        .iter()
        .filter(|f| &*f.rule == "grep/no-wildcard-import" && &*f.path == "src/braced.lean")
        .collect();
    assert_eq!(
        wildcards.len(),
        2,
        "only the surviving `open` may report: {:?}",
        r.findings
    );
    assert!(
        wildcards.iter().all(|f| f.line == 3),
        "the swallowed `open` judges truncated code and must stay silent: {:?}",
        wildcards
    );
}

/// `ERROR`-root whole-file wrap: the `for`-loop plus `if`-without-`else`
/// shape breaks the `module` rule itself, so no top-level error span exists
/// to excise. The bad command is blanked by its dropped-token gap while the
/// healthy commands are still analyzed.
#[test]
fn boundary_lean_error_root_keeps_healthy_commands() {
    let (_t, root) = tmp();
    put(
        &root,
        "src/hashset.lean",
        "import Lean\n\
         \n\
         open Lean Meta Std.HashSet\n\
         \n\
         variable [BEq α] [Hashable α]\n\
         \n\
         def good (x : Nat) : Nat := x\n\
         \n\
         def anyM [Monad m] (s : HashSet α) (f : α → m Bool) : m Bool := do\n\
         \x20 for a in s do\n\
         \x20   if ← f a then\n\
         \x20     return true\n\
         \x20 return false\n\
         \n\
         def after (y : Nat) : Nat := y\n",
    );
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Strict),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["grep/no-wildcard-import", "meta/*"]),
        ..base(&root)
    });
    let pe = r
        .findings
        .iter()
        .find(|f| &*f.rule == "meta/parse-error" && &*f.path == "src/hashset.lean")
        .unwrap_or_else(|| {
            panic!(
                "the wrapped command must still be reported: {:?}",
                r.findings
            )
        });
    assert_eq!(
        pe.line, 9,
        "the parse error must point at the bad command, not line 1"
    );
    assert!(
        r.findings
            .iter()
            .any(|f| &*f.rule == "grep/no-wildcard-import"
                && &*f.path == "src/hashset.lean"
                && f.line == 3),
        "the surviving `open` must still be reported: {:?}",
        r.findings
    );
}

/// A file holding only the unparseable command still reports just the parse
/// error: the rescue must not fabricate findings from nothing.
#[test]
fn boundary_lean_fully_unparseable_file_still_reports_only_parse_error() {
    let (_t, root) = tmp();
    put(
        &root,
        "src/satisfies.lean",
        "module\n\
         \n\
         deprecated_module\n\
         \x20 \"This module has been moved\"\n\
         \x20 (since := \"2026-10-05\")\n",
    );
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Strict),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["grep/no-wildcard-import", "meta/*"]),
        ..base(&root)
    });
    assert_eq!(
        r.findings.len(),
        1,
        "only the parse error may remain: {:?}",
        r.findings
    );
    let pe = &r.findings[0];
    assert_eq!(&*pe.rule, "meta/parse-error");
    assert_eq!(&*pe.path, "src/satisfies.lean");
}

/// `ERROR`-root whole-file wrap with no attributable span: the unsupported
/// loop as the final command leaves clean complete commands plus loose
/// fragments with neither child errors nor token gaps. Recovery must blank
/// the fragment run and keep the healthy prefix analyzed.
#[test]
fn boundary_lean_gapless_error_root_keeps_prefix() {
    let (_t, root) = tmp();
    put(
        &root,
        "src/noafter.lean",
        "import Lean\n\
         \n\
         open Lean Meta Std.HashSet\n\
         \n\
         variable [BEq α] [Hashable α]\n\
         \n\
         def good (x : Nat) : Nat := x\n\
         \n\
         def anyM [Monad m] (s : HashSet α) (f : α → m Bool) : m Bool := do\n\
         \x20 for a in s do\n\
         \x20   if ← f a then\n\
         \x20     return true\n\
         \x20 return false\n",
    );
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Strict),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["grep/no-wildcard-import", "meta/*"]),
        ..base(&root)
    });
    let pe = r
        .findings
        .iter()
        .find(|f| &*f.rule == "meta/parse-error" && &*f.path == "src/noafter.lean")
        .unwrap_or_else(|| {
            panic!(
                "the wrapped command must still be reported: {:?}",
                r.findings
            )
        });
    assert_eq!(
        pe.line, 9,
        "the parse error must point at the bad command, not line 1"
    );
    let wildcards: Vec<_> = r
        .findings
        .iter()
        .filter(|f| &*f.rule == "grep/no-wildcard-import" && &*f.path == "src/noafter.lean")
        .collect();
    assert_eq!(
        wildcards.len(),
        3,
        "the surviving `open` keeps all three namespace findings: {:?}",
        r.findings
    );
    assert!(
        wildcards.iter().all(|f| f.line == 3),
        "no finding may move onto the blanked command: {:?}",
        r.findings
    );
}

/// Indented commands inside a namespace are complete commands even though
/// they never start at column 0: a failed indented theorem must blank only
/// its own rows and leave the healthy indented neighbors analyzed.
#[test]
fn boundary_lean_indented_commands_recover_independently() {
    let (_t, root) = tmp();
    put(
        &root,
        "src/ns.lean",
        "namespace Demo\n\
         \n\
         \x20\x20 open Lean Meta\n\
         \n\
         \x20\x20 theorem mid : MyNat.le = (.≤.) := rfl\n\
         \n\
         \x20\x20 open Foo.Bar\n\
         \n\
         end Demo\n",
    );
    let r = run_ok(&super::RunOptions {
        strictness: Some(super::Strictness::Strict),
        mode: Some(super::Mode::Threshold),
        rule_filter: super::filt(&["grep/no-wildcard-import", "meta/*"]),
        ..base(&root)
    });
    let pe = r
        .findings
        .iter()
        .find(|f| &*f.rule == "meta/parse-error" && &*f.path == "src/ns.lean")
        .unwrap_or_else(|| {
            panic!(
                "the indented theorem must still be reported: {:?}",
                r.findings
            )
        });
    assert_eq!(
        pe.line, 5,
        "the parse error must point at the bad command, not its neighbors"
    );
    let wildcards: Vec<_> = r
        .findings
        .iter()
        .filter(|f| &*f.rule == "grep/no-wildcard-import" && &*f.path == "src/ns.lean")
        .collect();
    assert_eq!(
        wildcards.len(),
        3,
        "both indented `open` commands must still report: {:?}",
        r.findings
    );
    assert!(
        wildcards.iter().all(|f| f.line == 3 || f.line == 7),
        "no finding may move onto the blanked command: {:?}",
        r.findings
    );
}
