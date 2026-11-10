use std::collections::BTreeMap;

use crate::callee::CalleePattern;
use crate::config::Strictness;
use crate::facts::{CallKind, FileFacts, Stmt};
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};
use crate::rules::{EmptyOptions, parse_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "slop/zombie-catch",
    family: Family::Slop,
    summary: "error handlers that swallow or log and rethrow",
    why: "Swallowed or log-and-rethrow handlers hide or duplicate failures.",
    confidence: Confidence::Low,
    roles: RolePolicy {
        entry: Shift::Keep,
        test: Shift::Lower1,
        support: Shift::Keep,
        bench: Shift::Lower1,
        example: Shift::Keep,
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

fn build(options: toml::Table, settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    parse_options::<EmptyOptions>(options)?;
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
            if handler.has_else || handler.try_imports_only || handler.in_cleanup {
                continue;
            }
            let Some((kind, fix)) = classify(patterns, &handler.stmts) else {
                continue;
            };
            out.push(Finding::at(
                facts,
                handler.pos,
                INFO.id,
                Severity::Major,
                format!("error handler {kind}"),
                fix.to_string(),
            ));
        }
    }
}

fn classify<'s>(
    patterns: &[CalleePattern],
    stmts: &[Stmt<'s>],
) -> Option<(&'static str, &'static str)> {
    let (stmts, logs) = trim_log_calls(patterns, stmts);
    match stmts {
        [] if logs == 0 => Some((
            "swallows the error",
            "handle the error or propagate it; never discard it silently",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{FileFacts, Handler, Pos};
    use crate::finding::line_starts;

    fn handler(stmts: Vec<Stmt<'_>>) -> Handler<'_> {
        build_handler(stmts, false, false, false)
    }

    fn build_handler(
        stmts: Vec<Stmt<'_>>,
        has_else: bool,
        try_imports_only: bool,
        in_cleanup: bool,
    ) -> Handler<'_> {
        Handler {
            pos: Pos { line: 1, byte: 0 },
            stmts,
            has_else,
            try_imports_only,
            in_cleanup,
        }
    }

    fn facts<'s>(source: &'s str, handlers: Vec<Handler<'s>>) -> FileFacts<'s> {
        FileFacts {
            lang: crate::lang::Lang::JavaScript,
            path: "src/lib.rs",
            source,
            line_starts: line_starts(source),
            role: crate::role::CodeRole::Library,
            regions: Vec::new(),
            package: String::new(),
            is_generated: false,
            line_count: 1,
            functions: Vec::new(),
            calls: Vec::new(),
            bindings: Vec::new(),
            imports: Vec::new(),
            types: Vec::new(),
            impls: Vec::new(),
            wildcard_arms: Vec::new(),
            handlers,
            assertions: Vec::new(),
            field_copies: Vec::new(),
            dynamic_symbols: Vec::new(),
            ambient: Vec::new(),
            barrel: None,
            has_inline_tests: false,
        }
    }

    fn rule() -> Box<dyn FileRule> {
        let strict = preset(Strictness::Strict);
        let pattern = CalleePattern::parse("console.error").expect("pattern parses");
        let mut settings = Settings::default();
        settings
            .log_callees
            .insert(crate::lang::Lang::JavaScript.config_key(), vec![pattern]);
        build(strict.options, &settings).expect("strict preset builds")
    }

    fn flagged(source: &str, handlers: Vec<Handler<'_>>) -> usize {
        let owned = facts(source, handlers);
        let mut out = Vec::new();
        rule().check(&owned, &mut out);
        out.len()
    }

    fn log_call() -> Stmt<'static> {
        Stmt::Call {
            callee: "console.error",
            kind: CallKind::Plain,
        }
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
        assert_eq!(
            flagged("try: pass\n", vec![handler(Vec::new())]),
            1,
            "positive input flags once at strict"
        );
    }

    #[test]
    fn empty_swallow_flags() {
        assert_eq!(flagged("try: pass\n", vec![handler(Vec::new())]), 1);
    }

    #[test]
    fn else_swallow_stays_silent() {
        let handlers = vec![build_handler(Vec::new(), true, false, false)];
        assert_eq!(flagged("try: pass\n", handlers), 0);
    }

    #[test]
    fn imports_only_swallow_stays_silent() {
        let handlers = vec![build_handler(Vec::new(), false, true, false)];
        assert_eq!(flagged("try: pass\n", handlers), 0);
    }

    #[test]
    fn cleanup_swallow_stays_silent() {
        let handlers = vec![build_handler(Vec::new(), false, false, true)];
        assert_eq!(flagged("try: pass\n", handlers), 0);
    }

    #[test]
    fn bare_rethrow_stays_silent() {
        let handlers = vec![handler(vec![Stmt::Rethrow])];
        assert_eq!(flagged("try: pass\n", handlers), 0);
    }

    #[test]
    fn log_and_rethrow_flags() {
        let handlers = vec![handler(vec![log_call(), Stmt::Rethrow])];
        assert_eq!(flagged("try: pass\n", handlers), 1);
    }
}
