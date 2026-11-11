use sloplint_core::facts::{ArgShape, FileFacts, PrimitiveKind, ScrutineeKind, Visibility};
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

#[test]
fn receiver_call_text_stays_out_of_callee() {
    let source = r#"using System;
public class Svc {
    public void Run() {
        services.GetRequiredService<ILogger>().LogError("boom");
        GetLogger().LogError("boom");
        Log.Error("x");
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        let chained: Vec<_> = facts
            .calls
            .iter()
            .filter(|c| c.callee == "LogError")
            .collect();
        assert_eq!(
            chained.len(),
            2,
            "both receiver-call chains trim to the member name"
        );
        assert!(
            chained.iter().all(|c| !c.is_simple_path),
            "a chain bottoming at a call is never a simple path"
        );
        assert!(
            facts
                .calls
                .iter()
                .any(|c| c.callee == "Log.Error" && c.is_simple_path),
            "a pure member chain keeps its dotted path"
        );
    });
}

#[test]
fn curried_call_has_no_argument_callee() {
    let source = r#"public class C {
    public void M() {
        Factory(logger)(msg);
        Log.Error("x");
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        assert!(
            facts.calls.iter().all(|c| c.callee != "logger"),
            "an argument never becomes a callee"
        );
        assert!(
            facts.calls.iter().any(|c| c.callee == "Factory"),
            "the inner call keeps its name"
        );
        assert!(
            facts
                .calls
                .iter()
                .any(|c| c.callee == "Log.Error" && c.is_simple_path),
            "a pure member chain keeps its dotted path"
        );
    });
}

#[test]
fn null_conditional_chain_roots_at_the_receiver() {
    let source = r#"public class C {
    public void M() {
        logger?.LogError("boom");
        Log.Error("x");
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        let null_safe = facts
            .calls
            .iter()
            .find(|c| c.callee == "LogError")
            .expect("null-conditional call trims to the member name");
        assert_eq!(
            null_safe.callee_root,
            Some("logger"),
            "a conditional chain bottoms at the identifier"
        );
        assert!(
            null_safe.is_simple_path,
            "conditional access joins identifiers like member access"
        );
        assert!(
            facts
                .calls
                .iter()
                .any(|c| c.callee == "Log.Error" && c.callee_root == Some("Log")),
            "a pure chain keeps its dotted path and root"
        );
    });
}

#[test]
fn constructor_args_are_not_a_collection() {
    let source = r#"public class C {
    public void M(string a) {
        Register(new Settings(a));
        Register(new List<int> { 1, 2 });
        Register(new Settings { Name = a });
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        let regs: Vec<_> = facts
            .calls
            .iter()
            .filter(|c| c.callee == "Register")
            .collect();
        assert_eq!(regs.len(), 3, "three Register calls");
        let plain = regs
            .iter()
            .filter(|c| {
                c.args
                    .first()
                    .is_some_and(|a| matches!(a.shape, ArgShape::Other))
            })
            .count();
        let collection = regs
            .iter()
            .filter(|c| {
                c.args
                    .first()
                    .is_some_and(|a| matches!(a.shape, ArgShape::Collection))
            })
            .count();
        assert_eq!(
            plain, 2,
            "constructor args and member assignments stay other"
        );
        assert_eq!(
            collection, 1,
            "an initializer-only creation is a collection"
        );
    });
}

#[test]
fn collection_with_closure_is_code_body() {
    let source = r#"using System;
using System.Collections.Generic;
public class C {
    public void M() {
        Run(new List<Func<int>> { () => 1 });
        Run(new List<int> { 1, 2 });
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        let runs: Vec<_> = facts.calls.iter().filter(|c| c.callee == "Run").collect();
        assert_eq!(runs.len(), 2, "two Run calls");
        assert!(
            runs.iter().any(|c| c
                .args
                .first()
                .is_some_and(|a| matches!(a.shape, ArgShape::CodeBody))),
            "a collection holding a closure is code"
        );
        assert!(
            runs.iter().any(|c| c
                .args
                .first()
                .is_some_and(|a| matches!(a.shape, ArgShape::Collection))),
            "a plain literal list stays a collection"
        );
    });
}

#[test]
fn long_plus_chain_with_static_stays_concat() {
    let source = r#"public class C {
    public void M(string name, string id) {
        logger.LogError("Failed for " + name + ": " + id);
        logger.LogError("a" + id);
        logger.LogError(name + id);
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        let statics: Vec<Option<u32>> = facts
            .calls
            .iter()
            .filter(|c| c.callee == "logger.LogError")
            .map(|c| match c.args.first().map(|a| &a.shape) {
                Some(ArgShape::Concat { longest_static }) => Some(*longest_static),
                _ => None,
            })
            .collect();
        assert_eq!(statics.len(), 3, "three logger calls");
        assert!(
            statics.contains(&Some(11)),
            "a literal-led chain reports its run"
        );
        assert!(statics.contains(&Some(1)), "a short chain reports its run");
        assert!(
            statics.contains(&None),
            "a fully dynamic addition stays other"
        );
    });
}

#[test]
fn escapes_count_as_static_text() {
    let source = r#"public class C {
    public void M() {
        logger.LogWarning("Bad \"id\" value");
        logger.LogWarning("ok");
        logger.LogWarning("{{ok}}");
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        let statics: Vec<u32> = facts
            .calls
            .iter()
            .filter(|c| c.callee == "logger.LogWarning")
            .filter_map(|c| match c.args.first().map(|a| &a.shape) {
                Some(ArgShape::Str { longest_static }) => Some(*longest_static),
                _ => None,
            })
            .collect();
        assert_eq!(statics.len(), 3, "all warnings carry string shapes");
        assert!(
            statics.contains(&14),
            "escapes count as static literal text"
        );
        assert!(
            statics.contains(&2),
            "a short literal keeps its short static run"
        );
        assert!(
            statics.contains(&4),
            "an escaped brace pair counts as one char"
        );
    });
}

#[test]
fn interpolated_message_with_static_is_concat() {
    let source = r#"public class C {
    public void M(string id) {
        logger.LogError($"Failed for {id}");
        logger.LogError($"E{id}");
        logger.LogError($"{id}");
        logger.LogError("Failed for all");
    }
}
"#;
    with_facts(source, "case.cs", |facts| {
        let errors: Vec<_> = facts
            .calls
            .iter()
            .filter(|c| c.callee == "logger.LogError")
            .collect();
        assert_eq!(errors.len(), 4, "four logger calls");
        let statics: Vec<Option<u32>> = errors
            .iter()
            .map(|c| match c.args.first().map(|a| &a.shape) {
                Some(ArgShape::Concat { longest_static }) => Some(*longest_static),
                _ => None,
            })
            .collect();
        assert!(
            statics.contains(&Some(11)),
            "interpolation keeps its static run"
        );
        assert!(
            statics.contains(&Some(1)),
            "a short template reports its run"
        );
        assert!(
            statics.contains(&Some(0)),
            "an all-dynamic template reports zero"
        );
        assert!(
            errors.iter().any(|c| c
                .args
                .first()
                .is_some_and(|a| matches!(a.shape, ArgShape::Str { .. }))),
            "a plain literal stays a string"
        );
    });
}
