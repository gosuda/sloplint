use sloplint_core::facts::{AmbientKind, FileFacts, PrimitiveKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn with_facts(source: &str, path: &str, check: impl FnOnce(&FileFacts<'_>)) {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::CSharp,
        path,
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "parses");
    check(&analysis.facts);
}

#[test]
fn single_variant_switch_is_not_enum_like() {
    let source = r#"public class C {
    public int M1(Mode mode) {
        switch (mode) {
            case Mode.Fast: return 1;
            default: return 0;
        }
    }
    public int M2(Mode mode) {
        switch (mode) {
            case Mode.Fast: return 1;
            case Mode.Slow: return 2;
            default: return 0;
        }
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        assert_eq!(facts.wildcard_arms.len(), 2, "two default arms");
        assert!(
            facts
                .wildcard_arms
                .iter()
                .any(|a| !a.enum_like && a.variant_count == 1),
            "a single variant plus default is not enum-like"
        );
        assert!(
            facts
                .wildcard_arms
                .iter()
                .any(|a| a.enum_like && a.variant_count == 2),
            "two variants of one type are enum-like"
        );
    });
}

#[test]
fn stacked_labels_count_each_variant() {
    let source = r#"public class C {
    public int M(Mode mode) {
        switch (mode) {
            case Mode.Fast:
            case Mode.Slow:
                return 1;
            default:
                return 0;
        }
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        assert_eq!(facts.wildcard_arms.len(), 1, "one default arm");
        let arm = &facts.wildcard_arms[0];
        assert!(arm.enum_like, "stacked labels of one type are enum-like");
        assert_eq!(arm.variant_count, 2, "each label counts as a variant");
    });
}

#[test]
fn block_body_return_marks_a_value_return() {
    let source = r#"public class C {
    public int F(int x) { return x + 1; }
    public void G() { Log("x"); }
}
"#;
    with_facts(source, "case.cs", |facts| {
        let valued = facts
            .functions
            .iter()
            .find(|f| f.name == "F")
            .expect("block method is a function");
        assert!(
            valued.has_value_return,
            "a block return marks a value return"
        );
        let plain = facts
            .functions
            .iter()
            .find(|f| f.name == "G")
            .expect("void method is a function");
        assert!(
            !plain.has_value_return,
            "a void method without a return marks no value"
        );
    });
}

#[test]
fn bodyless_interface_member_emits_a_function() {
    let source = r#"public interface IStore { void Save(bool flush, bool sync); }
"#;
    with_facts(source, "case.cs", |facts| {
        let save = facts
            .functions
            .iter()
            .find(|f| f.name == "Save")
            .expect("interface member is a function");
        assert!(save.is_method, "an interface member is a method");
        assert_eq!(save.params.len(), 2, "two boolean parameters");
        assert!(
            save.params
                .iter()
                .all(|p| p.primitive_kind == PrimitiveKind::Bool),
            "boolean parameters keep their kind"
        );
        assert_eq!(
            save.visibility,
            Visibility::Exported,
            "an interface member is exported"
        );
        assert!(
            !save.signature_forced,
            "an interface member owns nothing to match"
        );
    });
}

#[test]
fn implicit_interface_impl_stays_actionable() {
    let source = r#"using System;
public interface IRepo { void Save(bool flush, bool sync); }
public class Repo : IRepo { public void Save(bool flush, bool sync) { } }
"#;
    with_facts(source, "case.cs", |facts| {
        let save = facts
            .functions
            .iter()
            .find(|f| f.name == "Save" && !f.is_trait_item)
            .expect("the implementation is a function");
        assert!(!save.signature_forced, "only override forces a signature");
    });
}

#[test]
fn explicit_interface_impl_stays_actionable() {
    let source = r#"public interface IRepo { void Save(bool flush, bool sync); }
public class Repo : IRepo { void IRepo.Save(bool flush, bool sync) { } }
"#;
    with_facts(source, "case.cs", |facts| {
        let save = facts
            .functions
            .iter()
            .find(|f| f.name == "Save" && !f.is_trait_item)
            .expect("the implementation is a function");
        assert!(!save.signature_forced, "only override forces a signature");
    });
}

#[test]
fn third_party_using_names_its_root() {
    let source = r#"using Newtonsoft.Json;
using My.App.Utils;
namespace My.App { public class C { } }
"#;
    with_facts(source, "case.cs", |facts| {
        let third = facts
            .imports
            .iter()
            .find(|i| i.name == "Json")
            .expect("third-party using is an import");
        assert_eq!(
            third.external_root,
            Some("Newtonsoft"),
            "an absolute namespace import names its root"
        );
        let own = facts
            .imports
            .iter()
            .find(|i| i.name == "Utils")
            .expect("own-namespace using is an import");
        assert_eq!(
            own.external_root, None,
            "an import from the file namespace has no external root"
        );
    });
}

#[test]
fn awaited_call_in_catch_keeps_its_callee() {
    let source = r#"public class C {
    public async Task M() {
        try { await Work(); }
        catch (Exception e) { await Log.Error(e); throw; }
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        assert_eq!(facts.handlers.len(), 1, "one catch handler");
        let stmts = &facts.handlers[0].stmts;
        assert_eq!(stmts.len(), 2, "a log call and a rethrow");
        assert!(
            matches!(&stmts[0], sloplint_core::facts::Stmt::Call { callee, .. } if *callee == "Log.Error"),
            "an awaited log call keeps its callee"
        );
        assert!(
            matches!(&stmts[1], sloplint_core::facts::Stmt::Rethrow),
            "a bare throw stays a rethrow"
        );
    });
}

#[test]
fn lambda_is_its_own_function() {
    let source = r#"using System.Collections.Generic;
public class C {
    public void M(List<X> xs) {
        xs.ForEach(y => { if (y.A) { if (y.B) Use(y); } });
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        assert!(
            facts.functions.iter().any(|f| f.name == "M"),
            "the enclosing method keeps its function"
        );
        assert!(
            facts.functions.iter().any(|f| f.name.is_empty()),
            "a lambda gets its own function"
        );
    });
}

#[test]
fn expression_lambda_keeps_its_sole_call() {
    let source = r#"using System.Linq;
using System.Collections.Generic;
public class C {
    public void M(List<X> xs) {
        var ys = xs.Select(x => Foo(x));
    }
    private X Foo(X x) { return x; }
}
"#;
    with_facts(source, "case.cs", |facts| {
        let lambda = facts
            .functions
            .iter()
            .find(|f| f.name.is_empty())
            .expect("a lambda gets its own function");
        assert!(
            lambda.sole_call.is_some(),
            "an expression-bodied forwarding lambda keeps its sole call"
        );
    });
}

#[test]
fn lambda_in_test_method_is_test() {
    let source = r#"public class C {
    [Test]
    public void M() {
        Run(x => { return Helper(x); });
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        let lambda = facts
            .functions
            .iter()
            .find(|f| f.name.is_empty())
            .expect("a lambda gets its own function");
        assert!(lambda.in_test, "a lambda in a test method is test");
    });
}

#[test]
fn unrelated_interface_does_not_force_the_signature() {
    let source = r#"public interface ISink { void Save(); }
public class Base { }
public class Repo : Base { public void Save(bool flush, bool sync) { } }
"#;
    with_facts(source, "case.cs", |facts| {
        let save = facts
            .functions
            .iter()
            .find(|f| f.name == "Save" && !f.is_trait_item)
            .expect("the implementation is a function");
        assert!(
            !save.signature_forced,
            "a base-class impl stays actionable without a matching interface member"
        );
    });
}

#[test]
fn discard_label_is_a_wildcard() {
    let source = r#"public class C {
    public string M(Mode mode) {
        switch (mode) {
            case Mode.Fast: return "fast";
            case _: return "other";
        }
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        assert_eq!(facts.wildcard_arms.len(), 1, "one discard arm");
        let arm = &facts.wildcard_arms[0];
        assert_eq!(
            arm.scrutinee,
            sloplint_core::facts::ScrutineeKind::Ident,
            "switch on a variable reports Ident"
        );
        assert_eq!(arm.variant_count, 1, "one sibling variant");
        assert!(!arm.enum_like, "a single variant is not enum-like");
    });
}

#[test]
fn or_pattern_labels_count_each_variant() {
    let source = r#"public class C {
    public int M(Mode mode) {
        switch (mode) {
            case Mode.Fast or Mode.Slow: return 1;
            default: return 0;
        }
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        assert_eq!(facts.wildcard_arms.len(), 1, "one default arm");
        let arm = &facts.wildcard_arms[0];
        assert!(arm.enum_like, "labels joined by or name one type");
        assert_eq!(arm.variant_count, 2, "each or branch counts as a variant");
    });
}

#[test]
fn nested_getter_scores_like_a_method() {
    let source = r#"public class C {
    public int Score {
        get {
            if (a) { if (b) { if (c) { if (d) { if (e) { if (f) { return 1; } } } } } }
            return 0;
        }
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        let score = facts
            .functions
            .iter()
            .find(|f| f.name == "Score")
            .expect("a getter is a function");
        assert!(score.max_depth >= 2, "nested branches in a getter count");
        assert!(score.cognitive > 0, "branching in a getter scores");
    });
}

#[test]
fn loop_and_pattern_locals_shadow_statics() {
    let source = r#"using System.Collections.Generic;
class C {
    static int count; static int total; static int got; static int matched;
    void M(List<int> xs, object o) {
        foreach (var count in xs) { count++; }
        for (int total = 0; total < 3; total++) { }
        Foo(out var got);
        if (o is int matched) { matched++; }
    }
    void Foo(out int v) { v = 1; }
}
"#;
    with_facts(source, "case.cs", |facts| {
        for w in ["count", "total", "got", "matched"] {
            assert!(
                facts
                    .ambient
                    .iter()
                    .all(|a| !(a.name == w && a.kind == AmbientKind::GlobalWrite)),
                "a loop or pattern local shadows the static"
            );
        }
    });
    let plain = r#"class C { static int count; void M() { count++; } }"#;
    with_facts(plain, "case.cs", |facts| {
        assert!(
            facts
                .ambient
                .iter()
                .any(|a| a.name == "count" && a.kind == AmbientKind::GlobalWrite),
            "an unshadowed static write still reports"
        );
    });
}
