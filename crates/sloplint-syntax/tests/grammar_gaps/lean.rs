//! Lean 4 constructs the vendored grammar must accept, observed through a
//! raw-tree walk because a hidden MISSING recovery node sets `has_error`
//! without producing a cursor-visible error position.
//!
//! The grammar is vendored unpatched at upstream commit `0d28e5d`, so each
//! test pins a construct class against that revision: a re-vendor that
//! drops a shape shows up here. There is no external Lean corpus in this
//! repository, so the fixtures below are the coverage.

use std::sync::LazyLock;

use sloplint_core::facts::ImportKind;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::Lean,
        path,
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn run_clean<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let analysis = run(path, source);
    assert!(
        analysis.parse_error.is_none(),
        "{path} must parse cleanly, first error at {:?}",
        analysis.parse_error
    );
    let tree = parse_raw(source);
    assert!(
        first_problem(&tree).is_none(),
        "{path} has a recovery node: {:?}",
        first_problem(&tree)
    );
    analysis
}

fn parse_raw(source: &str) -> tree_sitter::Tree {
    let language: tree_sitter::Language = tree_sitter_lean::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("lean language loads");
    parser.parse(source, None).expect("parser returns a tree")
}

/// True when the tree holds a visible ERROR node or a MISSING node anywhere.
/// When the walk finds no such node but the root still reports an error, the
/// recovery is hidden, so the root position is reported instead of silence.
fn first_problem(tree: &tree_sitter::Tree) -> Option<String> {
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            return Some(format!("{} at {}", node.kind(), node.start_position()));
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return tree
                    .root_node()
                    .has_error()
                    .then(|| format!("error at {}", tree.root_node().start_position()));
            }
        }
    }
}

fn function_names<'a>(analysis: &'a Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

#[test]
fn invalid_lean_still_reports_a_parse_error() {
    let analysis = run("src/bad.lean", "def f (x : Nat) :=");
    assert!(
        analysis.parse_error.is_some(),
        "a missing definition body must not parse cleanly"
    );
}

#[test]
fn definition_commands_keep_their_shapes() {
    let source = "def add (x : Nat) (y : Nat) : Nat := x + y\ntheorem add_zero (n : Nat) : n + 0 = n := by simp\nlemma add_zero_lemma (n : Nat) : n + 0 = n := by simp\nabbrev MyNat := Nat\naxiom myAx : Nat\nopaque myOp : Nat → Nat\ninstance : ToString Nat where\n  toString _ := \"s\"\ninductive Color where\n  | red : Color\n  | green : Color\nstructure Point where\n  x : Nat\n  y : Nat\nexample : 1 + 1 = 2 := by decide\nconstant myConst : Nat\nmutual\n  def even : Nat → Bool\n    | 0 => true\n    | _ + 1 => !even\nend\n";
    let analysis = run_clean("src/defns.lean", source);
    for want in [
        "add",
        "add_zero",
        "add_zero_lemma",
        "MyNat",
        "myAx",
        "myOp",
        "even",
    ] {
        assert!(
            function_names(&analysis).contains(&want),
            "{want} must surface as a function, got {:?}",
            function_names(&analysis)
        );
    }
    let tree = parse_raw(source);
    let sexp = tree.root_node().to_sexp();
    for shape in [
        "(def ",
        "(theorem ",
        "(abbrev ",
        "(axiom ",
        "(opaque ",
        "(instance ",
        "(inductive ",
        "(ctor_alt ",
        "(structure ",
        "(field ",
        "(example ",
        "(constant ",
        "(mutual)",
        "(match_alt ",
        "(where_struct ",
    ] {
        assert!(
            sexp.contains(shape),
            "tree must contain {shape}, got {sexp}"
        );
    }
}

#[test]
fn control_and_match_terms_keep_their_shapes() {
    let source = "def f (x : Nat) : Nat :=\n  match x with\n  | 0 => 0\n  | n + 1 => n\ndef g (x : Nat) : Nat :=\n  if x = 0 then 1 else 2\ndef h : IO Unit := do\n  let mut n := 0\n  n := n + 1\ndef lam : Nat → Nat := fun x => x + 1\ndef useForall : Prop := ∀ x : Nat, x ≥ 0\ndef tac (h : Nat = 0) : Nat = 0 := by\n  induction h with\n  | refl => exact rfl\n";
    let analysis = run_clean("src/control.lean", source);
    for want in ["f", "g", "h", "lam", "useForall", "tac"] {
        assert!(
            function_names(&analysis).contains(&want),
            "{want} must surface as a function, got {:?}",
            function_names(&analysis)
        );
    }
    let tree = parse_raw(source);
    let sexp = tree.root_node().to_sexp();
    for shape in [
        "(match ",
        "(match_alt ",
        "(if_then_else ",
        "(do_block ",
        "(block_assign ",
        "(fun ",
        "(forall ",
        "(by ",
        "(induction ",
    ] {
        assert!(
            sexp.contains(shape),
            "tree must contain {shape}, got {sexp}"
        );
    }
    assert!(
        analysis.facts.calls.iter().any(|c| c.callee == "exact"),
        "the tactic head application must surface as a call, got {:?}",
        analysis
            .facts
            .calls
            .iter()
            .map(|c| c.callee)
            .collect::<Vec<_>>()
    );
}

#[test]
fn modules_imports_and_comments_parse_clean() {
    let source = "/-! module doc !-/\nimport Std.Data.HashMap\nopen Nat hiding Term in\nopen scoped BigOperators\nopen Nat renaming succ \u{2192} mySucc\nexport Nat (succ add)\nopen only Nat\n/- block -/\n-- line\n/-- doc -/\ndef s : String := \"hello\"\ndef t : String := s!\"value {1 + 2} end\"\n";
    let analysis = run_clean("src/module.lean", source);
    assert_eq!(
        analysis.facts.imports.len(),
        8,
        "one import, four opens, two named re-exports (upstream parity: `hiding`/`Term` splits into extra open nodes, and `open only Nat` parses `only` as a real first namespace plus `Nat`, so the modifier shapes contribute eight entries), got {:?}",
        analysis
            .facts
            .imports
            .iter()
            .map(|i| i.module.clone())
            .collect::<Vec<_>>()
    );
    assert!(
        analysis
            .facts
            .imports
            .iter()
            .any(|i| i.module == "Std.Data.HashMap"),
        "the dotted import path must stay whole"
    );
    assert!(
        analysis
            .facts
            .imports
            .iter()
            .any(|i| i.module == "BigOperators"),
        "an open-scoped namespace must record an import"
    );
    assert!(
        analysis
            .facts
            .imports
            .iter()
            .any(|i| i.module == "BigOperators" && i.kind == ImportKind::SideEffect),
        "an open-scoped namespace is a side-effect open, not a wildcard"
    );
    assert!(
        !analysis
            .facts
            .imports
            .iter()
            .any(|i| i.module == "hiding" || i.module == "Term" || i.module == "renaming"),
        "open modifiers and hidden names are not namespaces"
    );
    assert!(
        analysis.facts.imports.iter().any(|i| i.module == "only"),
        "an open modifier word in first position is a real namespace"
    );
    assert!(
        analysis
            .facts
            .imports
            .iter()
            .filter(|i| i.is_reexport)
            .map(|i| i.name)
            .collect::<Vec<_>>()
            == ["succ", "add"],
        "an export list names its re-exported symbols"
    );
    assert!(
        analysis
            .facts
            .imports
            .iter()
            .all(|i| i.external_root.is_none()),
        "open namespaces and stdlib imports carry no external root"
    );
    let tree = parse_raw(source);
    let sexp = tree.root_node().to_sexp();
    for shape in [
        "(import ",
        "(open ",
        "(block_comment)",
        "(line_comment)",
        "(module_doc_comment)",
        "(doc_comment)",
        "(str_lit)",
        "(interpolated_str ",
        "(interpolation ",
    ] {
        assert!(
            sexp.contains(shape),
            "tree must contain {shape}, got {sexp}"
        );
    }
}

#[test]
fn hash_commands_parse_clean() {
    let source = "#lint\n#find _ + _ = _ + _\n#whnf Nat.add 1 2\n#guard_msgs in\n#eval \"hi\"\n#guard 1 + 1 = 2\n#print Nat\n#check fun x => x\n";
    let analysis = run_clean("src/hashcmd.lean", source);
    let tree = parse_raw(source);
    let sexp = tree.root_node().to_sexp();
    for shape in ["(hash_cmd ", "(hash_name)", "(eval ", "(app "] {
        assert!(
            sexp.contains(shape),
            "tree must contain {shape}, got {sexp}"
        );
    }
    assert!(
        analysis.parse_error.is_none(),
        "#-commands must not error, got {:?}",
        analysis.parse_error
    );
}

#[test]
fn custom_command_heads_parse_clean() {
    let source = "World \"Lecture10\"\net.semantics.termination \"total\"\n";
    run_clean("src/customcmd.lean", source);
    let sexp = parse_raw(source).root_node().to_sexp();
    assert!(
        sexp.contains("(custom_command "),
        "custom command heads must parse, got {sexp}"
    );
}

#[test]
fn do_try_catch_finally_parse_clean() {
    // NOTE: a multiline `finally <block>` followed by another `def` is an
    // upstream-parity gap (the pinned grammar eats the next `def`), so the
    // block-finally def goes last and gets closed by EOF.
    let source = "def t2 : IO Unit := do\n  try\n    risky\n  catch\n    | e1 => pure ()\n    | e2 => pure ()\ndef t3 : IO Unit := do\n  try risky catch e => fallback finally done\ndef t1 : IO Unit := do\n  try\n    risky\n  catch e =>\n    pure ()\n  finally\n    cleanup\n";
    run_clean("src/dotry.lean", source);
    let sexp = parse_raw(source).root_node().to_sexp();
    for shape in ["(do_try ", "pattern:", "(do_block "] {
        assert!(
            sexp.contains(shape),
            "tree must contain {shape}, got {sexp}"
        );
    }
}

#[test]
fn do_block_assign_and_tactic_apps_parse() {
    // NOTE: `calc` chains and `first | …` alternation inside `by` blocks are
    // upstream-parity gaps (identical or worse ERRORs on the pinned grammar),
    // so they are omitted here.
    let source = "def h : IO Unit := do\n  let mut n := 0\n  n := n + 1\n  IO.println n\nexample : 1 = 1 := by\n  intro h\n  apply f\n  rw [h]\n  simp [a, b]\n  repeat exact h\n";
    run_clean("src/doblock.lean", source);
    let sexp = parse_raw(source).root_node().to_sexp();
    for shape in ["(block_assign ", "(app "] {
        assert!(
            sexp.contains(shape),
            "tree must contain {shape}, got {sexp}"
        );
    }
}

#[test]
fn variables_omit_elab_and_relative_imports_parse() {
    let source = "import Std.Basic\nimport .mm0\nimport ..rel\nvariable (x : Nat)\nvariables {α : Type} [Inhabited α]\nomit [Sort] in\ndef f : Nat := 0\nelab \"foo\" x:term : term => x\n";
    run_clean("src/misc.lean", source);
    let sexp = parse_raw(source).root_node().to_sexp();
    for shape in ["(import ", "(variable ", "(omit ", "(elab_cmd "] {
        assert!(
            sexp.contains(shape),
            "tree must contain {shape}, got {sexp}"
        );
    }
}

#[test]
fn nested_do_block_sibling_after_dedent() {
    // A `let x ← do` block dedented back to the parent's statement indent:
    // the newline that closed the inner block must still separate the outer
    // statements (regression: the dedent consumed it upstream).
    let source = "def h : IO Unit := do\n  let x ← do\n    pure 1\n  IO.println x\n";
    run_clean("src/nested.lean", source);
    let sexp = parse_raw(source).root_node().to_sexp();
    assert!(
        sexp.matches("stmt:").count() >= 2,
        "the sibling after a dedented inner block must be a stmt, got {sexp}"
    );
}

#[test]
fn first_problem_agrees_with_has_error() {
    for source in ["def f (x : Nat) : Nat := x\n", "def broken (x : Nat) :=\n"] {
        let tree = parse_raw(source);
        assert_eq!(
            first_problem(&tree).is_some(),
            tree.root_node().has_error(),
            "first_problem must agree with has_error for {source:?}"
        );
    }
}

#[test]
fn tactic_vocabulary_keyword_tails_parse() {
    // rw/simp/refine with keyword tails (`at`, `+flag`) parse as term
    // applications — `at`/`only` are plain identifiers here and `+x` is a
    // unary-op argument; no dedicated tactic rule needed.
    for source in [
        "theorem t : True := by\n  rw [foo] at h\n  exact h\n",
        "theorem t : True := by\n  simp +arith [a] at h ⊢\n",
        "theorem t : True := by\n  refine ⟨h, ?_⟩ <;> simp [a]\n",
        "theorem t : True := by\n  intro s hs\n  constructor\n",
        "theorem t : True := by\n  induction n with\n  | zero => rfl\n  | succ n ih => simp [ih]\n",
    ] {
        run_clean("src/tactic.lean", source);
    }
}

#[test]
fn quantifier_comparison_binders_parse() {
    // `∀ᶠ` is upstream-parity broken (identical ERROR on the pinned grammar).
    run_clean("src/quant.lean", "theorem t : ∀ ε > 0, ∃ N > 0, N = N := by sorry\n");
}

#[test]
fn for_loop_and_have_forms_parse() {
    for source in [
        "def f : IO Unit := do\n  for x in xs do\n    IO.println x\n",
        "def f : IO Unit := do\n  for h : x in xs do\n    pure h\n",
        "def f : IO Unit := do\n  for x in xs, y in ys do\n    IO.println (x + y)\n",
        "theorem t : True := by\n  have h : True\n  · exact True.intro\n",
        "theorem t : True := by\n  have : True := True.intro\n  exact this\n",
    ] {
        run_clean("src/loops.lean", source);
    }
}

#[test]
fn attribute_erasure_and_option_dotnames_parse() {
    for source in [
        "attribute [-simp] Fintype.sum_sum_type\n",
        "set_option linter.verso.manual.headerTags false in\ntheorem t : True := True.intro\n",
        "open scoped Classical in\ndef f : Nat := 0\n",
    ] {
        run_clean("src/attrs.lean", source);
    }
}
