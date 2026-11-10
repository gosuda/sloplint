use std::collections::{HashMap, HashSet};

use crate::config::Strictness;
use crate::facts::{ArgShape, Call, FileFacts, Function};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::EmptyOptions;
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, parse_options};

const RECEIVER_ROOTS: [&str; 7] = ["self", "this", "cls", "super", "base", "parent", "static"];

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "slop/passthrough-wrapper",
    family: Family::Slop,
    summary: "functions whose body only forwards its parameters to another call",
    why: "A wrapper that only forwards adds a hop and hides nothing.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Off,
        support: Shift::Keep,
        bench: Shift::Off,
        example: Shift::Lower1,
    },
    preset,
    build: Build::File(build),
};

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal | Strictness::Basic | Strictness::Standard => Level::Off,
        Strictness::Strict => Level::At(Severity::Info),
    };
    Preset {
        level,
        options: toml::Table::new(),
    }
}
fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    parse_options::<EmptyOptions>(options)?;
    Ok(Box::new(PassthroughWrapper))
}

struct PassthroughWrapper;

impl FileRule for PassthroughWrapper {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        let index = forward_index(facts);
        for function in &facts.functions {
            let Some(callee) = forwarded_callee(facts, &index, function) else {
                continue;
            };
            out.push(Finding::at(
                facts,
                function.pos,
                INFO.id,
                Severity::Minor,
                format!(
                    "`{}` only forwards its parameters to `{callee}`",
                    function.name
                ),
                format!(
                    "call `{callee}` directly at the call sites of `{}` and delete `{}`",
                    function.name, function.name
                ),
            ));
        }
    }
}

struct ForwardIndex<'a> {
    twins: HashMap<&'a str, u32>,
    valued: HashSet<&'a str>,
}

fn forward_index<'a>(facts: &'a FileFacts<'_>) -> ForwardIndex<'a> {
    let mut twins: HashMap<&str, u32> = HashMap::new();
    for function in &facts.functions {
        *twins.entry(function.name).or_insert(0) += 1;
    }
    let mut valued: HashSet<&str> = HashSet::new();
    for call in &facts.calls {
        for arg in &call.args {
            if let ArgShape::Ident(name) = &arg.shape {
                valued.insert(name);
            }
        }
    }
    ForwardIndex { twins, valued }
}

fn forwarded_callee<'s>(
    facts: &FileFacts<'s>,
    index: &ForwardIndex<'_>,
    function: &Function<'s>,
) -> Option<&'s str> {
    if !cheap_eligible(function) {
        return None;
    }
    let sole_call = usize::try_from(function.sole_call?).ok()?;
    let call = facts.calls.get(sole_call)?;
    if !call.is_simple_path {
        return None;
    }
    if call.args.len() != function.params.len() || !forwards_every_param(call, function) {
        return None;
    }
    let root = call.callee_root?;
    if blocked_root(function, root) {
        return None;
    }
    if !indexed_eligible(index, function) {
        return None;
    }
    Some(call.callee)
}

fn cheap_eligible(function: &Function<'_>) -> bool {
    !function.name.is_empty()
        && !function.signature_forced
        && !function.params.is_empty()
        && function
            .params
            .iter()
            .all(|param| !param.has_default && !param.is_variadic)
        && !function.in_test
        && !function.is_deprecated
        && !function.is_decorated
        && !function.is_trait_item
}

fn indexed_eligible(index: &ForwardIndex<'_>, function: &Function<'_>) -> bool {
    !has_twin(index, function) && !value_used(index, function.name)
}

fn has_twin(index: &ForwardIndex<'_>, function: &Function<'_>) -> bool {
    index
        .twins
        .get(function.name)
        .is_some_and(|&count| count != 1)
}

fn value_used(index: &ForwardIndex<'_>, name: &str) -> bool {
    index.valued.contains(name)
}

fn blocked_root(function: &Function<'_>, root: &str) -> bool {
    RECEIVER_ROOTS.contains(&root)
        || function.receiver.is_some_and(|receiver| receiver == root)
        || function.params.iter().any(|param| param.name == root)
}

fn forwards_every_param(call: &Call<'_>, function: &Function<'_>) -> bool {
    call.args
        .iter()
        .zip(function.params.iter())
        .all(|(arg, param)| matches!(&arg.shape, ArgShape::Ident(name) if *name == param.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::facts::{Arg, CallKind, Param, Pos, PrimitiveKind, Visibility};
    use crate::finding::line_starts;
    use crate::lang::Lang;
    fn param<'s>(name: &'s str) -> Param<'s> {
        Param {
            name,
            ty: None,
            primitive_kind: PrimitiveKind::Other,
            has_default: false,
            bool_default: false,
            keyword_only: false,
            is_variadic: false,
        }
    }

    fn fun<'s>(name: &'s str, params: Vec<Param<'s>>, sole_call: Option<u32>) -> Function<'s> {
        Function {
            name,
            pos: Pos { line: 1, byte: 0 },
            doc: Vec::new(),
            doc_pos: None,
            visibility: Visibility::Private,
            signature_forced: false,
            entry_point: false,
            in_test: false,
            is_overload: false,
            is_deprecated: false,
            is_decorated: false,
            is_trait_item: false,
            is_method: false,
            receiver: None,
            end_line: 1,
            has_value_return: false,
            params,
            return_type: None,
            untyped_throws: false,
            missing_annotations: Vec::new(),
            cognitive: 0,
            max_depth: 0,
            deepest_line: 0,
            sole_call,
        }
    }

    fn ident_arg(name: &str) -> Arg<'_> {
        Arg {
            shape: ArgShape::Ident(name),
            start_line: 1,
            end_line: 1,
        }
    }

    fn call<'s>(
        callee: &'s str,
        root: Option<&'s str>,
        simple: bool,
        args: Vec<Arg<'s>>,
    ) -> Call<'s> {
        Call {
            pos: Pos { line: 1, byte: 0 },
            callee,
            kind: CallKind::Plain,
            callee_root: root,
            is_simple_path: simple,
            initializes_binding: false,
            args,
        }
    }

    fn facts<'s>(
        source: &'s str,
        functions: Vec<Function<'s>>,
        calls: Vec<Call<'s>>,
    ) -> FileFacts<'s> {
        FileFacts {
            lang: Lang::Rust,
            path: "src/lib.rs",
            source,
            line_starts: line_starts(source),
            role: crate::role::CodeRole::Library,
            regions: Vec::new(),
            package: String::new(),
            is_generated: false,
            line_count: 1,
            functions,
            calls,
            bindings: Vec::new(),
            imports: Vec::new(),
            types: Vec::new(),
            impls: Vec::new(),
            wildcard_arms: Vec::new(),
            handlers: Vec::new(),
            assertions: Vec::new(),
            field_copies: Vec::new(),
            dynamic_symbols: Vec::new(),
            ambient: Vec::new(),
            barrel: None,
            has_inline_tests: false,
        }
    }

    fn flagged(source: &str, functions: Vec<Function<'_>>, calls: Vec<Call<'_>>) -> usize {
        let strict = preset(Strictness::Strict);
        let rule = build(strict.options, &Settings::default()).expect("strict preset builds");
        let owned = facts(source, functions, calls);
        let mut out = Vec::new();
        rule.check(&owned, &mut out);
        out.len()
    }

    fn forward_facts<'s>() -> (Vec<Function<'s>>, Vec<Call<'s>>) {
        let functions = vec![fun("greet", vec![param("name")], Some(0))];
        let calls = vec![call(
            "render_greeting",
            Some("render_greeting"),
            true,
            vec![ident_arg("name")],
        )];
        (functions, calls)
    }

    #[test]
    fn strict_only_at_info() {
        assert_eq!(
            preset(Strictness::Minimal).level,
            Level::Off,
            "minimal stays off"
        );
        assert_eq!(
            preset(Strictness::Basic).level,
            Level::Off,
            "basic stays off"
        );
        assert_eq!(
            preset(Strictness::Standard).level,
            Level::Off,
            "standard stays off"
        );
        assert_eq!(
            preset(Strictness::Strict).level,
            Level::At(Severity::Info),
            "strict stays info"
        );
        assert_eq!(INFO.confidence, Confidence::Low, "confidence stays low");
        let (functions, calls) = forward_facts();
        assert_eq!(
            flagged("fn greet() {}\n", functions, calls),
            1,
            "positive input flags once at strict"
        );
    }

    #[test]
    fn chained_call_stays_silent() {
        let functions = vec![fun("file_name", vec![param("path")], Some(0))];
        let calls = vec![call("rsplit", None, false, vec![ident_arg("path")])];
        assert_eq!(flagged("fn file_name() {}\n", functions, calls), 0);
    }

    #[test]
    fn missing_root_stays_silent() {
        let functions = vec![fun("greet", vec![param("name")], Some(0))];
        let calls = vec![call("render_greeting", None, true, vec![ident_arg("name")])];
        assert_eq!(flagged("fn greet() {}\n", functions, calls), 0);
    }

    #[test]
    fn self_delegation_stays_silent() {
        let functions = vec![fun("get", vec![param("key")], Some(0))];
        let calls = vec![call(
            "self.map.get",
            Some("self"),
            true,
            vec![ident_arg("key")],
        )];
        assert_eq!(flagged("fn get() {}\n", functions, calls), 0);
    }

    #[test]
    fn param_root_stays_silent() {
        let functions = vec![fun("wrap", vec![param("path")], Some(0))];
        let calls = vec![call(
            "path.parse",
            Some("path"),
            true,
            vec![ident_arg("path")],
        )];
        assert_eq!(flagged("fn wrap() {}\n", functions, calls), 0);
    }

    #[test]
    fn deprecated_forward_stays_silent() {
        let (mut functions, calls) = forward_facts();
        functions[0].is_deprecated = true;
        assert_eq!(flagged("fn greet() {}\n", functions, calls), 0);
    }

    #[test]
    fn trait_item_stays_silent() {
        let (mut functions, calls) = forward_facts();
        functions[0].is_trait_item = true;
        assert_eq!(flagged("fn greet() {}\n", functions, calls), 0);
    }

    #[test]
    fn decorated_forward_stays_silent() {
        let (mut functions, calls) = forward_facts();
        functions[0].is_decorated = true;
        assert_eq!(flagged("fn greet() {}\n", functions, calls), 0);
    }

    #[test]
    fn test_forward_stays_silent() {
        let (mut functions, calls) = forward_facts();
        functions[0].in_test = true;
        assert_eq!(flagged("fn greet() {}\n", functions, calls), 0);
    }

    #[test]
    fn twin_name_stays_silent() {
        let (mut functions, calls) = forward_facts();
        functions.push(fun("greet", vec![param("name")], None));
        assert_eq!(flagged("fn greet() {}\n", functions, calls), 0);
    }
    #[test]
    fn unnamed_callback_stays_silent() {
        let (mut functions, calls) = forward_facts();
        functions[0].name = "";
        assert_eq!(
            flagged("xs.map((x) => parse(x))\n", functions, calls),
            0,
            "an unnamed callback that forwards one call is not a wrapper"
        );
    }

    #[test]
    fn default_param_stays_silent() {
        let (mut functions, calls) = forward_facts();
        functions[0].params[0].has_default = true;
        assert_eq!(
            flagged(
                "function greet(name = \"world\") { return render(name); }\n",
                functions,
                calls
            ),
            0,
            "a default would be dropped at the call sites"
        );
    }

    #[test]
    fn variadic_param_stays_silent() {
        let (mut functions, calls) = forward_facts();
        functions[0].params[0].is_variadic = true;
        assert_eq!(
            flagged("function g(...xs) { return f(xs); }\n", functions, calls),
            0,
            "callers would have to pass an array"
        );
    }

    #[test]
    fn value_use_stays_silent() {
        let (functions, mut calls) = forward_facts();
        calls.push(call(
            "register",
            Some("register"),
            true,
            vec![ident_arg("greet")],
        ));
        assert_eq!(flagged("fn greet() {}\n", functions, calls), 0);
    }

    #[test]
    fn receiver_rooted_call_stays_silent() {
        let mut functions = vec![fun("get", vec![param("key")], Some(0))];
        functions[0].receiver = Some("c");
        let calls = vec![call("c.items.get", Some("c"), true, vec![ident_arg("key")])];
        assert_eq!(flagged("func (c *Cache) Get() {}\n", functions, calls), 0);
    }

    #[test]
    fn other_object_root_still_flags() {
        let mut functions = vec![fun("get", vec![param("key")], Some(0))];
        functions[0].receiver = Some("c");
        let calls = vec![call("o.items.get", Some("o"), true, vec![ident_arg("key")])];
        assert_eq!(flagged("func (c *Cache) Get() {}\n", functions, calls), 1);
    }
}
