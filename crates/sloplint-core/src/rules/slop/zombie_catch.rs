//! `slop/zombie-catch`: handlers that swallow, rethrow unchanged, or log-and-rethrow.

use std::collections::BTreeMap;

use crate::callee::CalleePattern;
use crate::config::Strictness;
use crate::facts::{CallKind, FileFacts, Stmt};
use crate::finding::{Finding, Level, Severity};
use crate::lang::Lang;
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

/// `slop/zombie-catch`: an error handler that hides or duplicates the failure.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "slop/zombie-catch",
    family: Family::Slop,
    summary: "error handlers that swallow, rethrow unchanged, or log and rethrow",
    why: "Swallowed, unchanged, or log-and-rethrow handlers hide or duplicate failures.",
    exempt_tests: false,
    preset,
    build: Build::File(build),
};

fn preset(_strictness: Strictness) -> Preset {
    Preset {
        level: Level::Deny,
        options: toml::Table::new(),
    }
}

fn build(_options: toml::Table, settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    Ok(Box::new(ZombieCatch {
        log_callees: settings.log_callees.clone(),
    }))
}

struct ZombieCatch {
    log_callees: BTreeMap<&'static str, Vec<CalleePattern>>,
}

impl FileRule for ZombieCatch {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        let patterns = self
            .log_callees
            .get(facts.lang.config_key())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for handler in &facts.handlers {
            let Some((kind, fix)) = classify(facts.lang, patterns, &handler.stmts) else {
                continue;
            };
            out.push(Finding::at(
                facts,
                handler.pos,
                INFO.id,
                Severity::Deny,
                format!("error handler {kind}"),
                fix.to_string(),
            ));
        }
    }
}

fn classify<'s>(
    lang: Lang,
    patterns: &[CalleePattern],
    stmts: &[Stmt<'s>],
) -> Option<(&'static str, &'static str)> {
    let (stmts, logs) = trim_log_calls(patterns, stmts);
    match stmts {
        [] if logs == 0 => Some((
            "swallows the error",
            "handle the error or propagate it; never discard it silently",
        )),
        [Stmt::Rethrow] if logs == 0 && lang != Lang::Go => Some((
            "rethrows the error unchanged",
            "delete the handler and let the error propagate (`?` in Rust)",
        )),
        [Stmt::Rethrow] if logs > 0 => Some((
            "logs and rethrows the same error",
            "either log and stop propagation here, or propagate without logging; doing both reports the failure twice",
        )),
        _ => None,
    }
}

fn trim_log_calls<'a, 's>(
    patterns: &[CalleePattern],
    stmts: &'a [Stmt<'s>],
) -> (&'a [Stmt<'s>], u32) {
    let mut rest = stmts;
    let mut logs = 0u32;
    while let Some(Stmt::Call { callee, kind }) = rest.first()
        && is_log(patterns, callee, *kind)
    {
        rest = &rest[1..];
        logs += 1;
    }
    while let Some(Stmt::Call { callee, kind }) = rest.last()
        && is_log(patterns, callee, *kind)
    {
        rest = &rest[..rest.len() - 1];
        logs += 1;
    }
    (rest, logs)
}

fn is_log(patterns: &[CalleePattern], callee: &str, kind: CallKind) -> bool {
    patterns.iter().any(|pattern| pattern.matches(callee, kind))
}
