use crate::config::Strictness;
use crate::facts::FileFacts;
use crate::finding::{Confidence, Finding, Level, Severity};
use crate::role::{RolePolicy, Shift};
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, parse_options};

pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "slop/field-copy",
    family: Family::Slop,
    summary: "struct literals copying every field one-to-one from another value",
    why: "One-to-one field copying means two types carry the same data.",
    confidence: Confidence::Low,
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

fn preset(strictness: Strictness) -> Preset {
    let (level, min_fields) = match strictness {
        Strictness::Minimal | Strictness::Basic => (Level::Off, 6),
        Strictness::Standard => (Level::At(Severity::Minor), 6),
        Strictness::Strict => (Level::At(Severity::Minor), 4),
    };
    Preset {
        level,
        options: options(min_fields),
    }
}

fn options(min_fields: u32) -> toml::Table {
    let mut table = toml::Table::new();
    table.insert(
        "min_fields".to_string(),
        toml::Value::Integer(i64::from(min_fields)),
    );
    table
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let options: Options = parse_options(options)?;
    if options.min_fields < 2 {
        return Err(format!(
            "min_fields {} must be at least 2",
            options.min_fields
        ));
    }
    Ok(Box::new(FieldCopy {
        min_fields: options.min_fields,
    }))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    min_fields: u32,
}

struct FieldCopy {
    min_fields: u32,
}

impl FileRule for FieldCopy {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        let mut open: Vec<&crate::facts::Function<'_>> = Vec::new();
        let mut next = 0;
        for copy in &facts.field_copies {
            let Some(source) = copy.source else {
                continue;
            };
            if copy.fields < self.min_fields {
                continue;
            }
            while next < facts.functions.len() && facts.functions[next].pos.line <= copy.pos.line {
                while open
                    .last()
                    .is_some_and(|function| function.end_line < facts.functions[next].pos.line)
                {
                    open.pop();
                }
                open.push(&facts.functions[next]);
                next += 1;
            }
            while open
                .last()
                .is_some_and(|function| copy.pos.line > function.end_line)
            {
                open.pop();
            }
            let owned = open.iter().rev().any(|function| {
                copy.pos.line <= function.end_line
                    && function.receiver.is_some_and(|receiver| receiver == source)
            });
            if owned {
                continue;
            }
            out.push(Finding::at(
                facts,
                copy.pos,
                INFO.id,
                Severity::Minor,
                format!("{} fields copied one-to-one from `{source}`", copy.fields),
                "if both types carry the same data, merge them into one type; if this copy is the deliberate conversion, keep it and suppress with a reason".to_string(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Function, Pos, Visibility};
    use crate::finding::line_starts;
    use crate::lang::Lang;

    fn method<'s>(receiver: Option<&'s str>, first_line: u32, end_line: u32) -> Function<'s> {
        Function {
            name: "clone",
            pos: Pos {
                line: first_line,
                byte: 0,
            },
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
            is_method: true,
            receiver,
            params: Vec::new(),
            return_type: None,
            untyped_throws: false,
            missing_annotations: Vec::new(),
            cognitive: 0,
            max_depth: 0,
            deepest_line: end_line,
            end_line,
            has_value_return: true,
            sole_call: None,
        }
    }

    fn rebuild<'s>(source: Option<&'s str>, fields: u32, line: u32) -> crate::facts::FieldCopy<'s> {
        crate::facts::FieldCopy {
            pos: Pos { line, byte: 0 },
            fields,
            source,
        }
    }

    fn flagged<'s>(
        source: &'s str,
        functions: Vec<Function<'s>>,
        copies: Vec<crate::facts::FieldCopy<'s>>,
    ) -> usize {
        let strict = preset(Strictness::Strict);
        let rule = build(strict.options, &Settings::default()).expect("strict preset builds");
        let owned = FileFacts {
            lang: Lang::Go,
            path: "pkg/cache.go",
            source,
            line_starts: line_starts(source),
            role: crate::role::CodeRole::Library,
            regions: Vec::new(),
            line_count: 9,
            package: String::new(),
            is_generated: false,
            functions,
            calls: Vec::new(),
            bindings: Vec::new(),
            imports: Vec::new(),
            types: Vec::new(),
            impls: Vec::new(),
            wildcard_arms: Vec::new(),
            handlers: Vec::new(),
            assertions: Vec::new(),
            field_copies: copies,
            dynamic_symbols: Vec::new(),
            ambient: Vec::new(),
            barrel: None,
            has_inline_tests: false,
        };
        let mut out = Vec::new();
        rule.check(&owned, &mut out);
        out.len()
    }

    #[test]
    fn receiver_sourced_rebuild_stays_silent() {
        let functions = vec![method(Some("c"), 1, 9)];
        let copies = vec![rebuild(Some("c"), 6, 5)];
        assert_eq!(flagged("package probe\n", functions, copies), 0);
    }

    #[test]
    fn other_sourced_rebuild_still_flags() {
        let functions = vec![method(Some("c"), 1, 9)];
        let copies = vec![rebuild(Some("p"), 6, 5)];
        assert_eq!(flagged("package probe\n", functions, copies), 1);
    }

    #[test]
    fn copy_outside_method_range_still_flags() {
        let functions = vec![method(Some("c"), 1, 4)];
        let copies = vec![rebuild(Some("c"), 6, 7)];
        assert_eq!(flagged("package probe\n", functions, copies), 1);
    }
}
