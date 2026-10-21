//! The sharded cache format: partial stores, corrupt and orphan shards, and pruning.

use std::path::{Path, PathBuf};

use super::{
    CHECK, SAMPLE, UNCACHED, cache_files, isolated, project, put, succeeded_with_finding, temp,
    text,
};

fn file_bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path)
        .unwrap_or_else(|e| panic!("cannot read the cache shard {}: {e}", path.display()))
}

/// The one shard file whose bytes name `rel`; entries are keyed by rel path.
fn shard_holding(files: &[PathBuf], rel: &str) -> PathBuf {
    let mut found = files.iter().filter(|path| {
        file_bytes(path)
            .windows(rel.len())
            .any(|w| w == rel.as_bytes())
    });
    let shard = found
        .next()
        .unwrap_or_else(|| panic!("no shard names {rel}"))
        .clone();
    assert!(found.next().is_none(), "two shards name {rel}");
    shard
}

/// Rewrites every stored occurrence of `from` in `file` with the same-length `to`.
fn plant_same_length(file: &Path, from: &str, to: &str) {
    assert_eq!(from.len(), to.len(), "a plant must keep the stored length");
    let mut bytes = file_bytes(file);
    let mut count = 0;
    for at in 0..=bytes.len().saturating_sub(from.len()) {
        if bytes[at..at + from.len()] == *from.as_bytes() {
            bytes[at..at + from.len()].copy_from_slice(to.as_bytes());
            count += 1;
        }
    }
    assert!(count > 0, "the cache must hold {from:?}");
    std::fs::write(file, &bytes)
        .unwrap_or_else(|e| panic!("cannot write the planted cache shard: {e}"));
}

#[test]
fn a_partial_store_rewrites_only_the_touched_shards() {
    let (_tmp, root) = project();
    put(&root, "a.rs", SAMPLE);
    put(&root, "b.rs", SAMPLE);
    let (_home_guard, home) = temp();

    let cold = isolated(&root, &CHECK, &home);
    succeeded_with_finding(&cold);
    let second = isolated(&root, &CHECK, &home);
    assert_eq!(second.stdout, cold.stdout, "the second run must be warm");
    assert!(second.stderr.is_empty(), "stderr: {}", text(&second.stderr));
    let before = cache_files(&home);
    assert_eq!(before.len(), 3, "one shard per file: {before:?}");
    let bytes_before: std::collections::HashMap<PathBuf, Vec<u8>> = before
        .iter()
        .map(|path| (path.clone(), file_bytes(path)))
        .collect();

    put(&root, "b.rs", &SAMPLE.replace("handle", "settle"));
    let edited = isolated(&root, &CHECK, &home);
    assert!(edited.status.success(), "stderr: {}", text(&edited.stderr));
    let uncached = isolated(&root, &UNCACHED, &home);
    assert_eq!(
        edited.stdout, uncached.stdout,
        "the edited run must equal a cold run"
    );

    let after = cache_files(&home);
    assert_eq!(after.len(), 3, "no shard gained or lost: {after:?}");
    let changed: Vec<PathBuf> = after
        .iter()
        .filter(|path| {
            let old = bytes_before
                .get(*path)
                .unwrap_or_else(|| panic!("{} is a new shard", path.display()));
            &file_bytes(path.as_path()) != old
        })
        .cloned()
        .collect();
    assert_eq!(changed.len(), 1, "exactly one shard rewrites: {after:?}");
    assert_eq!(
        changed[0].file_name().and_then(|name| name.to_str()),
        Some("shard-81.bin"),
        "only the b.rs shard rewrites"
    );

    let warm = isolated(&root, &CHECK, &home);
    assert_eq!(
        warm.stdout, edited.stdout,
        "the mixed old and new shards must all load"
    );
    assert!(warm.stderr.is_empty(), "stderr: {}", text(&warm.stderr));
}

#[test]
fn a_corrupt_shard_is_ignored_while_siblings_load() {
    let (_tmp, root) = project();
    put(&root, "a.rs", SAMPLE);
    put(&root, "b.rs", SAMPLE);
    let (_home_guard, home) = temp();

    let cold = isolated(&root, &CHECK, &home);
    succeeded_with_finding(&cold);
    let files = cache_files(&home);
    assert_eq!(files.len(), 3, "one shard per file: {files:?}");
    let corrupt = shard_holding(&files, "a.rs");
    let sibling = shard_holding(&files, "b.rs");
    assert_ne!(corrupt, sibling, "a.rs and b.rs must shard apart");

    std::fs::write(&corrupt, b"not a cache").unwrap_or_else(|e| panic!("corrupt: {e}"));
    plant_same_length(&sibling, "unrelated code", "UNRELATED CODE");
    let sibling_bytes = file_bytes(&sibling);

    let out = isolated(&root, &CHECK, &home);
    assert!(out.status.success(), "stderr: {}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("UNRELATED CODE"),
        "the sibling shard was not reused: {stdout}"
    );
    assert!(
        stdout.contains("unrelated code"),
        "the corrupt shard was not recomputed: {stdout}"
    );
    assert_eq!(
        file_bytes(&sibling),
        sibling_bytes,
        "the sibling shard must not rewrite"
    );
    assert!(out.stderr.is_empty(), "stderr: {}", text(&out.stderr));

    let warm = isolated(&root, &CHECK, &home);
    assert_eq!(
        warm.stdout, out.stdout,
        "the healed shard must serve the same findings"
    );
    assert!(warm.stderr.is_empty(), "stderr: {}", text(&warm.stderr));
}

#[test]
fn an_orphan_shard_that_loads_nothing_is_pruned_by_the_next_store() {
    let (_tmp, root) = project();
    put(&root, "a.rs", SAMPLE);
    let (_home_guard, home) = temp();

    let cold = isolated(&root, &CHECK, &home);
    succeeded_with_finding(&cold);
    let files = cache_files(&home);
    assert_eq!(files.len(), 2, "one shard per file: {files:?}");
    let shards = files[0]
        .parent()
        .unwrap_or_else(|| panic!("{} has no directory", files[0].display()))
        .to_path_buf();
    let orphan = (0..=255u8)
        .map(|byte| shards.join(format!("shard-{byte:02x}.bin")))
        .find(|path| !files.contains(path))
        .unwrap_or_else(|| panic!("every shard name is taken"));
    std::fs::write(&orphan, b"not a cache").unwrap_or_else(|e| panic!("plant orphan: {e}"));

    put(&root, "a.rs", &SAMPLE.replace("handle", "settle"));
    let edited = isolated(&root, &CHECK, &home);
    assert!(edited.status.success(), "stderr: {}", text(&edited.stderr));
    assert!(edited.stderr.is_empty(), "stderr: {}", text(&edited.stderr));
    assert!(
        !orphan.exists(),
        "a shard with no entries must be removed by the store: {}",
        orphan.display()
    );
    assert_eq!(cache_files(&home).len(), 2, "only the live shards remain");
}

#[test]
fn a_shard_left_with_only_deleted_files_is_pruned() {
    let (_tmp, root) = project();
    put(&root, "victim.rs", SAMPLE);
    put(&root, "keep1.rs", SAMPLE);
    put(&root, "keep2.rs", SAMPLE);
    let (_home_guard, home) = temp();

    let cold = isolated(&root, &CHECK, &home);
    succeeded_with_finding(&cold);
    let files = cache_files(&home);
    assert_eq!(files.len(), 4, "one shard per file: {files:?}");
    let pruned = shard_holding(&files, "victim.rs");
    let bytes_before: std::collections::HashMap<PathBuf, Vec<u8>> = files
        .iter()
        .map(|path| (path.clone(), file_bytes(path)))
        .collect();
    std::fs::remove_file(root.join("victim.rs")).unwrap_or_else(|e| panic!("remove victim: {e}"));

    let out = isolated(&root, &CHECK, &home);
    assert!(out.status.success(), "stderr: {}", text(&out.stderr));
    let uncached = isolated(&root, &UNCACHED, &home);
    assert_eq!(
        out.stdout, uncached.stdout,
        "the pruned run must equal a cold run"
    );
    let after = cache_files(&home);
    assert_eq!(after.len(), 3, "the emptied shard is gone: {after:?}");
    assert!(
        !after.contains(&pruned),
        "the victim shard must be pruned: {after:?}"
    );
    for path in &after {
        assert_eq!(
            &file_bytes(path.as_path()),
            bytes_before
                .get(path)
                .unwrap_or_else(|| panic!("{} is a new shard", path.display())),
            "{path:?} must not rewrite"
        );
    }

    let warm = isolated(&root, &CHECK, &home);
    assert_eq!(warm.stdout, out.stdout, "the pruned cache must stay warm");
    assert!(warm.stderr.is_empty(), "stderr: {}", text(&warm.stderr));
}
