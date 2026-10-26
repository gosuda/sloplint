# Technical debt

## Audit of `bf96ca8..8789bd9`

Resolved in that branch:

- [x] vendor/tree-sitter-cpp/src/scanner.c `deserialize` now validates `length` against `sizeof(wchar_t)` and the delimiter array before any read, and resets on mismatch.
- [x] vendor/tree-sitter-bash/src/scanner.c `deserialize` now checks remaining length before every field and payload read, stages header scalars in locals, and resets on any malformed input.
- [x] crates/sloplint-syntax/src/hooks.rs `Cx` exposes query methods only; the node tables are private to the crate and attached by `walk.rs`.
- [x] crates/sloplint/src/cache.rs `Loaded` carries per-shard keys and hashes, `process` records `from_cache` at take time, and `store` compares shard rel sets without a cloned key map.

Remaining from that branch:

- [ ] crates/sloplint-core/src/rules/grep/homonym_definitions.rs `reported_names` [low][performance][high-confidence] On a 100k-file Go corpus (502k findings) the one hash pass over every definition takes about 0.39 s of a 2.3 s warm run, on the main thread. Trigger: the corpus that measured it is not present on this machine, so a change cannot be validated against its profile. Suggested fix when it is: run the world rules on the worker pool, or carry a precomputed name hash in `NamedSite`, which changes the cache format.
- [ ] crates/sloplint/src/main.rs `write_agent` [low][performance][high-confidence] Rendering the 159 MB agent report takes about 0.28 s on one thread. Trigger: no consumer has asked for faster report rendering. Suggested fix when one does: format windows of findings in parallel and write them in order.

## Audit of the owned surface, 2026-10-21

Nine high findings were fixed on `audit/owned-surface` (Python boolean defaults, C# and OCaml coverage, hermetic non-UTF-8 classification, per-run suppression sets, zero-copy source retention, allocation-free child traversal, Windows reparse refusals); a tenth high, the release tag-filter finding, was a doc-verified false positive and was reverted. The integration also resolves every previously listed item below the high floor:

- [x] `crates/sloplint/src/store.rs` reserves source buffers with `try_reserve_exact` and returns an I/O error when a sparse file's apparent length cannot fit in memory.
- [x] `crates/sloplint/src/project.rs` rejects absolute and parent-traversing `custom_rule_files` patterns with a `ConfigError` that names the pattern.
- [x] `crates/sloplint/src/lib.rs` applies suppressions before assigning fingerprints to the reported findings.
- [x] `crates/sloplint-syntax/src/custom.rs` keeps `CustomRules::run` crate-private, so `tree_sitter::Tree` does not enter that crate's public API.
- [x] `crates/sloplint-syntax/src/lib.rs` owns the OCaml sibling-interface set in `Parsers`; the OCaml adapter selects shadowed hooks without a generic `has_interface` pipeline field.
- [x] `crates/sloplint-syntax/src/lib.rs` keeps one configured parser per language and calls `set_language` only when that parser is first created.
- [x] `crates/sloplint/src/cache.rs` enumerates shard files on load and limits store comparisons to shards present on disk or in the fresh set.
- [x] `crates/sloplint/src/lib.rs` keys the OCaml interface set by the sibling `.ml` path, removing per-file `format!("{}i", rel)` allocation.
- [x] `crates/sloplint-core/src/finding.rs` makes `context_of` borrow lines that need neither carriage-return removal nor clipping.
- [x] `crates/sloplint-core/src/rules/agent/colocated_tests.rs` compares borrowed path components against sorted indexes without formatting candidate strings.
- [x] Kotlin visibility and test classification are pinned in `crates/sloplint-syntax/tests/native_adapter_boundaries/kotlin_visibility.rs`, `kotlin_test_attributes.rs`, and `audit_propagation.rs`; TypeScript namespace re-exports have positive and negative coverage in `rust_web_boundaries/typescript_import_require.rs`.
- [x] `crates/sloplint/tests/invariants/suppression_oracles.rs` runs exact, family, malformed, unused, and custom-rule suppression oracles across multiple language and comment forms.
- [x] `crates/sloplint/src/lib.rs` evaluates unused suppression operands independently, so a mixed directive reports each active, unmatched operand.
- [x] `crates/sloplint/src/lib.rs` initializes active rule and family sets lazily through `OnceLock`.
- [x] `crates/sloplint/src/lib.rs` retains source and scans for unused suppressions only when `meta/unused-suppression` is both active and selected.
- [x] `crates/sloplint-core/src/rules/mod.rs` extracts the single parse-error and unused-suppression finding by array destructuring.
- [x] `crates/sloplint/src/store.rs` rejects a directory or other non-regular baseline target with `InvalidInput` before replacement.
- [x] `crates/sloplint/src/lib.rs` clamps explicit and detected worker counts to 512 before constructing the rayon pool.
- [x] `crates/sloplint/src/main.rs` uses `sloplint_core::finding::line_starts` for human-renderer source lookup.
- [x] `crates/sloplint/src/terminal.rs` visibly escapes Unicode bidirectional controls along with C0, DEL, and C1 controls.
- [x] `crates/sloplint/src/fileio.rs`, `project.rs`, `store.rs`, and `cache.rs` bound configuration, per-file and total custom-rule, baseline byte and entry, source, and cache-shard reads.
- [x] `crates/sloplint/src/project.rs` propagates ancestor metadata failures while checking custom-rule paths for directory symlinks.

## Confirmed offense fixes

- `crates/sloplint/src/store.rs` refuses to overwrite an existing regular file unless it parses as a baseline; `main.rs` treats `BrokenPipe` as a completed write while preserving the run's gate status.
- `crates/sloplint-core/src/suppress.rs` stops reasons at the closer for each supported block-comment form.
- `crates/sloplint/src/fileio.rs` and `store.rs` open analyzed sources without following the final symlink; `discover.rs` stops repository `.gitignore` lookup at the repository boundary.
- `crates/sloplint-syntax/src/walk.rs` traverses each tree with one explicit cursor, and deep adapter helpers use explicit stacks or loops instead of recursion.
- `crates/sloplint-syntax/src/hooks.rs` records member state in one forward pass per container; `crates/sloplint-core/src/rules/agent/colocated_tests.rs` builds sorted borrowed path indexes once per run.
- `crates/sloplint-core/src/rules/agent/unique_patch_anchors.rs` coalesces adjacent repeated windows into one maximal repeated run before reporting anchors.

Deferred verification:

- The c4-go performance corpus is not present on the current machine, so no new c4 measurements ran here; the deferred homonym finding above keeps its original measurement.
- No new sanitizer proof was established. LeakSanitizer remains unavailable on the pinned stable toolchain, which has no `-Zsanitizer`; no full-suite valgrind substitute ran.
- Windows-only reparse-point tests and macOS-specific behavior have no runtime proof from this integration.
- Parent-level full gates have not run for this integration; this register records source-confirmed fixes only.
