use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::callee::CalleePattern;
use crate::config::Strictness;
use crate::facts::{ArgShape, Call, FileFacts};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::parse_options;
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/contiguous-log-literal",
    family: Family::Grep,
    summary: "log calls whose message is not a searchable literal",
    why: "A log line that is not a fixed literal cannot be searched back to its call site.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Off,
        support: Shift::Lower1,
        bench: Shift::Off,
        example: Shift::Off,
    },
    preset,
    build: Build::File(build),
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    min_len: u32,
}

struct ContiguousLogLiteral {
    min_len: u32,
    log_callees: BTreeMap<&'static str, Vec<CalleePattern>>,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, min_len) = match strictness {
        Strictness::Minimal | Strictness::Basic | Strictness::Standard => (Level::Off, 8),
        Strictness::Strict => (Level::At(Severity::Info), 8),
    };
    Preset {
        level,
        options: fixed_options(min_len),
    }
}

fn fixed_options(min_len: u32) -> toml::Table {
    let mut options = toml::Table::new();
    options.insert(
        "min_len".to_string(),
        toml::Value::Integer(i64::from(min_len)),
    );
    options
}

fn build(options: toml::Table, settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let options: Options = parse_options(options)?;
    if options.min_len < 1 {
        return Err(format!("min_len {} must be at least 1", options.min_len));
    }
    Ok(Box::new(ContiguousLogLiteral {
        min_len: options.min_len,
        log_callees: settings.log_callees.clone(),
    }))
}

impl FileRule for ContiguousLogLiteral {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        let Some(patterns) = self.log_callees.get(facts.lang.config_key()) else {
            return;
        };
        for call in &facts.calls {
            if !is_log_call(patterns, call) {
                continue;
            }
            let Some(reason) = self.reason(call) else {
                continue;
            };
            out.push(Finding::at(
                facts,
                call.pos,
                INFO.id,
                Severity::Major,
                format!("log or error message is not a searchable literal ({reason})"),
                format!(
                    "start the message with a fixed literal of at least {} characters and pass dynamic values as format arguments",
                    self.min_len
                ),
            ));
        }
    }
}

impl ContiguousLogLiteral {
    fn reason(&self, call: &Call<'_>) -> Option<String> {
        if call.args.is_empty() {
            return Some("no message".to_string());
        }
        for arg in &call.args {
            match &arg.shape {
                ArgShape::Str { longest_static } | ArgShape::Concat { longest_static } => {
                    if *longest_static < self.min_len {
                        return Some(format!("longest literal run is {longest_static} chars"));
                    }
                    return None;
                }
                ArgShape::Ident(_)
                | ArgShape::Other
                | ArgShape::Collection
                | ArgShape::CodeBody => {}
            }
        }
        Some("dynamic message".to_string())
    }
}

fn is_log_call(patterns: &[CalleePattern], call: &Call<'_>) -> bool {
    for pattern in patterns {
        if pattern.matches(call.callee, call.kind) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Arg, CallKind, Pos};
    use crate::lang::Lang;
    use crate::role::CodeRole;

    fn rule() -> ContiguousLogLiteral {
        let strict = preset(Strictness::Strict);
        assert_eq!(
            strict.level,
            Level::At(Severity::Info),
            "strict stays informational"
        );
        let options: Options = parse_options(strict.options).expect("strict preset parses");
        ContiguousLogLiteral {
            min_len: options.min_len,
            log_callees: [(Lang::Rust.config_key(), patterns(&["error!"]))]
                .into_iter()
                .collect(),
        }
    }

    fn patterns(texts: &[&str]) -> Vec<CalleePattern> {
        let mut out = Vec::new();
        for text in texts {
            if let Ok(pattern) = CalleePattern::parse(text) {
                out.push(pattern);
            }
        }
        out
    }

    fn facts(calls: Vec<Call<'static>>) -> FileFacts<'static> {
        FileFacts {
            lang: Lang::Rust,
            path: "src/main.rs",
            source: "",
            line_starts: vec![0],
            role: CodeRole::Library,
            regions: Vec::new(),
            line_count: 1,
            package: String::new(),
            is_generated: false,
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
        Call {
            pos: Pos { line: 1, byte: 0 },
            callee: "error!",
            kind: CallKind::Macro,
            callee_root: None,
            is_simple_path: true,
            initializes_binding: false,
            args,
        }
    }

    fn str_arg(longest_static: u32) -> Arg<'static> {
        Arg {
            shape: ArgShape::Str { longest_static },
            start_line: 1,
            end_line: 1,
        }
    }

    fn ident_arg(name: &'static str) -> Arg<'static> {
        Arg {
            shape: ArgShape::Ident(name),
            start_line: 1,
            end_line: 1,
        }
    }

    fn concat_arg(longest_static: u32) -> Arg<'static> {
        Arg {
            shape: ArgShape::Concat { longest_static },
            start_line: 1,
            end_line: 1,
        }
    }

    fn check(rule: &ContiguousLogLiteral, calls: Vec<Call<'static>>) -> Vec<Finding> {
        let facts = facts(calls);
        let mut out = Vec::new();
        rule.check(&facts, &mut out);
        out
    }

    #[test]
    fn short_literal_still_flags() {
        let out = check(&rule(), vec![call(vec![str_arg(4)])]);
        assert_eq!(out.len(), 1, "short literals flag once: {out:?}");
        assert!(
            out[0].message.contains("longest literal run is 4 chars"),
            "the finding names the run: {}",
            out[0].message
        );
    }

    #[test]
    fn short_concat_flags() {
        let out = check(&rule(), vec![call(vec![concat_arg(7)])]);
        assert_eq!(out.len(), 1, "short concats flag once: {out:?}");
        assert!(
            out[0].message.contains("longest literal run is 7 chars"),
            "the finding names the run: {}",
            out[0].message
        );
    }

    #[test]
    fn boundary_concat_passes() {
        let out = check(&rule(), vec![call(vec![concat_arg(8)])]);
        assert!(out.is_empty(), "eight-char concats pass: {out:?}");
    }

    #[test]
    fn empty_concat_flags() {
        let out = check(&rule(), vec![call(vec![concat_arg(0)])]);
        assert_eq!(out.len(), 1, "empty concats flag once: {out:?}");
        assert!(
            out[0].message.contains("longest literal run is 0 chars"),
            "zero-static concats keep the run text: {}",
            out[0].message
        );
    }

    #[test]
    fn empty_str_reports_zero_run() {
        let out = check(&rule(), vec![call(vec![str_arg(0)])]);
        assert_eq!(out.len(), 1, "empty literals flag once: {out:?}");
        assert!(
            out[0].message.contains("longest literal run is 0 chars"),
            "empty literals keep the run text: {}",
            out[0].message
        );
    }

    #[test]
    fn receiver_then_literal_passes() {
        let out = check(&rule(), vec![call(vec![ident_arg("err"), str_arg(20)])]);
        assert!(out.is_empty(), "literals after a receiver pass: {out:?}");
    }

    #[test]
    fn missing_message_flags() {
        let out = check(&rule(), vec![call(vec![])]);
        assert_eq!(out.len(), 1, "missing messages flag once: {out:?}");
    }

    #[test]
    fn off_at_standard_info_at_strict() {
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
            "strict stays informational"
        );
        assert_eq!(INFO.confidence, Confidence::Low, "confidence stays low");
        assert_eq!(
            preset(Strictness::Strict).options.get("min_len"),
            Some(&toml::Value::Integer(8)),
            "strict keeps the eight-char floor"
        );
        let out = check(&rule(), vec![call(vec![str_arg(4)])]);
        assert_eq!(out.len(), 1, "strict flags a short literal once: {out:?}");
    }
}
