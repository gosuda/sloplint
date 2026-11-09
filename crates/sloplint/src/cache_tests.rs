use std::io;

use super::{Config, config_key, hash_regular_into};
use crate::fileio::{open_regular, read_bounded};

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

fn materialized_config_key(
    config: &Config,
    custom_sources: &[(String, String)],
) -> io::Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    let exe = std::env::current_exe()?;
    let (file, len) = open_regular(&exe)?;
    hasher.update(&read_bounded(file, len)?);
    hasher.update(&sonic_rs::to_vec(config).map_err(io::Error::other)?);
    for (rel, text) in custom_sources {
        hasher.update(rel.as_bytes());
        hasher.update(&[0u8]);
        hasher.update(text.as_bytes());
    }
    Ok(*hasher.finalize().as_bytes())
}

#[test]
fn config_key_matches_the_materialized_original() {
    let config = Config::default();
    let custom_sources = vec![("rel".to_string(), "text".to_string())];
    let streamed = config_key(&config, &custom_sources).unwrap();
    let materialized = materialized_config_key(&config, &custom_sources).unwrap();
    assert_eq!(
        streamed, materialized,
        "config_key must equal the pre-change materialized key on this executable"
    );
}
