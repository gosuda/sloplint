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

Nine high findings were fixed on `audit/owned-surface` (Python boolean defaults, C# and OCaml coverage, hermetic non-UTF-8 classification, per-run suppression sets, zero-copy source retention, allocation-free child traversal, Windows reparse refusals); a tenth high, the release tag-filter finding, was a doc-verified false positive and was reverted. Everything at or above the high floor is fixed and gate-verified. Below the floor, with triggers:

- [ ] crates/sloplint/src/lib.rs:656 [medium][security][high-confidence] `read_within_limit` sizes `Vec::with_capacity` from user-set `max_file_bytes` and apparent file length, so a sparse file aborts on OOM where `read_bounded` fails gracefully. Trigger: first hardening pass. Suggested fix: `try_reserve_exact`, mapping failure to an IO error.
- [ ] crates/sloplint/src/project.rs:513 [medium][code-quality][medium-confidence] `custom_source_roots` silently drops absolute or `..`-containing `custom_rule_files` patterns, so custom rules load as if absent. Trigger: first config-error pass. Suggested fix: return a `ConfigError` naming the pattern.
- [ ] crates/sloplint/src/lib.rs:562 [medium][backend][medium-confidence] Fingerprints are assigned before suppressions, so removing a suppressed duplicate renumbers a visible one and the next ratchet run misreports. Trigger: first ratchet-semantics pass. Suggested fix: apply suppressions, then fingerprint the reported set.
- [ ] crates/sloplint-syntax/src/custom.rs:51 [medium][architecture][high-confidence] `CustomRules::run` is public and re-exports `tree_sitter::Tree` across the crate boundary `analyze` otherwise keeps closed. Trigger: next public-API pass. Suggested fix: `pub(crate)`.
- [ ] crates/sloplint-syntax/src/lang/ocaml.rs:53 [medium][architecture][high-confidence] `has_interface` threads through generic pipeline inputs although only OCaml reads it. Trigger: next adapter-contract pass. Suggested fix: resolve sibling interfaces inside the adapter.
- [ ] crates/sloplint-syntax/src/lib.rs:49 [medium][performance][medium-confidence] `Parsers::parse` clones the grammar `Language` and revalidates per file. Trigger: measurable on a large single-language corpus. Suggested fix: skip `set_language` when the incoming language is unchanged.
- [ ] crates/sloplint/src/cache.rs:267 [medium][performance][high-confidence] Every run probes all 256 shard ids twice regardless of repo size. Trigger: measurable per-run syscall overhead. Suggested fix: enumerate the shard directory and touch only the loaded-plus-fresh set.
- [ ] crates/sloplint/src/lib.rs:382 [medium][performance][high-confidence] `process` allocates `format!("{}i", rel)` per OCaml file to probe the interface set. Trigger: first OCaml-heavy corpus. Suggested fix: key the set by the `.ml` path.
- [ ] crates/sloplint-core/src/finding.rs:194 [medium][performance][high-confidence] `context_of` copies every context line even when no `\r` and no margin exist. Trigger: profile on a findings-heavy corpus. Suggested fix: borrow when the line needs no edit.
- [ ] crates/sloplint-core/src/rules/agent/colocated_tests.rs:252 [medium][performance][high-confidence] World-rule counterpart matching formats candidate `String`s per prefix and per file. Trigger: same corpus as above. Suggested fix: length-partitioned borrowed comparisons against the sorted index.
- [ ] crates/sloplint-syntax/src/lang/kotlin.rs:92 [medium][test-quality][high-confidence] Kotlin visibility and test classification run unpinned by any test. Trigger: next adapter-coverage pass. Suggested fix: mirror the C#/OCaml boundary tests.
- [ ] crates/sloplint-syntax/src/lang/typescript/imports.rs:233 [medium][test-quality][high-confidence] `export * as ns from` namespace imports are unpinned. Trigger: same pass. Suggested fix: one positive and one negative case beside `typescript_import_require.rs`.
- [ ] crates/sloplint/tests/invariants/suppression_oracles.rs:17 [medium][test-quality][high-confidence] Every suppression oracle runs the same Rust sample project, so cross-language suppression is unproven. Trigger: same pass. Suggested fix: parameterize the sample language.
- [ ] crates/sloplint/src/lib.rs:634 [medium][backend][high-confidence] A suppression directive counts as active only when every operand names an active rule, while `Index::suppresses` marks it used on any operand match, so a mixed directive that suppresses nothing is never reported. Pre-existing; decide the intended mixed-directive semantics first. Suggested fix once decided: align the two predicates and pin both with oracles.
- [ ] crates/sloplint/src/lib.rs:234 [low][performance][high-confidence] The active id and family sets are built on every run even when no suppressions exist or the unused-suppression rule is filtered out. Trigger: measurable only on suppression-free corpora. Suggested fix: build lazily once any suppressions are found.
- [ ] crates/sloplint/src/lib.rs:424 [low][performance][high-confidence] Source retention and the finalize unused scan key on the rule filter alone, so a configured-off `meta/unused-suppression` still pays retention and scanning. Suggested fix: also gate on `rules.is_active`.
- [ ] crates/sloplint-core/src/rules/mod.rs:438 [low][code-quality][high-confidence] `RuleSet::parse_error` wraps a one-element array in `into_iter().next().unwrap_or_else` with an unreachable fallback. Suggested fix: destructure the single element.
- [ ] crates/sloplint-core/src/rules/mod.rs:459 [low][code-quality][high-confidence] `RuleSet::unused_suppression` repeats the same impossible-branch pattern. Suggested fix: same destructure.
- [ ] crates/sloplint/src/lib.rs:698 [low][code-quality][high-confidence] `write_replacing` routes directories through a raw OS open error instead of the clear invalid-input refusal the doc comment promises. Suggested fix: reject non-regular targets outright.
- [ ] crates/sloplint/src/lib.rs:787 [low][code-quality][medium-confidence] `jobs()` forwards any nonzero thread request to rayon; library callers get an IO-mapped failure on absurd values. Suggested fix: clamp or validate up front.
- [ ] crates/sloplint/src/main.rs:292 [low][code-quality][medium-confidence] `Source::new` keeps a second line-index with edge semantics the core helper does not share. Suggested fix: reuse `line_starts`.
- [ ] crates/sloplint/src/project.rs:602 [low][code-quality][medium-confidence] `has_dir_symlink` fails open when an ancestor's metadata cannot be read. Suggested fix: propagate the error.
- [ ] crates/sloplint/src/terminal.rs:40 [low][security][medium-confidence] Terminal output escapes C0/DEL/C1 but not Unicode bidi controls, so attacker-controlled source text can reorder displayed findings. Suggested fix: strip or escape bidi controls in `TerminalText`.
- [ ] crates/sloplint/src/project.rs:400 [low][security][high-confidence] Config, custom-rule, and baseline reads have no size ceiling, so an oversized hostile file is fully read and parsed. Suggested fix: a small explicit ceiling for config-class inputs.
- [ ] crates/sloplint/src/project.rs:581 [low][security][high-confidence] Each matched `custom_rule_files` entry is read unbounded. Suggested fix: apply the config ceiling per file and cap the total.
- [ ] crates/sloplint/src/lib.rs:640 [low][security][high-confidence] Ratchet baselines are read and parsed unbounded. Suggested fix: cap baseline bytes and entry count.
- [ ] crates/sloplint/src/cache.rs:215 [low][security][medium-confidence] Shard decode trusts the on-disk length prefix with no per-shard bound. Suggested fix: cap shard bytes and drop oversized shards.

Deferred verification:

- The c4-go performance corpus is not present on the current machine, so no c4 measurements run here; the homonym finding above keeps its original measurement.
- LeakSanitizer is unestablished: the pinned stable toolchain has no `-Zsanitizer`, and a full-suite valgrind run costs hours. Establish one before relying on leak claims.
- The Windows reparse-point adversary tests compile cross-target and run only on the CI Windows legs; they have no local runtime proof.
