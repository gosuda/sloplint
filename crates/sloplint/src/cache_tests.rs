use super::{Config, config_key, hash_regular_into};
use sloplint_core::{Confidence, FailOn, config::Mode};

fn pseudo_random(len: usize) -> Vec<u8> {
    let mut state = 0x2545f4914f6cdd1du64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            u8::try_from(state & 0xff).unwrap()
        })
        .collect()
}

#[test]
fn streamed_file_hash_matches_a_one_shot_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blob");
    std::fs::write(&path, pseudo_random(5 * 1024 * 1024 + 7)).unwrap();
    let mut streamed = blake3::Hasher::new();
    hash_regular_into(&mut streamed, &path).unwrap();
    let one_shot = blake3::hash(&std::fs::read(&path).unwrap());
    assert_eq!(
        streamed.finalize(),
        one_shot,
        "streamed digest must equal the one-shot read digest"
    );
}

#[test]
fn gate_only_settings_keep_the_cache_key() {
    let base = Config::default();
    let mut gated = Config::default();
    gated.project.fail_on = FailOn::Never;
    gated.project.min_confidence = Confidence::High;
    gated.project.mode = Mode::Ratchet;
    gated.project.baseline = String::from("other-baseline.json");
    let custom_sources = vec![("rel".to_string(), "text".to_string())];
    let uncut = config_key(&base, &custom_sources).unwrap();
    let regated = config_key(&gated, &custom_sources).unwrap();
    assert_eq!(
        uncut, regated,
        "fail_on, min_confidence, mode, and baseline must not invalidate the cache"
    );
}

#[test]
fn rule_level_changes_the_cache_key() {
    let base = Config::default();
    let mut changed = Config::default();
    let mut table = toml::Table::new();
    table.insert("level".to_string(), toml::Value::String("off".to_string()));
    changed.rules.insert("grep/generic-name".to_string(), table);
    let custom_sources = vec![("rel".to_string(), "text".to_string())];
    let uncut = config_key(&base, &custom_sources).unwrap();
    let releveled = config_key(&changed, &custom_sources).unwrap();
    assert_ne!(
        uncut, releveled,
        "a rule level changes analysis and must invalidate the cache"
    );
}

#[test]
fn support_globs_change_the_cache_key() {
    let base = Config::default();
    let mut changed = Config::default();
    changed.project.support_globs = vec!["**/helpers/**".to_string()];
    let custom_sources = vec![("rel".to_string(), "text".to_string())];
    let uncut = config_key(&base, &custom_sources).unwrap();
    let releveled = config_key(&changed, &custom_sources).unwrap();
    assert_ne!(
        uncut, releveled,
        "support globs change file roles and must invalidate the cache"
    );
}
