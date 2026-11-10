use serde::{Deserialize, Serialize};

use super::parse_options;
use crate::config::Strictness;
use crate::facts::{ArgShape, Call, FileFacts};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/multiline-argument",
    family: Family::Grep,
    summary: "call arguments spanning many lines",
    why: "Long inline arguments split a call across lines, so line-based search shows no context.",
    confidence: Confidence::Medium,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Off,
        support: Shift::Lower1,
        bench: Shift::Off,
        example: Shift::Lower1,
    },
    preset,
    build: Build::File(build),
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    min_span: u32,
}

struct MultilineArgument {
    min_span: u32,
}

fn preset(strictness: Strictness) -> Preset {
    Preset {
        level: match strictness {
            Strictness::Strict => Level::At(Severity::Minor),
            Strictness::Minimal | Strictness::Basic | Strictness::Standard => Level::Off,
        },
        options: fixed_options(),
    }
}

fn fixed_options() -> toml::Table {
    let mut options = toml::Table::new();
    options.insert("min_span".to_string(), toml::Value::Integer(15));
    options
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let options: Options = parse_options(options)?;
    if options.min_span < 1 {
        return Err(format!("min_span {} must be at least 1", options.min_span));
    }
    Ok(Box::new(MultilineArgument {
        min_span: options.min_span,
    }))
}

impl FileRule for MultilineArgument {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        if facts.is_generated {
            return;
        }
        for call in &facts.calls {
            if call.initializes_binding {
                continue;
            }
            let Some((index, span)) = widest_collection(call) else {
                continue;
            };
            if span < self.min_span {
                continue;
            }
            out.push(Finding::at(
                facts,
                call.pos,
                INFO.id,
                Severity::Major,
                format!(
                    "argument {index} of `{}` spans {span} lines (minor at {})",
                    call.callee, self.min_span
                ),
                "bind the argument to a named local before the call so the call reads on one line"
                    .to_string(),
            ));
        }
    }
}

fn widest_collection(call: &Call<'_>) -> Option<(usize, u32)> {
    let mut best: Option<(usize, u32)> = None;
    for (index, arg) in call.args.iter().enumerate() {
        if !matches!(arg.shape, ArgShape::Collection) {
            continue;
        }
        let span = arg.end_line.saturating_sub(arg.start_line) + 1;
        if best.is_none_or(|(_, widest)| span > widest) {
            best = Some((index + 1, span));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Arg, CallKind, Pos};
    use crate::lang::Lang;
    use crate::role::CodeRole;

    fn rule() -> MultilineArgument {
        MultilineArgument { min_span: 15 }
    }

    fn facts(calls: Vec<Call<'static>>) -> FileFacts<'static> {
        facts_with_generated(false, calls)
    }

    fn generated_facts(calls: Vec<Call<'static>>) -> FileFacts<'static> {
        facts_with_generated(true, calls)
    }

    fn facts_with_generated(generated: bool, calls: Vec<Call<'static>>) -> FileFacts<'static> {
        FileFacts {
            lang: Lang::Rust,
            path: "src/main.rs",
            source: "",
            line_starts: vec![0],
            role: CodeRole::Library,
            regions: Vec::new(),
            line_count: 20,
            package: String::new(),
            is_generated: generated,
            functions: Vec::new(),
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

    fn call(args: Vec<Arg<'static>>) -> Call<'static> {
        named_call(false, args)
    }

    fn binding_initializer(args: Vec<Arg<'static>>) -> Call<'static> {
        named_call(true, args)
    }

    fn named_call(initializes_binding: bool, args: Vec<Arg<'static>>) -> Call<'static> {
        Call {
            pos: Pos { line: 1, byte: 0 },
            callee: "compute",
            kind: CallKind::Plain,
            callee_root: None,
            is_simple_path: true,
            initializes_binding,
            args,
        }
    }

    fn collection(start_line: u32, end_line: u32) -> Arg<'static> {
        Arg {
            shape: ArgShape::Collection,
            start_line,
            end_line,
        }
    }

    fn code_body(start_line: u32, end_line: u32) -> Arg<'static> {
        Arg {
            shape: ArgShape::CodeBody,
            start_line,
            end_line,
        }
    }

    fn concat() -> Arg<'static> {
        Arg {
            shape: ArgShape::Concat,
            start_line: 1,
            end_line: 20,
        }
    }

    fn check(rule: &MultilineArgument, calls: Vec<Call<'static>>) -> Vec<Finding> {
        run_check(rule, facts(calls))
    }

    fn check_generated(rule: &MultilineArgument, calls: Vec<Call<'static>>) -> Vec<Finding> {
        run_check(rule, generated_facts(calls))
    }

    fn run_check(rule: &MultilineArgument, facts: FileFacts<'static>) -> Vec<Finding> {
        let mut out = Vec::new();
        rule.check(&facts, &mut out);
        out
    }

    #[test]
    fn callback_body_stays_silent() {
        let out = check(&rule(), vec![call(vec![code_body(1, 18)])]);
        assert!(out.is_empty(), "callback bodies stay silent: {out:?}");
    }

    #[test]
    fn short_collection_stays_silent() {
        let out = check(&rule(), vec![call(vec![collection(1, 6)])]);
        assert!(out.is_empty(), "short collections stay silent: {out:?}");
    }

    #[test]
    fn long_collection_flags() {
        let out = check(&rule(), vec![call(vec![collection(1, 15)])]);
        assert_eq!(out.len(), 1, "long collections flag once: {out:?}");
        assert!(
            out[0].message.contains("spans 15 lines"),
            "the finding names the span: {}",
            out[0].message
        );
    }

    #[test]
    fn long_concat_stays_silent() {
        let out = check(&rule(), vec![call(vec![concat()])]);
        assert!(
            out.is_empty(),
            "concatenated arguments stay silent: {out:?}"
        );
    }

    #[test]
    fn binding_initializer_stays_silent() {
        let out = check(&rule(), vec![binding_initializer(vec![collection(1, 20)])]);
        assert!(out.is_empty(), "binding initializers stay silent: {out:?}");
    }

    #[test]
    fn generated_file_stays_silent() {
        let out = check_generated(&rule(), vec![call(vec![collection(1, 20)])]);
        assert!(out.is_empty(), "generated files stay silent: {out:?}");
    }

    #[test]
    fn strict_only_at_minor() {
        assert_eq!(
            preset(Strictness::Standard).level,
            Level::Off,
            "standard stays off"
        );
        assert_eq!(
            preset(Strictness::Strict).level,
            Level::At(Severity::Minor),
            "strict stays minor"
        );
        assert_eq!(
            preset(Strictness::Strict).options.get("min_span"),
            Some(&toml::Value::Integer(15)),
            "min_span stays 15 at strict"
        );
    }
}
