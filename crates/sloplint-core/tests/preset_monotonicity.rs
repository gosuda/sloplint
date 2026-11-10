//! Catalog-wide preset contract: rising strictness never loosens a rule.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use sloplint_core::CATALOG;
use sloplint_core::config::Strictness;
use sloplint_core::finding::{Level, Severity};

const PROFILES: [Strictness; 4] = [
    Strictness::Minimal,
    Strictness::Basic,
    Strictness::Standard,
    Strictness::Strict,
];

const RUNGS: [Severity; 5] = [
    Severity::Info,
    Severity::Minor,
    Severity::Major,
    Severity::Critical,
    Severity::Blocker,
];

#[test]
fn preset_levels_never_loosen_as_strictness_rises() {
    for info in CATALOG {
        for index in 1..PROFILES.len() {
            let lower = (info.preset)(PROFILES[index - 1]);
            let higher = (info.preset)(PROFILES[index]);
            assert!(
                higher.level >= lower.level,
                "{}: level loosens from {:?} to {:?} at {:?}",
                info.id,
                lower.level,
                higher.level,
                PROFILES[index]
            );
        }
    }
}

#[test]
fn off_sits_below_every_rung() {
    assert!(
        RUNGS.iter().all(|rung| Level::Off < Level::At(*rung)),
        "Off must order below every rung"
    );
    for window in RUNGS.windows(2) {
        assert!(window[0] < window[1], "ladder must order {window:?}");
    }
}

#[test]
fn preset_numbers_never_loosen_as_strictness_rises() {
    for info in CATALOG {
        let tables: Vec<toml::Table> = PROFILES
            .iter()
            .map(|strictness| (info.preset)(*strictness).options)
            .collect();
        assert_stable_numeric_keys(info.id, &tables);
        for key in numeric_keys(&tables) {
            let loosening = direction(key);
            let mut previous = option_value(info.id, key, &tables[0]);
            for (table, stricter) in tables[1..].iter().zip(PROFILES[1..].iter()) {
                let value = option_value(info.id, key, table);
                assert!(
                    !loosens(loosening, compare(info.id, key, previous, value)),
                    "{}: `{key}` loosens from {previous} to {value} at {stricter:?}",
                    info.id
                );
                previous = value;
            }
        }
        assert_ladder_cut_points_ordered(info.id, &tables);
    }
}

#[test]
fn every_graded_rule_sets_all_four_cut_points() {
    for info in CATALOG {
        let tables: Vec<toml::Table> = PROFILES
            .iter()
            .map(|strictness| (info.preset)(*strictness).options)
            .collect();
        let graded = tables.iter().any(|table| table.contains_key("minor_at"));
        if !graded {
            continue;
        }
        for (table, strictness) in tables.iter().zip(PROFILES.iter()) {
            for key in ["minor_at", "major_at", "critical_at", "blocker_at"] {
                assert!(
                    table.get(key).is_some(),
                    "{}: graded rule is missing `{key}` at {strictness:?}",
                    info.id
                );
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Loosening {
    Rising,
    Falling,
}

fn direction(key: &str) -> Loosening {
    match key {
        "min_len" => Loosening::Rising,
        "minor_at" | "major_at" | "critical_at" | "blocker_at" | "window" | "min_repeats"
        | "min_files" | "min_fields" | "min_similarity" | "min_span" => Loosening::Falling,
        other => panic!(
            "numeric preset option `{other}` has no direction in preset_monotonicity::direction; \
             declare whether a larger value is stricter before shipping it"
        ),
    }
}

fn loosens(loosening: Loosening, order: Ordering) -> bool {
    match loosening {
        Loosening::Rising => order == Ordering::Greater,
        Loosening::Falling => order == Ordering::Less,
    }
}

fn compare(rule: &str, key: &str, previous: &toml::Value, value: &toml::Value) -> Ordering {
    match (previous, value) {
        (toml::Value::Integer(a), toml::Value::Integer(b)) => a.cmp(b),
        (toml::Value::Float(a), toml::Value::Float(b)) => a
            .partial_cmp(b)
            .unwrap_or_else(|| panic!("{rule}: option `{key}` value {a} is not comparable")),
        _ => {
            panic!("{rule}: option `{key}` changes type between profiles: {previous} then {value}")
        }
    }
}

fn option_value<'a>(rule: &'a str, key: &'a str, table: &'a toml::Table) -> &'a toml::Value {
    table
        .get(key)
        .unwrap_or_else(|| panic!("{rule}: numeric option `{key}` is missing from a profile"))
}

fn numeric_keys(tables: &[toml::Table]) -> BTreeSet<&str> {
    let mut keys = BTreeSet::new();
    for table in tables {
        for (key, value) in table {
            if value.is_integer() || value.is_float() {
                keys.insert(key.as_str());
            }
        }
    }
    keys
}

fn assert_stable_numeric_keys(rule: &str, tables: &[toml::Table]) {
    for key in numeric_keys(tables) {
        for table in tables {
            assert!(
                table.get(key).is_some(),
                "{rule}: numeric option `{key}` exists in one profile but not another"
            );
        }
    }
}

fn assert_ladder_cut_points_ordered(rule: &str, tables: &[toml::Table]) {
    for (table, strictness) in tables.iter().zip(PROFILES.iter()) {
        let cuts: Vec<(&str, i64)> = ["minor_at", "major_at", "critical_at", "blocker_at"]
            .into_iter()
            .filter_map(|key| {
                table
                    .get(key)
                    .and_then(|value| value.as_integer())
                    .map(|cut| (key, cut))
            })
            .collect();
        if cuts.is_empty() {
            continue;
        }
        assert_eq!(
            cuts.len(),
            4,
            "{rule}: graded cut points must come as a full ladder set at {strictness:?}"
        );
        for pair in cuts.windows(2) {
            assert!(
                pair[0].1 <= pair[1].1,
                "{}: `{}` {} exceeds `{}` {} at {strictness:?}",
                rule,
                pair[0].0,
                pair[0].1,
                pair[1].0,
                pair[1].1
            );
        }
    }
}
