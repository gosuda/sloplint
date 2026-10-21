# Technical debt

Low-severity findings deferred from the audit of `bf96ca8..8789bd9`. Resolved items were fixed in this branch; each remaining item lists its concrete trigger.

Resolved in this branch:

- [x] vendor/tree-sitter-cpp/src/scanner.c `deserialize` now validates `length` against `sizeof(wchar_t)` and the delimiter array before any read, and resets on mismatch.
- [x] vendor/tree-sitter-bash/src/scanner.c `deserialize` now checks remaining length before every field and payload read, stages header scalars in locals, and resets on any malformed input.
- [x] crates/sloplint-syntax/src/hooks.rs `Cx` exposes query methods only; the node tables are private to the crate and attached by `walk.rs`.
- [x] crates/sloplint/src/cache.rs `Loaded` carries per-shard keys and hashes, `process` records `from_cache` at take time, and `store` compares shard rel sets without a cloned key map.

Remaining:

- [ ] crates/sloplint-core/src/rules/grep/homonym_definitions.rs `reported_names` [low][performance][high-confidence] On a 100k-file Go corpus (502k findings) the one hash pass over every definition takes about 0.39 s of a 2.3 s warm run, on the main thread. Trigger: unresolved because the corpus that measured it is not present on the current machine, so a hashing or scheduling change cannot be validated against its profile here. Suggested fix when it is: run the world rules on the worker pool, or carry a precomputed name hash in `NamedSite`, which changes the cache format.
- [ ] crates/sloplint/src/main.rs `write_agent` [low][performance][high-confidence] Rendering the 159 MB agent report takes about 0.28 s on one thread. Trigger: no consumer has asked for faster report rendering. Suggested fix when one does: format windows of findings in parallel and write them in order.

Deferred verification:

- The c4-go performance corpus is not present on the current machine, so no c4 measurements run here; the homonym finding above keeps its original measurement.
- LeakSanitizer is unestablished: the pinned stable toolchain has no `-Zsanitizer`, and a full-suite valgrind run costs hours. Establish one before relying on leak claims.
