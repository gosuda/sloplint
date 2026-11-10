use sloplint_core::Config;
use sloplint_core::config::parse_config;

const EXPECTED: &[&str] = &[
    "**/conftest.py",
    "**/testutil/**",
    "**/testutils/**",
    "**/test_util/**",
    "**/test_utils/**",
    "**/test_helpers/**",
    "**/test_support/**",
    "**/testing/**",
    "**/tests/common/**",
    "**/tests/support/**",
    "**/spec/support/**",
    "**/__mocks__/**",
];

#[test]
fn support_globs_default_to_contract_list() {
    let config = Config::default();
    assert_eq!(config.project.support_globs.len(), EXPECTED.len());
    for pattern in EXPECTED {
        assert!(
            config
                .project
                .support_globs
                .iter()
                .any(|glob| glob == pattern),
            "missing default support glob {pattern}"
        );
    }
    assert!(
        config
            .project
            .support_globs
            .iter()
            .all(|glob| !glob.contains("testdata")),
        "testdata stays data, never support"
    );
}

#[test]
fn support_globs_parse_override_and_reject_bad_patterns() {
    let config = parse_config("[project]\nsupport_globs = [\"**/helpers/**\"]\n", "test")
        .expect("support_globs parse");
    assert_eq!(
        config.project.support_globs,
        vec!["**/helpers/**".to_string()]
    );
    assert!(
        parse_config("[project]\nsupport_globs = [\"[\"]\n", "test").is_err(),
        "an unclosed character class is rejected"
    );
}

#[test]
fn support_globs_round_trip_through_toml() {
    let config = parse_config("[project]\nsupport_globs = [\"**/helpers/**\"]\n", "test")
        .expect("support_globs parse");
    let serialized = toml::to_string(&config.project).expect("project serialize");
    let reparsed =
        parse_config(&format!("[project]\n{serialized}"), "test").expect("project reparse");
    assert_eq!(reparsed.project.support_globs, config.project.support_globs);
}
