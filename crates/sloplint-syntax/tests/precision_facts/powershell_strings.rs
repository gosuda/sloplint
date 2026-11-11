//! PowerShell string and argument shapes: static runs, collections, code bodies, bindings, assertions, ambient, copies, and arm facts.

use sloplint_core::facts::{ArgShape, AssertShape, FileFacts, ScrutineeKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

struct FnFacts {
    name: String,
    visibility: Visibility,
    params: usize,
}

fn observe<R>(source: &str, read: impl FnOnce(&FileFacts<'_>) -> R) -> R {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::PowerShell,
        path: "script.ps1",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "powershell parses");
    read(&analysis.facts)
}

fn functions_of(source: &str) -> Vec<FnFacts> {
    observe(source, |facts| {
        facts
            .functions
            .iter()
            .map(|f| FnFacts {
                name: f.name.to_owned(),
                visibility: f.visibility,
                params: f.params.len(),
            })
            .collect()
    })
}

type CallRow = (String, Option<String>, bool, bool, Vec<(String, u32, u32)>);
fn describe_shape(shape: &ArgShape<'_>) -> String {
    match shape {
        ArgShape::Str { longest_static } => format!("str:{longest_static}"),
        ArgShape::Concat { longest_static } => format!("concat:{longest_static}"),
        ArgShape::Ident(name) => format!("ident:{name}"),
        ArgShape::Collection => "collection".to_string(),
        ArgShape::CodeBody => "code".to_string(),
        ArgShape::Other => "other".to_string(),
    }
}

fn calls_full_of(source: &str) -> Vec<CallRow> {
    observe(source, |facts| {
        facts
            .calls
            .iter()
            .map(|c| {
                (
                    c.callee.to_owned(),
                    c.callee_root.map(str::to_owned),
                    c.is_simple_path,
                    c.initializes_binding,
                    c.args
                        .iter()
                        .map(|a| (describe_shape(&a.shape), a.start_line, a.end_line))
                        .collect(),
                )
            })
            .collect()
    })
}

fn assertions_of(source: &str) -> Vec<(bool, bool, AssertShape)> {
    observe(source, |facts| {
        facts
            .assertions
            .iter()
            .map(|a| (a.has_message, a.comparison, a.shape))
            .collect()
    })
}

fn ambient_of(source: &str) -> Vec<(String, bool)> {
    observe(source, |facts| {
        facts
            .ambient
            .iter()
            .map(|a| (format!("{:?}", a.kind), a.in_function.is_some()))
            .collect()
    })
}

fn copies_of(source: &str) -> Vec<(u32, Option<String>)> {
    observe(source, |facts| {
        facts
            .field_copies
            .iter()
            .map(|c| (c.fields, c.source.map(str::to_owned)))
            .collect()
    })
}

fn arms_of(source: &str) -> Vec<(ScrutineeKind, u32, bool, bool)> {
    observe(source, |facts| {
        facts
            .wildcard_arms
            .iter()
            .map(|a| {
                (
                    a.scrutinee,
                    a.variant_count,
                    a.has_nullable_ctor,
                    a.enum_like,
                )
            })
            .collect()
    })
}

fn handlers_of(source: &str) -> Vec<(bool, bool, bool)> {
    observe(source, |facts| {
        facts
            .handlers
            .iter()
            .map(|h| (h.has_else, h.try_imports_only, h.in_cleanup))
            .collect()
    })
}

#[test]
fn ordering_comparisons_map_to_other_shape() {
    let rows = assertions_of("$x | Should -BeGreaterThan 3\n");
    assert_eq!(rows.len(), 1, "pipeline assertion recorded");
    assert!(rows[0].1, "ordering operator still counts as a comparison");
    assert_eq!(
        rows[0].2,
        AssertShape::Other,
        "ordering operator is not top-level equality"
    );
    let lower = assertions_of("$x | should -Be 5\n");
    assert_eq!(lower.len(), 1, "lowercase Should still recorded");
    assert!(lower[0].1, "equality operator still counts as a comparison");
    assert_eq!(
        lower[0].2,
        AssertShape::Other,
        "named equality is not top-level syntax"
    );
    let messaged = assertions_of("$x | Should -Be 1 -Because \"cache must be warm\"\n");
    assert_eq!(messaged.len(), 1, "messaged assertion recorded");
    assert!(messaged[0].0, "Because flag marks the failure message");
}

#[test]
fn format_template_measures_static_run() {
    let rows = calls_full_of("$log.Error(\"failed to open file {0}\" -f $path)\n");
    assert_eq!(rows.len(), 1, "one error call recorded");
    assert!(
        rows[0].4.len() == 1 && rows[0].4[0].0 == "concat:20",
        "template run excludes placeholder"
    );
    let plain = calls_full_of("$log.Error(\"ok\")\n");
    assert!(
        plain[0].4.len() == 1 && plain[0].4[0].0 == "str:2",
        "plain literal stays a string"
    );
    let empty = calls_full_of("$log.Error(\"{0} {1} {2}\" -f $a, $b, $c)\n");
    assert!(
        empty[0].4.len() == 3 && empty[0].4[0].0 == "concat:1",
        "all-placeholder template stays below threshold",
    );
}

#[test]
fn array_and_hash_literals_are_collections() {
    let rows = calls_full_of("$batch.Add(@(\n    'a',\n    'b'\n))\n");
    assert_eq!(rows.len(), 1, "one invocation recorded");
    assert!(
        rows[0].4.len() == 1 && rows[0].4[0].0 == "collection",
        "multiline array is a collection"
    );
    assert!(
        rows[0].4[0].2 > rows[0].4[0].1,
        "collection spans more than one line"
    );
    let hashed = calls_full_of("$cfg.Put(@{\n    a = 1\n})\n");
    assert_eq!(hashed.len(), 1, "one hash call recorded");
    assert!(
        hashed[0].4.len() == 1 && hashed[0].4[0].0 == "other",
        "keyed hash is not a collection"
    );
    let single = calls_full_of("Invoke-One -Item 'solo'\n");
    assert_eq!(single.len(), 1, "one plain call recorded");
    let has_str = single[0]
        .4
        .iter()
        .any(|(shape, _, _)| shape.starts_with("str:"));
    assert!(has_str, "plain string is not a collection");
}

#[test]
fn block_inside_collection_is_code_body() {
    let rows = calls_full_of("$batch.Add(@( { Get-X } ))\n");
    assert!(
        rows[0].4.len() == 1 && rows[0].4[0].0 == "code",
        "block makes code body"
    );
    let plain = calls_full_of("$batch.Add(@(\n    'a',\n    'b'\n))\n");
    assert!(
        plain[0].4.len() == 1 && plain[0].4[0].0 == "collection",
        "array stays collection"
    );
}

#[test]
fn whole_initializer_marks_binding() {
    let rows = calls_full_of("$items = Build-List\n");
    assert_eq!(rows.len(), 1, "initializer call recorded");
    assert!(rows[0].3, "whole-value call marks its binding");
    let plain = calls_full_of("Build-List\n");
    assert!(!plain.is_empty(), "bare call recorded");
    assert!(!plain[0].3, "bare call marks no binding");
    let member = calls_full_of("$x.Prop = Build-List\n");
    assert_eq!(member.len(), 1, "member target call recorded");
    assert!(!member[0].3, "member target is not a plain binding");
    let invoke = calls_full_of("$x = $svc.Make()\n");
    assert_eq!(invoke.len(), 1, "invocation initializer recorded");
    assert!(invoke[0].3, "invocation whole-value marks its binding");
}

#[test]
fn index_receiver_has_no_identifier_root() {
    let rows = calls_full_of("$items[0].Save()\n");
    assert_eq!(rows.len(), 1, "one indexed call recorded");
    assert!(!rows[0].2, "index link is not a simple path");
    assert_eq!(rows[0].1, None, "index chain has no identifier root");
    let direct = calls_full_of("$repo.Save($x)\n");
    assert_eq!(direct.len(), 1, "direct member call recorded");
    assert!(direct[0].2, "variable receiver chain is a simple path");
    assert_eq!(
        direct[0].1.as_deref(),
        Some("repo"),
        "variable receiver keeps its root"
    );
}

#[test]
fn here_strings_skip_opening_token_in_gaps() {
    let rows = calls_full_of("Write-Host @\"\nDone: $x\n\"@\n");
    assert_eq!(rows.len(), 1, "one log call recorded");
    assert!(
        rows[0].4.len() == 1 && rows[0].4[0].0 == "concat:6",
        "static run excludes here-string opener"
    );
    let dynamic = calls_full_of("Write-Host @\"\n$x\n\"@\n");
    assert_eq!(dynamic.len(), 1, "one dynamic call recorded");
    assert!(
        dynamic[0].4.len() == 1 && dynamic[0].4[0].0 == "concat:0",
        "fully dynamic here-string reports zero"
    );
    let bare = calls_full_of("Write-Host @\"\nDone!!\n\"@\n");
    assert!(
        bare[0].4.len() == 1 && bare[0].4[0].0 == "concat:6",
        "variable-free here-string keeps body run"
    );
    let verbatim = calls_full_of("Write-Host @'\nDone!!\n'@\n");
    assert!(
        verbatim[0].4.len() == 1 && verbatim[0].4[0].0 == "str:6",
        "verbatim here-string excludes delimiters"
    );
}

#[test]
fn scoped_reads_in_functions_stay_silent() {
    let reader =
        ambient_of("$script:cache = @{}\nfunction Get-Cached($k) { return $script:cache[$k] }\n");
    assert_eq!(reader.len(), 1, "top-level mention recorded once");
    assert_eq!(
        reader[0].0, "GlobalMutable",
        "top-level mention stays mutable"
    );
    assert!(!reader[0].1, "top-level mention is outside functions");
    let writer = ambient_of("function Set-Cached($v) { $script:cache = $v }\n");
    assert_eq!(writer.len(), 1, "in-function write recorded");
    assert_eq!(writer[0].0, "GlobalWrite", "in-function write is a write");
    assert!(writer[0].1, "in-function write is inside a function");
    let reader_only = ambient_of("function Get-Cached($k) { return $script:cache[$k] }\n");
    assert!(reader_only.is_empty(), "in-function read emits no ambient");
}

#[test]
fn hash_literal_values_report_field_copies() {
    let rows = copies_of(
        "$o = @{ A = $src.A; B = $src.B; C = $src.C; D = $src.D; E = $src.E; F = $src.F }\n",
    );
    assert_eq!(rows.len(), 1, "one field copy recorded");
    assert_eq!(rows[0].0, 6, "all six entries counted");
    assert_eq!(rows[0].1.as_deref(), Some("src"), "source variable kept");
    let few = copies_of("$o = @{ A = $src.A; B = $src.B; C = $src.C }\n");
    assert_eq!(few.len(), 1, "small copy still a fact");
    assert_eq!(few[0].0, 3, "three entries counted");
}

#[test]
fn nested_function_is_private() {
    let rows = functions_of(
        "function Outer {\n    function Inner($Force = $false, $Quiet = $true) {}\n}\n",
    );
    assert_eq!(rows.len(), 2, "both functions recorded");
    let inner = rows
        .iter()
        .find(|f| f.name == "Inner")
        .expect("inner function recorded");
    assert_eq!(
        inner.visibility,
        Visibility::Private,
        "nested function is private"
    );
    assert_eq!(inner.params, 2, "nested params still collected");
    let outer = rows
        .iter()
        .find(|f| f.name == "Outer")
        .expect("outer function recorded");
    assert_eq!(
        outer.visibility,
        Visibility::Exported,
        "outer function stays exported"
    );
}

#[test]
fn default_arm_and_catch_carry_no_extra_facts() {
    let source = "switch ($x) {\n    1 { \"one\" }\n    default { \"other\" }\n}\ntry { Do-Thing } catch { Write-Host \"bad\" }\n";
    let arms = arms_of(source);
    assert_eq!(arms.len(), 1, "default arm recorded once");
    assert_eq!(
        arms[0].0,
        ScrutineeKind::Other,
        "switch scrutinee stays Other"
    );
    assert_eq!(arms[0].1, 0, "powershell counts no variants");
    assert!(!arms[0].2, "powershell has no nullable constructors");
    let handlers = handlers_of(source);
    assert_eq!(handlers.len(), 1, "catch handler recorded");
    assert!(!handlers[0].0, "catch has no else branch");
    assert!(!handlers[0].1, "catch is not imports-only");
    assert!(!handlers[0].2, "catch is not a cleanup frame");
    let bare = arms_of("switch ($x) {\n    1 { \"one\" }\n}\n");
    assert!(
        bare.is_empty(),
        "switch without default has no wildcard arm"
    );
    let no_try = handlers_of("$x = 1\n");
    assert!(no_try.is_empty(), "file without try has no handlers");
}
