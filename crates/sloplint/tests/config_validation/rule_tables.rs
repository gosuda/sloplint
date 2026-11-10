//! Rule-table validation: unknown ids, thresholds, options, levels, and filters.

use std::collections::BTreeSet;

use sloplint_core::config::{Strictness, parse_config};
use sloplint_core::{CATALOG, ConfigError, Level, RuleSet, Severity};

use super::{
    CUT_CASES, FLOORS, LEVEL_LINES, PROFILES, SPAN, WRONG_TYPES, accepted, assert_unknown_rule,
    config_error, custom_rule, level_profiles, nesting_severities, preset_int, project, rejected,
    rule_option, run_at, selector, table,
};

#[test]
fn audit_config_unknown_rule_id_is_rejected_at_every_profile_and_level() {
    let ids = "typo/x complexity/cognitive-dept complexity/* custom/ghost Grep/x";
    for id in ids.split(' ') {
        for level in LEVEL_LINES {
            let (_tmp, root) = project(&table(id, level, ""), &[]);
            for strictness in PROFILES {
                let context = format!("{id} {level:?} {strictness:?}");
                assert_unknown_rule(&root, id, strictness, &context);
            }
        }
    }
}

#[test]
fn audit_config_threshold_boundaries_hold_for_every_rule_profile_and_level() {
    let rules: BTreeSet<&str> = CATALOG
        .iter()
        .filter(|info| {
            let options = (info.preset)(Strictness::Strict).options;
            options.contains_key("minor_at")
                && options.contains_key("major_at")
                && options.contains_key("critical_at")
                && options.contains_key("blocker_at")
        })
        .map(|info| info.id)
        .collect();
    assert!(
        rules.len() >= 7 && rules.contains(SPAN),
        "threshold rules vanished: {rules:?}"
    );
    for rule in rules {
        for (minor_at, major_at, critical_at, blocker_at, valid) in CUT_CASES {
            let body = format!(
                "minor_at = {minor_at}\nmajor_at = {major_at}\ncritical_at = {critical_at}\nblocker_at = {blocker_at}\n"
            );
            for (level, strictness) in level_profiles(&PROFILES) {
                let src = table(rule, level, &body);
                let context = format!(
                    "{rule} {minor_at}/{major_at}/{critical_at}/{blocker_at} {level:?} {strictness:?}"
                );
                if valid {
                    accepted(&src, strictness, &context);
                    continue;
                }
                let error = rejected(&src, strictness, &context);
                rule_option(&error, rule, None, &context);
            }
        }
    }
}

#[test]
fn audit_config_partial_threshold_overrides_validate_against_the_preset() {
    for (level, strictness) in level_profiles(&PROFILES) {
        let minor = preset_int(SPAN, strictness, "minor_at");
        let major = preset_int(SPAN, strictness, "major_at");
        let critical = preset_int(SPAN, strictness, "critical_at");
        let blocker = preset_int(SPAN, strictness, "blocker_at");
        let context =
            format!("preset {minor}/{major}/{critical}/{blocker} {level:?} {strictness:?}");
        let high = table(SPAN, level, &format!("minor_at = {}\n", major + 1));
        let error = rejected(&high, strictness, &context);
        rule_option(&error, SPAN, None, &context);
        let low = table(SPAN, level, &format!("major_at = {}\n", minor - 1));
        let error = rejected(&low, strictness, &context);
        rule_option(&error, SPAN, None, &context);
        let descending = table(
            SPAN,
            level,
            &format!("critical_at = {minor}\nmajor_at = {critical}\n"),
        );
        let error = rejected(&descending, strictness, &context);
        rule_option(&error, SPAN, None, &context);
        let zero = table(SPAN, level, "blocker_at = 0\n");
        let error = rejected(&zero, strictness, &context);
        rule_option(&error, SPAN, None, &context);
        let at_major = table(SPAN, level, &format!("minor_at = {major}\n"));
        accepted(&at_major, strictness, &context);
        let at_minor = table(SPAN, level, &format!("major_at = {minor}\n"));
        accepted(&at_minor, strictness, &context);
    }
}

#[test]
fn audit_config_numeric_floors_are_enforced_for_disabled_rules() {
    for (rule, key, literal, is_rejected) in FLOORS {
        for (level, strictness) in level_profiles(&PROFILES) {
            let src = table(rule, level, &format!("{key} = {literal}\n"));
            let context = format!("{rule} {key}={literal} {level:?} {strictness:?}");
            if !is_rejected {
                accepted(&src, strictness, &context);
                continue;
            }
            rule_option(&rejected(&src, strictness, &context), rule, None, &context);
        }
    }
}

#[test]
fn audit_config_unknown_option_is_rejected_for_every_rule_profile_and_level() {
    for info in CATALOG {
        for (level, strictness) in level_profiles(&PROFILES) {
            let src = table(info.id, level, "not_an_option = 1\n");
            let context = format!("{} {level:?} {strictness:?}", info.id);
            let error = rejected(&src, strictness, &context);
            rule_option(&error, info.id, Some("not_an_option"), &context);
        }
    }
    let meta = table("meta/parse-error", "level = \"off\"\n", "bogus = 1\n");
    let error = rejected(&meta, Strictness::Strict, "meta rule option");
    rule_option(
        &error,
        "meta/parse-error",
        Some("bogus"),
        "meta rule option",
    );
    let typo = table(SPAN, "level = \"off\"\n", "minor_att = 2\n");
    let error = rejected(&typo, Strictness::Minimal, "misspelled key");
    rule_option(&error, SPAN, Some("minor_att"), "misspelled key");
}

#[test]
fn audit_config_wrong_type_option_values_name_the_key() {
    for (rule, key, literal) in WRONG_TYPES {
        for (level, strictness) in level_profiles(&[Strictness::Minimal, Strictness::Strict]) {
            let src = table(rule, level, &format!("{key} = {literal}\n"));
            let context = format!("{rule} {key}={literal} {level:?} {strictness:?}");
            let error = rejected(&src, strictness, &context);
            rule_option(&error, rule, Some(key), &context);
        }
    }
}

#[test]
fn audit_config_invalid_level_values_name_the_value() {
    for literal in ["\"fatal\"", "\"warn\"", "\"deny\"", "\"Warn\"", "3", "true"] {
        let src = format!("[rules.\"{SPAN}\"]\nlevel = {literal}\n");
        let error = parse_config(&src, "sloplint.toml").err();
        let error = error.unwrap_or_else(|| panic!("level = {literal} was accepted"));
        rule_option(&error, SPAN, Some(literal.trim_matches('"')), literal);
    }
}

#[test]
fn audit_config_bad_rule_tables_fail_through_run_with_exit_two() {
    let cases = [
        (
            SPAN,
            "minor_at = 9\nmajor_at = 1\ncritical_at = 2\nblocker_at = 3\n",
        ),
        (
            SPAN,
            "minor_at = 0\nmajor_at = 4\ncritical_at = 5\nblocker_at = 6\n",
        ),
        (SPAN, "bogus = 1\n"),
        ("meta/parse-error", "bogus = 1\n"),
        ("grep/homonym-definitions", "min_files = 1\n"),
        ("agent/colocated-tests", "exclude = [\"[\"]\n"),
    ];
    for (rule, body) in cases {
        for level in ["", "level = \"off\"\n"] {
            let (_tmp, root) = project(&table(rule, level, body), &[]);
            for strictness in [Strictness::Minimal, Strictness::Strict] {
                let context = format!("{rule} {body:?} {level:?} {strictness:?}");
                let error = config_error(run_at(&root, strictness), &context);
                rule_option(&error, rule, None, &context);
            }
        }
    }
}

#[test]
fn audit_config_rule_filter_does_not_hide_invalid_rule_tables() {
    let filter = selector("complexity/cognitive-depth");
    let enabled = table("complexity/cognitive-depth", "level = \"minor\"\n", "");
    let bad_src = format!(
        "{enabled}{}",
        table("grep/homonym-definitions", "", "min_files = 1\n")
    );
    let mut bad = parse_config(&bad_src, "sloplint.toml").unwrap_or_else(|e| panic!("{e}"));
    let mut good = parse_config(&enabled, "sloplint.toml").unwrap_or_else(|e| panic!("{e}"));
    for strictness in PROFILES {
        bad.project.strictness = strictness;
        good.project.strictness = strictness;
        let context = format!("{strictness:?}");
        let Err(error) = RuleSet::new(&bad, &filter) else {
            panic!("{context}: a filtered-out rule hid its invalid table");
        };
        rule_option(&error, "grep/homonym-definitions", None, &context);
        let Ok(set) = RuleSet::new(&good, &filter) else {
            panic!("{context}: valid configuration rejected under a filter");
        };
        assert!(
            set.is_active("complexity/cognitive-depth"),
            "{context}: selected rule off"
        );
        if strictness == Strictness::Strict {
            assert!(
                !set.selects("complexity/nesting-depth"),
                "{context}: filtered rule selected"
            );
        } else {
            assert!(
                !set.is_active("complexity/nesting-depth"),
                "{context}: filtered rule on"
            );
        }
    }
}

#[test]
fn audit_config_profiles_activate_exactly_their_preset_rules() {
    for strictness in PROFILES {
        let set = accepted("", strictness, &format!("{strictness:?}"));
        for info in CATALOG {
            let expected = (info.preset)(strictness).level != Level::Off;
            assert_eq!(
                set.is_active(info.id),
                expected,
                "{} under {strictness:?}",
                info.id
            );
        }
    }
}

#[test]
fn audit_config_level_caps_and_disables_still_apply_after_validation() {
    let rule = "complexity/nesting-depth";
    let enabled = table(rule, "level = \"major\"\n", "");
    assert_eq!(
        nesting_severities(&enabled),
        [Severity::Major],
        "enabled reports major for depth 8"
    );
    let minor = table(rule, "level = \"minor\"\n", "");
    assert_eq!(
        nesting_severities(&minor),
        [Severity::Minor],
        "level minor caps major"
    );
    let major = table(rule, "level = \"major\"\n", "");
    assert_eq!(
        nesting_severities(&major),
        [Severity::Major],
        "level major keeps major"
    );
    let off = table(rule, "level = \"off\"\n", "");
    assert!(
        nesting_severities(&off).is_empty(),
        "level off reports nothing"
    );
    let raised = table(
        rule,
        "level = \"major\"\n",
        "minor_at = 9\nmajor_at = 9\ncritical_at = 9\nblocker_at = 9\n",
    );
    assert!(
        nesting_severities(&raised).is_empty(),
        "thresholds above depth 8 pass"
    );
    let both = format!(
        "{off}{}",
        table(SPAN, "level = \"off\"\n", "minor_at = 3\n")
    );
    assert!(
        nesting_severities(&both).is_empty(),
        "valid options elsewhere change nothing"
    );
    let set = accepted(
        &both,
        Strictness::Standard,
        "off with valid override stays buildable",
    );
    assert!(!set.is_active(rule), "off nesting-depth must stay inactive");
    assert!(
        !set.is_active(SPAN),
        "off mutable-span with minor_at override must stay inactive"
    );
    let inactive = table(
        rule,
        "",
        "minor_at = 9\nmajor_at = 9\ncritical_at = 9\nblocker_at = 9\n",
    );
    let mut filtered_config = parse_config(&inactive, "sloplint.toml")
        .unwrap_or_else(|e| panic!("valid override rejected: {e}"));
    filtered_config.project.strictness = Strictness::Standard;
    let filtered = RuleSet::new(&filtered_config, &selector(SPAN))
        .unwrap_or_else(|e| panic!("filtered valid override rejected: {e}"));
    assert!(
        !filtered.is_active(rule),
        "filter-excluded nesting-depth with valid override must stay inactive"
    );
}

#[test]
fn audit_config_rejects_legacy_severity_words() {
    for (src, token) in [
        (table(SPAN, "level = \"warn\"\n", ""), "warn"),
        (table(SPAN, "level = \"deny\"\n", ""), "deny"),
        (table(SPAN, "", "warn_at = 2\n"), "warn_at"),
        (table(SPAN, "", "deny_at = 2\n"), "deny_at"),
    ] {
        let error = rejected(&src, Strictness::Standard, &src);
        rule_option(&error, SPAN, Some(token), &src);
    }
    for (src, token) in [
        (custom_rule(&[("severity", "warn")]), "warn"),
        (custom_rule(&[("severity", "deny")]), "deny"),
    ] {
        let error = rejected(&src, Strictness::Standard, &src);
        let ConfigError::Toml { message, .. } = &error else {
            panic!("{token}: expected a TOML error, got: {error}");
        };
        assert!(
            message.contains(token),
            "{token}: `{message}` lacks `{token}`"
        );
        assert!(
            error.to_string().contains(token),
            "{token}: display omits `{token}`: {error}"
        );
    }
}
