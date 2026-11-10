use sloplint_core::facts::{FileFacts, PrimitiveKind, ScrutineeKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"using System;
public class Service {
    public int Run(string name, int count = 3, bool flag = true, params int[] rest) {
        return count;
    }
    private int Hidden() {
        return 0;
    }
    internal int Internal() {
        return 0;
    }
    public override string ToString() {
        return "s";
    }
    static void Main() {
        var v = Run("a");
        Run("b");
        var g = Make()();
    }
    static Func<int> Make() {
        return () => 1;
    }
    public int Classify(int x) {
        switch (x) {
            case 1:
                return 1;
            default:
                return 0;
        }
    }
}
"#;

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
fn public_methods_are_exported_private_are_not() {
    with_facts(SOURCE, "case.cs", |facts| {
        let run = facts
            .functions
            .iter()
            .find(|f| f.name == "Run")
            .expect("has Run");
        assert_eq!(
            run.visibility,
            Visibility::Exported,
            "public method is exported"
        );
        let hidden = facts
            .functions
            .iter()
            .find(|f| f.name == "Hidden")
            .expect("has Hidden");
        assert_eq!(
            hidden.visibility,
            Visibility::Private,
            "private method is private"
        );
        let internal = facts
            .functions
            .iter()
            .find(|f| f.name == "Internal")
            .expect("has Internal");
        assert_eq!(
            internal.visibility,
            Visibility::Restricted,
            "internal method is restricted"
        );
    });
}

#[test]
fn override_forces_signature_plain_method_does_not() {
    with_facts(SOURCE, "case.cs", |facts| {
        let forced = facts
            .functions
            .iter()
            .find(|f| f.name == "ToString")
            .expect("has ToString");
        assert!(forced.signature_forced, "override states the signature");
        let run = facts
            .functions
            .iter()
            .find(|f| f.name == "Run")
            .expect("has Run");
        assert!(
            !run.signature_forced,
            "plain method leaves the signature free"
        );
    });
}

#[test]
fn main_is_entry_point_other_methods_are_not() {
    with_facts(SOURCE, "case.cs", |facts| {
        let main = facts
            .functions
            .iter()
            .find(|f| f.name == "Main")
            .expect("has Main");
        assert!(main.entry_point, "Main is the entry point");
        let run = facts
            .functions
            .iter()
            .find(|f| f.name == "Run")
            .expect("has Run");
        assert!(!run.entry_point, "ordinary method is not an entry point");
    });
}

#[test]
fn params_record_defaults_variadics_and_primitive_kinds() {
    with_facts(SOURCE, "case.cs", |facts| {
        let run = facts
            .functions
            .iter()
            .find(|f| f.name == "Run")
            .expect("has Run");
        assert_eq!(run.params.len(), 4, "four params recorded");
        let name = &run.params[0];
        assert!(!name.has_default, "plain param has no default");
        assert_eq!(
            name.primitive_kind,
            PrimitiveKind::Str,
            "string normalizes to Str"
        );
        let count = &run.params[1];
        assert!(count.has_default, "count has a default");
        assert!(!count.bool_default, "numeric default is not a bool default");
        assert_eq!(
            count.primitive_kind,
            PrimitiveKind::Int,
            "int normalizes to Int"
        );
        let flag = &run.params[2];
        assert!(flag.has_default, "flag has a default");
        assert!(flag.bool_default, "true default is a bool default");
        assert_eq!(
            flag.primitive_kind,
            PrimitiveKind::Bool,
            "bool normalizes to Bool"
        );
        let rest = &run.params[3];
        assert!(rest.is_variadic, "params array is variadic");
        assert!(!rest.has_default, "variadic has no default");
    });
}

#[test]
fn calls_mark_simple_paths_and_binding_initializers() {
    with_facts(SOURCE, "case.cs", |facts| {
        let runs: Vec<_> = facts.calls.iter().filter(|c| c.callee == "Run").collect();
        assert_eq!(runs.len(), 2, "two Run calls");
        assert!(
            runs.iter().all(|c| c.is_simple_path),
            "bare identifier callee is a simple path"
        );
        assert!(
            runs.iter().any(|c| c.initializes_binding),
            "var initializer call marks the binding"
        );
        assert!(
            runs.iter().any(|c| !c.initializes_binding),
            "bare statement call marks no binding"
        );
        let nested = facts
            .calls
            .iter()
            .find(|c| !c.is_simple_path)
            .expect("has a nested-callee call");
        assert!(
            nested.initializes_binding,
            "outer call still initializes its binding"
        );
    });
}

#[test]
fn switch_default_reports_ident_scrutinee() {
    with_facts(SOURCE, "case.cs", |facts| {
        assert_eq!(facts.wildcard_arms.len(), 1, "one default arm");
        let arm = &facts.wildcard_arms[0];
        assert!(!arm.enum_like, "numeric case labels are not enum-like");
        assert_eq!(
            arm.scrutinee,
            ScrutineeKind::Ident,
            "switch on a variable reports Ident"
        );
        assert_eq!(arm.variant_count, 0, "variant count defaults to zero");
        assert!(!arm.has_nullable_ctor, "no nullable constructors in scope");
    });
}
