//! `grep/contiguous-log-literal`: log calls whose message is not a searchable literal.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::parse_options;
use crate::callee::CalleePattern;
use crate::config::Strictness;
use crate::facts::{ArgShape, Call, FileFacts};
use crate::finding::{Finding, Level, Severity};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

/// `grep/contiguous-log-literal`: flags log calls whose first argument lacks a fixed literal run.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "grep/contiguous-log-literal",
    family: Family::Grep,
    summary: "log calls whose message is not a searchable literal",
    why: "A log line that is not a fixed literal cannot be searched back to its call site.",
    exempt_tests: false,
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
        Strictness::Minimal | Strictness::Basic => (Level::Off, 8),
        Strictness::Standard => (Level::Warn, 8),
        Strictness::Strict => (Level::Deny, 12),
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
                Severity::Deny,
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
        let Some(arg) = call.args.first() else {
            return Some("no message".to_string());
        };
        match &arg.shape {
            ArgShape::Str { longest_static } if *longest_static < self.min_len => {
                Some(format!("longest literal run is {longest_static} chars"))
            }
            ArgShape::Str { .. } => None,
            ArgShape::Concat => Some("concatenated message".to_string()),
            ArgShape::Ident(_) | ArgShape::Other => Some("dynamic message".to_string()),
        }
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
