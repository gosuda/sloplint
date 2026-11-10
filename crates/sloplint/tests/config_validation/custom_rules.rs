//! Custom-rule validation: ids, fields, languages, queries, levels, and files.

use sloplint_core::config::Strictness;

use super::{
    PROJECT_TABLE, RULES_TABLE, accepted_run, config_error, custom_error, custom_rule, file_error,
    files_config, project, rule_option, run_at, table,
};

#[test]
fn audit_custom_rule_ids_must_be_well_formed() {
    let bad_ids = "custom/Upper custom/ custom Custom/x custom/a--b custom/-a custom/a_b \
                   custom/a/b grep/generic-name";
    for bad in bad_ids.split(' ').chain([""]) {
        custom_error(&custom_rule(&[("id", bad)]), &[], bad, None, bad);
    }
    for good in ["custom/a", "custom/a-b-c", "custom/a1-2b", "custom/0"] {
        accepted_run(
            &custom_rule(&[("id", good)]),
            &[],
            Strictness::Standard,
            good,
        );
    }
}

#[test]
fn audit_custom_rule_ids_must_be_unique_across_config_and_files() {
    let id = [("id", "custom/dup")];
    let inline = format!("{}{}", custom_rule(&id), custom_rule(&id));
    custom_error(&inline, &[], "custom/dup", None, "inline twice");
    let dup = custom_rule(&id);
    let mixed = format!("{}{dup}", files_config(&["extra.toml"]));
    let extra = [("extra.toml", dup.as_str())];
    custom_error(&mixed, &extra, "custom/dup", None, "inline and file");
    let pair = [("a.toml", dup.as_str()), ("b.toml", dup.as_str())];
    let config = files_config(&["a.toml", "b.toml"]);
    custom_error(&config, &pair, "custom/dup", None, "two files");
}

#[test]
fn audit_custom_rule_fields_must_not_be_blank() {
    for field in ["message", "why", "fix", "query"] {
        for blank in ["", " ", "\t", " \n "] {
            let src = custom_rule(&[(field, blank)]);
            custom_error(
                &src,
                &[],
                "custom/ok",
                Some(field),
                &format!("{field}={blank:?}"),
            );
        }
    }
    let blank = custom_rule(&[("message", " ")]);
    let file = [("rules.toml", blank.as_str())];
    let config = files_config(&["rules.toml"]);
    custom_error(
        &config,
        &file,
        "custom/ok",
        Some("message"),
        "blank message in a file",
    );
}

#[test]
fn audit_custom_rule_language_and_globs_must_be_valid() {
    for language in ["cobol", "Rust", "rust ", ""] {
        let src = custom_rule(&[("language", language)]);
        let token = (!language.trim().is_empty()).then(|| language.trim());
        custom_error(&src, &[], "custom/ok", token, language);
    }
    for (key, glob) in [("include", "["), ("exclude", "{a")] {
        let src = format!("{}{key} = [{glob:?}]\n", custom_rule(&[]));
        custom_error(&src, &[], "custom/ok", Some(glob), key);
    }
}

#[test]
fn audit_custom_rule_queries_must_compile_with_a_match_capture() {
    let cases = [
        (
            "unbalanced",
            custom_rule(&[("query", "(call_expression @match")]),
        ),
        (
            "unknown node",
            custom_rule(&[("query", "(no_such_node) @match")]),
        ),
        ("wrong grammar", custom_rule(&[("language", "python")])),
        (
            "no capture",
            custom_rule(&[("query", "(call_expression) @call")]),
        ),
        ("placeholder", custom_rule(&[("message", "found {nope}")])),
    ];
    for (context, src) in cases {
        custom_error(&src, &[], "custom/ok", None, context);
        let disabled = format!("{src}{}", table("custom/ok", "level = \"off\"\n", ""));
        custom_error(
            &disabled,
            &[],
            "custom/ok",
            None,
            &format!("{context}, level off"),
        );
    }
    let callee = "(call_expression function: (identifier) @callee) @match";
    for message in ["calls {callee}", "literal {{braces}}"] {
        let src = custom_rule(&[("message", message), ("query", callee)]);
        accepted_run(&src, &[], Strictness::Standard, message);
    }
}
#[test]
fn audit_custom_rule_tables_accept_only_a_valid_level() {
    let spec = custom_rule(&[]);
    let cases = [
        ("minor_at = 1\n", None),
        ("bogus = true\n", None),
        ("level = \"fatal\"\n", Some("fatal")),
    ];
    for (body, token) in cases {
        let (_tmp, root) = project(&format!("{spec}{}", table("custom/ok", "", body)), &[]);
        let error = config_error(run_at(&root, Strictness::Standard), body);
        rule_option(&error, "custom/ok", token, body);
    }
}

#[test]
fn audit_custom_rule_files_reject_bad_content_naming_the_file() {
    let singular = custom_rule(&[]).replace("[[custom_rules]]", "[[custom_rule]]");
    let missing_fix = custom_rule(&[]).replace("fix = \"test fix\"\n", "");
    let typo_key = format!("{}severty = \"minor\"\n", custom_rule(&[]));
    let fatal = custom_rule(&[("severity", "fatal")]);
    let cases = [
        (
            "root key",
            "rules/team.toml",
            PROJECT_TABLE,
            Some("project"),
        ),
        ("rule table", "rules.toml", RULES_TABLE, Some("rules")),
        (
            "singular",
            "rules.toml",
            singular.as_str(),
            Some("custom_rule"),
        ),
        (
            "missing field",
            "rules.toml",
            missing_fix.as_str(),
            Some("fix"),
        ),
        ("spec key", "rules.toml", typo_key.as_str(), Some("severty")),
        ("severity", "rules.toml", fatal.as_str(), Some("fatal")),
        ("wrong type", "rules.toml", "custom_rules = \"x\"\n", None),
    ];
    for (context, path, text, token) in cases {
        file_error(&[(path, text)], path, token, context);
    }
    let good = custom_rule(&[]);
    let pair = [("a.toml", good.as_str()), ("b.toml", PROJECT_TABLE)];
    file_error(&pair, "b.toml", Some("project"), "second file is bad");
    let valid = [("empty.toml", ""), ("ok.toml", good.as_str())];
    let config = files_config(&["empty.toml", "ok.toml"]);
    accepted_run(
        &config,
        &valid,
        Strictness::Standard,
        "empty and valid files",
    );
}
