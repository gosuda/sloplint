use serde::{Deserialize, Serialize};

use crate::callee::CalleePattern;
use crate::config::Strictness;
use crate::facts::{ArgShape, Call, FileFacts};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};
use crate::rules::{parse_options, string_values};

const ALLOWED_CALLEES: &[&str] = &[
    "describe",
    "it",
    "test",
    "beforeEach",
    "afterEach",
    "beforeAll",
    "afterAll",
    "spawn",
    "scope",
    "t.Run",
    "b.Run",
];

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
    allow_callees: Vec<String>,
    minor_at: u32,
    major_at: u32,
    critical_at: u32,
    blocker_at: u32,
}

struct MultilineArgument {
    allow: Vec<CalleePattern>,
    thresholds: Thresholds,
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
    options.insert("allow_callees".to_string(), string_values(ALLOWED_CALLEES));
    options.insert("minor_at".to_string(), toml::Value::Integer(6));
    options.insert("major_at".to_string(), toml::Value::Integer(15));
    options.insert("critical_at".to_string(), toml::Value::Integer(23));
    options.insert("blocker_at".to_string(), toml::Value::Integer(30));
    options
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let options: Options = parse_options(options)?;
    let thresholds = Thresholds::new(
        options.minor_at,
        options.major_at,
        options.critical_at,
        options.blocker_at,
    )?;
    let mut allow = Vec::new();
    for callee in &options.allow_callees {
        allow.push(CalleePattern::parse(callee)?);
    }
    Ok(Box::new(MultilineArgument { allow, thresholds }))
}

impl FileRule for MultilineArgument {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for call in &facts.calls {
            if self.allowed(call) {
                continue;
            }
            let Some((index, span)) = widest_argument(call) else {
                continue;
            };
            let Some(severity) = self.thresholds.grade(span) else {
                continue;
            };
            let minor_at = self.thresholds.minor_at;
            let major_at = self.thresholds.major_at;
            let critical_at = self.thresholds.critical_at;
            let blocker_at = self.thresholds.blocker_at;
            out.push(Finding::at(
                facts,
                call.pos,
                INFO.id,
                severity,
                format!("argument {index} of `{}` spans {span} lines (minor at {minor_at}, major at {major_at}, critical at {critical_at}, blocker at {blocker_at})", call.callee),
                "bind the argument to a named local before the call so the call reads on one line"
                    .to_string(),
            ));
        }
    }
}

impl MultilineArgument {
    fn allowed(&self, call: &Call<'_>) -> bool {
        for pattern in &self.allow {
            if pattern.matches(call.callee, call.kind) {
                return true;
            }
        }
        false
    }
}

fn widest_argument(call: &Call<'_>) -> Option<(usize, u32)> {
    let mut best: Option<(usize, u32)> = None;
    for (index, arg) in call.args.iter().enumerate() {
        if matches!(arg.shape, ArgShape::Str { .. }) {
            continue;
        }
        let span = arg.end_line.saturating_sub(arg.start_line) + 1;
        if best.is_none_or(|(_, widest)| span > widest) {
            best = Some((index + 1, span));
        }
    }
    best
}
