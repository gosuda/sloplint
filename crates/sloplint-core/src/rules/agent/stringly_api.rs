//! `agent/stringly-api`: runs of swappable primitive parameters.

use serde::{Deserialize, Serialize};

use crate::config::Strictness;
use crate::facts::{FileFacts, Param};
use crate::finding::{Finding, Level};
use crate::lang::Lang;
use crate::rules::{Build, Family, FileRule, Preset, RuleInfo, Settings, Thresholds};

use super::threshold_options;

const RUST: &[&str] = &[
    "str", "String", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128",
    "usize",
];
const WEB: &[&str] = &["string", "number"];
const GO: &[&str] = &[
    "string", "int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16", "uint32",
    "uint64", "uintptr",
];
const DUCK: &[&str] = &["str", "int"];
const JVM: &[&str] = &[
    "String", "int", "long", "short", "byte", "double", "float", "boolean", "Integer", "Long",
    "Short", "Byte", "Double", "Float", "Boolean", "char", "Char",
];
const SWIFT_TYPES: &[&str] = &[
    "String", "Int", "Int8", "Int16", "Int32", "Int64", "UInt", "UInt8", "UInt16", "UInt32",
    "UInt64", "Double", "Float",
];
const DART: &[&str] = &["String", "int", "double", "bool", "num"];
const C: &[&str] = &[
    "char*",
    "bool",
    "_Bool",
    "int",
    "unsignedint",
    "unsigned",
    "long",
    "unsignedlong",
    "longlong",
    "unsignedlonglong",
    "short",
    "unsignedshort",
    "float",
    "double",
    "size_t",
    "ssize_t",
    "int8_t",
    "int16_t",
    "int32_t",
    "int64_t",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
];
const CPP: &[&str] = &[
    "char*",
    "bool",
    "_Bool",
    "int",
    "unsignedint",
    "unsigned",
    "long",
    "unsignedlong",
    "longlong",
    "unsignedlonglong",
    "short",
    "unsignedshort",
    "float",
    "double",
    "size_t",
    "ssize_t",
    "int8_t",
    "int16_t",
    "int32_t",
    "int64_t",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "std::size_t",
    "std::string",
    "std::string_view",
    "std::int32_t",
    "std::int64_t",
    "std::uint32_t",
    "std::uint64_t",
];
const ZIG: &[&str] = &[
    "[]constu8",
    "i8",
    "i16",
    "i32",
    "i64",
    "u8",
    "u16",
    "u32",
    "u64",
    "usize",
    "f32",
    "f64",
    "bool",
];
const ODIN: &[&str] = &[
    "string", "int", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool",
    "rune",
];
const OCAML: &[&str] = &["string", "int", "int64", "float", "bool", "char"];

const FIX: &str = "wrap these parameters in distinct newtypes or one named struct";

/// `agent/stringly-api`: flags adjacent primitive parameters callers can swap.
pub(crate) const INFO: RuleInfo = RuleInfo {
    id: "agent/stringly-api",
    family: Family::Agent,
    summary: "runs of consecutive primitive parameters of swappable types",
    why: "Adjacent primitive parameters of one type can be swapped without a compile error.",
    exempt_tests: true,
    preset,
    build: Build::File(build),
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    warn_at: u32,
    deny_at: u32,
}

struct StringlyApi {
    thresholds: Thresholds,
}

fn preset(strictness: Strictness) -> Preset {
    let (level, warn_at, deny_at) = match strictness {
        Strictness::Minimal => (Level::Off, 3, 4),
        Strictness::Basic => (Level::Off, 3, 4),
        Strictness::Standard => (Level::Warn, 3, 4),
        Strictness::Strict => (Level::Deny, 2, 3),
    };
    Preset {
        level,
        options: threshold_options(warn_at, deny_at),
    }
}

fn build(options: toml::Table, _settings: &Settings) -> Result<Box<dyn FileRule>, String> {
    let parsed: Options = toml::Value::Table(options)
        .try_into()
        .map_err(|error| error.to_string())?;
    let thresholds = Thresholds::new(parsed.warn_at, parsed.deny_at)?;
    Ok(Box::new(StringlyApi { thresholds }))
}

impl FileRule for StringlyApi {
    fn check(&self, facts: &FileFacts<'_>, out: &mut Vec<Finding>) {
        let primitives = primitives(facts.lang);
        for function in &facts.functions {
            if !function.public || function.in_test {
                continue;
            }
            let Some((start, len)) = longest_run(&function.params, primitives) else {
                continue;
            };
            let Some(severity) = self
                .thresholds
                .grade(u32::try_from(len).unwrap_or(u32::MAX))
            else {
                continue;
            };
            let names = function.params[start..start + len]
                .iter()
                .map(|param| param.name)
                .collect::<Vec<_>>()
                .join(", ");
            let name = function.name;
            let message = format!(
                "`{name}` takes {len} consecutive primitive parameters ({names}) that callers can swap"
            );
            out.push(Finding::at(
                facts,
                function.pos,
                INFO.id,
                severity,
                message,
                FIX.to_string(),
            ));
        }
    }
}

fn primitives(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Rust => RUST,
        Lang::TypeScript | Lang::Tsx | Lang::JavaScript => WEB,
        Lang::Go => GO,
        Lang::Python | Lang::Starlark => DUCK,
        Lang::Java | Lang::Kotlin | Lang::Scala | Lang::CSharp => JVM,
        Lang::Swift => SWIFT_TYPES,
        Lang::Dart => DART,
        Lang::Cpp => CPP,
        Lang::C | Lang::ObjC => C,
        Lang::Zig => ZIG,
        Lang::Odin => ODIN,
        Lang::Ocaml | Lang::OcamlInterface => OCAML,
        _ => &[],
    }
}

fn longest_run(params: &[Param<'_>], primitives: &[&str]) -> Option<(usize, usize)> {
    let mut best = (0usize, 0usize);
    let mut open: Option<usize> = None;
    for (index, param) in params.iter().enumerate() {
        if is_primitive(param, primitives) {
            if open.is_none() {
                open = Some(index);
            }
        } else if let Some(begin) = open.take() {
            best = longer(best, (begin, index - begin));
        }
    }
    if let Some(begin) = open {
        best = longer(best, (begin, params.len() - begin));
    }
    (best.1 > 0).then_some(best)
}

fn longer(current: (usize, usize), candidate: (usize, usize)) -> (usize, usize) {
    if candidate.1 > current.1 {
        candidate
    } else {
        current
    }
}

fn is_primitive(param: &Param<'_>, primitives: &[&str]) -> bool {
    param
        .ty
        .is_some_and(|ty| primitives.contains(&normalize(ty).as_str()))
}

fn normalize(ty: &str) -> String {
    let mut normalized = String::with_capacity(ty.len());
    for word in ty.split_whitespace() {
        let word = word.trim_start_matches('&');
        if word.starts_with('\'') || word == "mut" || word == "const" {
            continue;
        }
        normalized.push_str(word);
    }
    normalized
}
