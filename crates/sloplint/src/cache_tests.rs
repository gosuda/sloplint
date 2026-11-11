use super::{Config, config_key};
use sloplint_core::{Confidence, FailOn, config::Mode};

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
#[test]
fn stale_format_and_foreign_config_shards_are_pruned() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("cache");
    let shards = dir.join(super::SHARD_DIR);
    super::dirs::create_private(&shards).unwrap();
    let key = [7u8; 32];
    let cache = super::Cache {
        dir: dir.clone(),
        key,
    };
    let stale_byte = 0x11u8;
    let stale_payload = postcard::to_allocvec(&super::ShardWrite {
        version: super::VERSION + 1,
        config_key: &key,
        files: &[],
    })
    .unwrap();
    let mut stale_file = Vec::with_capacity(32 + stale_payload.len());
    stale_file.extend_from_slice(blake3::hash(&stale_payload).as_bytes());
    stale_file.extend_from_slice(&stale_payload);
    std::fs::write(shards.join(super::shard_file(stale_byte)), &stale_file).unwrap();
    let foreign_byte = 0x22u8;
    let foreign_key = [9u8; 32];
    let foreign_payload = postcard::to_allocvec(&super::ShardWrite {
        version: super::VERSION,
        config_key: &foreign_key,
        files: &[],
    })
    .unwrap();
    let mut foreign_file = Vec::with_capacity(32 + foreign_payload.len());
    foreign_file.extend_from_slice(blake3::hash(&foreign_payload).as_bytes());
    foreign_file.extend_from_slice(&foreign_payload);
    std::fs::write(shards.join(super::shard_file(foreign_byte)), &foreign_file).unwrap();
    let (stale_decoded, stale_observed) = cache.decode_shard(&shards, stale_byte);
    assert!(
        stale_decoded.is_none(),
        "a version+1 shard must be rejected"
    );
    assert!(
        stale_observed.is_some(),
        "a rejected shard must report an observation for pruning"
    );
    let (foreign_decoded, foreign_observed) = cache.decode_shard(&shards, foreign_byte);
    assert!(
        foreign_decoded.is_none(),
        "a foreign config_key shard must be rejected"
    );
    assert!(
        foreign_observed.is_some(),
        "a rejected shard must report an observation for pruning"
    );
    let loaded = cache.load();
    assert_eq!(
        loaded.entry_count(),
        0,
        "rejected shards must load no entries"
    );
    assert!(
        !shards.join(super::shard_file(stale_byte)).exists(),
        "a stale-format shard must be pruned"
    );
    assert!(
        !shards.join(super::shard_file(foreign_byte)).exists(),
        "a foreign-config shard must be pruned"
    );
}
#[cfg(unix)]
#[test]
fn probe_file_matches_owned_dir_owner() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("ns");
    std::fs::create_dir(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    let dir_uid = std::fs::symlink_metadata(&target).unwrap().uid();
    let probe = super::dirs::probe_uid(&target).unwrap();
    assert_eq!(dir_uid, probe, "a probe must share its owned dir uid");
    assert!(
        super::dirs::vet_existing(&target).is_ok(),
        "an owned 0755 dir must vet clean"
    );
}
#[cfg(unix)]
#[test]
fn mismatched_owners_are_refused() {
    assert!(super::dirs::owner_ok(1000, 1000), "matching uids must pass");
    assert!(
        !super::dirs::owner_ok(1000, 1001),
        "mismatched uids must be refused"
    );
}
