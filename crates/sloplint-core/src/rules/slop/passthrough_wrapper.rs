//! `slop/passthrough-wrapper`: functions whose body is exactly one forwarded call.

use crate::config::Strictness;
use crate::facts::{ArgShape, Call, FileFacts, Function};
use crate::finding::{Finding, Level, Severity};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings};

const RECEIVER_ROOTS: [&str; 4] = ["self", "this", "cls", "super"];

/// `slop/passthrough-wrapper`: a function that only forwards its parameters.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "slop/passthrough-wrapper",
    family: Family::Slop,
    summary: "functions whose body only forwards its parameters to another call",
    why: "A wrapper that only forwards adds a hop and hides nothing.",
    exempt_tests: false,
    preset,
    build: Build::File(build),
};

fn preset(strictness: Strictness) -> Preset {
    let level = match strictness {
        Strictness::Minimal => Level::Off,
        Strictness::Basic | Strictness::Standard | Strictness::Strict => Level::Warn,
    };
    Preset {
        level,
        options: toml::Table::new(),
    }
}

fn build(_options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    Ok(Box::new(PassthroughWrapper))
}

struct PassthroughWrapper;

impl FileRule for PassthroughWrapper {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        for function in &facts.functions {
            let Some(callee) = forwarded_callee(facts, function) else {
                continue;
            };
            out.push(Finding::at(
                facts,
                function.pos,
                INFO.id,
                Severity::Warn,
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

fn forwarded_callee<'s>(facts: &FileFacts<'s>, function: &Function<'s>) -> Option<&'s str> {
    if function.interface_bound || function.params.is_empty() {
        return None;
    }
    let sole_call = usize::try_from(function.sole_call?).ok()?;
    let call = facts.calls.get(sole_call)?;
    if call.args.len() != function.params.len() || !forwards_every_param(call, function) {
        return None;
    }
    if let Some(root) = call.callee_root
        && RECEIVER_ROOTS.contains(&root)
    {
        return None;
    }
    Some(call.callee)
}

fn forwards_every_param(call: &Call<'_>, function: &Function<'_>) -> bool {
    call.args
        .iter()
        .zip(function.params.iter())
        .all(|(arg, param)| matches!(&arg.shape, ArgShape::Ident(name) if *name == param.name))
}
