<div align="center">
  <h1>sloplint</h1>
  <p><strong>An agent-first code quality linter for thirty languages.</strong></p>

  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg?style=flat" alt="License"></a>
  <a href="https://github.com/gosuda/sloplint/actions/workflows/ci.yml"><img src="https://github.com/gosuda/sloplint/actions/workflows/ci.yml/badge.svg" alt="CI status"></a>

[![Built with Depot](https://depot.dev/badges/built-with-depot.svg)](https://depot.dev?utm_source=gosuda)
</div>

## What is this?

sloplint finds code patterns that make changes harder to review and apply: deep nesting, names hidden from search, redundant wrappers, and repeated blocks that make edits ambiguous. For each finding it reports a source location, an explanation, and a suggested fix.

It is written for developers and for coding agents. The default `agent` output format is stable, machine-parseable text, so an agent can consume findings directly and apply the `fix:` line attached to each one.

- **30 languages**: Rust, TypeScript/TSX, JavaScript/JSX, Python, Go, LoGOs, Java, Kotlin, C#, C, C++, Objective-C, PHP, Ruby, Bash, PowerShell, Swift, R, Scala, Dart, Zig, OCaml, Odin, Starlark, Lean 4, Quint, TLA+, Alloy, Dafny, Racket.
- **29 builtin rules** in four families (`complexity/`, `grep/`, `slop/`, `agent/`), plus custom tree-sitter queries from `[[custom_rules]]` configuration.
- **4 strictness profiles** (`minimal`, `basic`, `standard`, `strict`), each monotone in coverage and thresholds. A rule stays on at `standard` only when at least 80 percent of sampled findings were judged true after tightening; new rules without measured precision start as strict-only advisories (see [Precision](#precision)).
- **Ratchet mode** freezes today's findings into a baseline, so only new debt fails the build.

sloplint works from syntax alone: tree-sitter parses each file and per-language heuristics lower the tree into facts, so results are approximate by design. It is not a typechecker or a security analyzer, and it applies no patches itself; every finding carries a `fix:` line describing the change a human or agent should make. In C#, property accessors, operators, conversion operators, and destructors count as functions.

## Quick start

Requires the `sloplint` binary (see [Install](#install)). Then, in a new empty directory:

```console
$ sloplint init
$ mkdir src && printf 'pub fn handle(flag: bool) {}\n' > src/lib.rs
$ sloplint check --strictness standard
src/lib.rs:1:8: minor/low: `handle` is a generic name shared by unrelated code [grep/generic-name]
  context: pub fn handle(flag: bool) {}
  fix: name the item after what it holds or does

rules:
  grep/generic-name: Generic names match unrelated code in every search.
summary: strictness=standard files=1 findings=1 blocker=0 critical=0 major=0 minor=1 info=0 baselined=0 stale=0 skipped=0
```

In an existing project, run `sloplint init` once to write the default configuration, then run `sloplint check`.

Exit code `0` means the gate passed: the run can still list below-`fail_on` findings (default `major`). `1` means at least one reported finding is at or above `fail_on` and at or above `min_confidence`, or the `--fail-on-skipped` gate tripped; `2` means usage or config error; `3` means an I/O error stopped the run.

## Install

Install from source with the command below. There is no registry install: the `sloplint` name on crates.io is a reserved stub, and no release binaries or npm wrapper have been published.

Prerequisites: Git, [rustup](https://rustup.rs/), and a C/C++ toolchain for the target platform (the tree-sitter parsers and scanners compile native sources). The checked-in `rust-toolchain.toml` pins Rust 1.100.0, which `cargo` selects automatically.

```bash
cargo install --git https://github.com/gosuda/sloplint --locked
```

This places the `sloplint` binary in `~/.cargo/bin`; make sure that directory is on your `PATH`. The release workflow builds archives for six targets (x86_64 and aarch64 on Linux, macOS, and Windows), but no release has been published yet.

## Usage

### Commands

| Command | What it does |
|---|---|
| `sloplint check [PATHS]` | Lint files (bare `sloplint` is the same) |
| `sloplint baseline` | Freeze current findings into the baseline file |
| `sloplint rules [ID]` | Show effective rule settings for the active profile |
| `sloplint init` | Write a starter `sloplint.toml`; use `sloplint rules` for effective rule settings |

`check` flags: `--format agent|human|json`, `--strictness`, `--mode threshold|ratchet`, `--fail-on info|minor|major|critical|blocker|never`, `--min-confidence low|medium|high`, `--fail-on-skipped`, `--rule ID` (repeatable, `family/*` allowed), `--root DIR`, `--config FILE`, `--no-cache`, `--jobs N`.

Unknown `--rule` selectors fail with exit 2 listing the valid rule ids. `--root DIR` pins the project root for every command; the baseline file and custom rule files still resolve under that root. Without `--root`, the config directory becomes the root when `--config` names a file, else the nearest ancestor holding `sloplint.toml` or `.git` (or the current directory) does. CI should pass `--root` with a trusted `--config` so a checkout cannot redirect the lint at another tree; without the pin, a `check` from a checkout that carries its own `sloplint.toml` lints that checkout under its own rules.

Every run analyzes the project root. With no `PATHS`, only findings under the current directory are reported and gated; when the current directory is outside the root, the whole root is reported. With `PATHS`, only findings under those paths are reported and gated. Hidden files are skipped. `.gitignore` applies up to the enclosing repository; `.ignore` applies from every ancestor directory.

### Rules

- **complexity/**: nesting-weighted cognitive depth, nesting depth, mutable-statement spans, boolean params, import fan-out.
- **grep/**: generic names, homonym definitions across files, renamed and wildcard imports, runtime-built symbols, non-literal log messages, multiline arguments.
- **slop/**: pass-through wrappers, redundant else blocks, echo comments, swallowed or double-reported errors, single-implementation abstractions, barrel files, field-copy literals.
- **agent/**: file size budgets, stringly-typed APIs, missing boundary annotations, untyped error returns, catch-all matches, ambient global state, message-less assertions, ambiguous patch anchors, missing colocated tests, wall-clock sleeps in tests.
- **custom/**: tree-sitter queries in `sloplint.toml` or separate files, with captures interpolated into messages, per-rule severity, and path globs. Each rule is bounded per file at 16,384 in-progress matches and 1,024 reported hits: a query that exceeds either cap reports `meta/custom-cap` for rules that are enabled and whose globs match the file, instead of returning partial results silently.

`meta/parse-error` is Minor in every profile, so a default `major` gate does not fail on it. Set the rule to a higher rung or pass `--fail-on minor`. `meta/custom-cap` is Major in every profile: a capped query's results are partial, so a default `major` gate fails until the query is anchored; the cap uses the same eligibility as the rule's own findings, so turning a rule off or excluding the file silences its cap too. An empty `project.languages` list analyzes nothing.

Parsing has a shared ten-second budget per file, checked cooperatively across
the primary grammar, fallback grammars, and recovery attempts. A timeout reports
one `meta/parse-error` finding and contributes no syntax facts. Timed-out files
are not cached, so a later run retries them. The budget limits parsing work; it
is not a hard memory limit or a deadline for the complete project scan.

Recovery can retain healthy constructs after removing unsupported syntax while
still reporting the original parse error. Findings in affected constructs are
discarded, and source byte positions remain unchanged. Lean recovery also
handles complete commands and parser fragments without discarding neighboring
healthy commands.

The full rule table, with per-profile levels, role shifts, and predicates, is under [Rule reference](#rule-reference) below; `sloplint rules` prints the effective settings and short summaries for the active profile.

`slop/redundant-else` and `agent/no-sleep-in-tests` adopt selected practices from JustGoodPractices. They are strict-only advisories with explicit language and runtime limits. See [Adopted practices](docs/just-good-practices.md) for examples, exclusions, and the rationale for leaving semantic judgments to review.

### Severity and confidence

Findings use five rungs, ordered `info < minor < major < critical < blocker`. Each graded rule sets four cut points (`minor_at`, `major_at`, `critical_at`, `blocker_at`); the highest cut point at or below the measured value wins. Each single-severity rule reports its level rung. `project.fail_on` names the lowest failing rung (`info|minor|major|critical|blocker|never`, default `major`). `[rules."<id>"] level` is `off` or a rung; for graded rules the level is a ceiling.

Each finding carries `low < medium < high`. `project.min_confidence` (default `low`) sets the lowest confidence counted toward the gate; `--min-confidence low|medium|high` overrides it. Low confidence findings still print, they only stop failing the run.

### Configuration

`--config FILE` names a file that must exist; without `--config`, the root's `sloplint.toml` applies when present and defaults apply otherwise. Configuration and custom rule files are capped at 1 MiB per file (16 MiB total for custom rules); baselines are capped at 512 MiB and 1,000,000 entries total across findings and skipped paths.

Rule option names and values are validated even when a rule is disabled or filtered out. An invalid threshold or unknown option fails with exit `2`; changing strictness cannot hide the error.

Project keys: `strictness`, `languages`, `mode`, `baseline`, `fail_on` (rung or `never`), `min_confidence` (`low|medium|high`), `fail_on_skipped` (default `false`), `exclude`, `extend_exclude`, `test_globs`, `bench_globs`, `example_globs`, `support_globs`, `entry_globs`, `data_globs`, `max_file_bytes`, `custom_rule_files`. Rule tables accept `level = "off"` or a rung, graded cut points `minor_at`, `major_at`, `critical_at`, `blocker_at` (each at least 1, non-decreasing in rung order), and every other option key of that rule's preset. For example, `[rules."grep/generic-name"]` accepts `names` to replace the built-in denylist. Custom rules accept `severity` as a rung and optional `confidence` (default `medium`).

Keep `max_file_bytes` at or below 16777216. The default is 2097152. A larger value fails with exit `2` and names the ceiling.

```toml
[project]
strictness = "standard"
fail_on = "major"
min_confidence = "low"

[rules."complexity/cognitive-depth"]
major_at = 18                          # minor_at still comes from the preset

[rules."agent/colocated-tests"]
level = "minor"                       # enables a rule the standard profile leaves off
```

A `[languages.<key>]` table sets `log_callees`, the call names the language treats as log calls. `grep/contiguous-log-literal` and `slop/zombie-catch` read it. `sloplint init` writes one such table per language with the default names. `grep/contiguous-log-literal` does not run on Lean or TLA+, including with configured log callees: Lean elaborator diagnostics and `#do` test scaffolding do not provide a reliable syntactic signal for this rule, and TLA+ `Print` and `PrintT` calls are the only observability in a model and serve as its test oracles.

### Ratchet mode

```bash
sloplint baseline               # writes sloplint-baseline.json
sloplint check --mode ratchet   # reports only findings absent from the baseline
```

`project.baseline` is relative to the project root. Paths that resolve outside the root are rejected. A baseline written before the skipped set existed records no skipped paths, so enabling `fail_on_skipped` with such a baseline reports every skipped file as newly skipped until `sloplint baseline` is rerun. The recorded skipped set is a snapshot like the findings: a path stays known until the baseline is rewritten, even if the file was analyzed in between.

`sloplint baseline` refuses to replace an existing regular file that is not a sloplint baseline, such as `sloplint.toml` or a source file: it exits 2 and leaves the file untouched; delete the file to record a baseline there.

The cache is per-user and never stored in the checkout. It lives in `sloplint/<hash of the project root>/cache-v2/` under `$XDG_CACHE_HOME` when that is absolute, else under `~/.cache`, `~/Library/Caches`, or `%LOCALAPPDATA%`. The key includes a hash of the running executable. `--no-cache` skips it. A damaged or mismatched cache falls back to a fresh analysis.

sloplint warns and runs without the cache when the cache directory is a symlink, is not a directory, is writable by group or others, or sits inside the project root, when no home directory is set, or when the running executable cannot be read.

Older releases left a `cache-v1.bin` file in the per-user cache directory, and the oldest kept a `.sloplint` directory in the checkout. sloplint never reads or removes either; delete them by hand to reclaim the space.

### Custom rules

```toml
[[custom_rules]]
id = "custom/no-unwrap-in-lib"
language = "rust"
severity = "major"
confidence = "medium"
message = "`{method}()` in library code"
why = "unwrap panics on the error path and hides the failure from callers"
fix = "propagate with ? and return a typed error"
query = '''
(call_expression
  function: (field_expression field: (field_identifier) @method)
  (#eq? @method "unwrap")) @match
'''
```

Files named by `project.custom_rule_files` load even when hidden or gitignored. `project.exclude` and `project.extend_exclude` do not apply to custom rule files, and files under `.git/` are never loaded. An unreadable directory inside the selection fails the run. An absolute pattern, a pattern containing `..`, or a symlinked rule file is a configuration error.

### Suppressions

Suppress inline only where you cannot fix the code yet. A reason is required. A directive that suppresses nothing is itself a `meta/unused-suppression` finding, reported for each named active rule that matched nothing.

```rust
// sloplint-allow(complexity/cognitive-depth): state machine is one place by design
```

A directive covers its own comment and the next line. `sloplint-allow-file(rule): reason` covers the whole file.

When `complexity/cognitive-depth` and `complexity/nesting-depth` report the same function, only the cognitive-depth finding is kept. Suppressing one of the two rules leaves the other in place.

### Output formats

`--format agent` (default, stable machine-parseable text), `--format human` (annotated snippets with color), `--format json` (one document: findings, rules fired, summary).

Agent lines read `path:line:col: <rung>/<confidence>: message [rule]`, followed by `context:` and `fix:` lines. The summary reads `summary: strictness=S files=N findings=N blocker=N critical=N major=N minor=N info=N baselined=N stale=N skipped=N`. JSON findings carry `"severity"` and `"confidence"`; the JSON summary carries one count per rung. Human output uses one color per rung. `sloplint rules` shows `level`, `confidence`, `roles=entry:..,test:..,support:..,bench:..,example:..`, and cut points for graded rules.

### Skipped files

An unreadable source file or source directory stops the run with exit `3` and reports its path and cause. The run skips oversized files, non-UTF-8 files, vanished files, symlinks, non-regular files, files with non-UTF-8 names, generated files, and enabled source files matched by `project.data_globs` (by default, `testdata`, `test-data`, `__fixtures__`, and `__snapshots__`). Unsupported data files and files in disabled languages are ignored. Each unreadable data directory contributes one `data` skip; readable data directories are traversed and their enabled source files are counted individually. The summary reports `skipped=N`; skips do not change the exit status unless the skip gate is enabled.

A file counts as generated when a marker phrase appears in a comment of its header, which is the leading run of blank and comment lines in the first 2048 bytes. Code that follows the end of a block comment is not read. A comment uses only its own language's syntax: `//` and `/* */` in Rust, TypeScript, JavaScript, Go, LoGOs, Java, Kotlin, C#, C, C++, Objective-C, Swift, Scala, Dart, Odin, PHP, Quint, and Dafny; `//` alone in Zig; `#` in Python, Ruby, Bash, PowerShell, R, Starlark, and PHP, except a PHP `#[` attribute line; `<# #>` in PowerShell; `(* *)` in OCaml; `\*` lines and `(* *)` blocks in TLA+; `--` lines with `/-` to `-/` blocks in Lean; `//` and `--` lines with `/* */` blocks in Alloy; `;` lines with `#|` to `|#` blocks in Racket; and a `"""` or `'''` docstring in Python and Starlark. An interpreter line that starts with `#!/` or `#! ` is a header line in every language. The marker phrases are `@generated`, `automatically generated`, `auto generated`, `auto-generated`, `autogenerated`, `do not edit`, and `do not modify by hand`. Pass `--fail-on-skipped` (or set `project.fail_on_skipped = true`) to fail with exit `1` instead: threshold mode fails on any skipped file, ratchet mode fails only on files newly skipped since the baseline, and the run names each gating file after the summary.

## Code roles

Files are `library`, `entry`, `test`, `support`, `bench`, or `example`; `data` paths are excluded from analysis. The file role comes from path globs alone, resolved with precedence `data > bench > example > support > test > entry > library`. Language adapters additionally mark inline `test` and `bench` regions inside a file (test functions, `cfg(test)` modules, bench functions), and Go `Example*` functions as `example` regions; a file with at least one inline test region reports `has_inline_tests`. Defaults:

- `bench`: `**/benches/**,**/bench/**,**/benchmarks/**,**/benchmark/**,**/fuzz/**,**/fuzz_targets/**,**/*_bench.rs,**/*.bench.ts,**/*.bench.tsx,**/*.bench.js,**/*.bench.mjs,**/*_bench.py,**/bench_*.py,**/*_benchmark.cc,**/*_benchmark.cpp,**/*_bench.cc,**/*_bench.cpp,**/*_fuzzer.cc,**/*_fuzzer.cpp,**/src/jmh/**`
- `example`: `**/examples/**,**/example/**,**/_examples/**`
- `support`: `**/conftest.py,**/testutil/**,**/testutils/**,**/test_util/**,**/test_utils/**,**/test_helpers/**,**/test_support/**,**/testing/**,**/tests/common/**,**/tests/support/**,**/spec/support/**,**/__mocks__/**`
- `entry`: `**/src/main.rs,**/src/bin/**,**/build.rs,**/main.go,**/main.lgo,**/cmd/**,**/__main__.py,**/manage.py,**/scripts/**`
- `data`: `**/testdata/**,**/test-data/**,**/__fixtures__/**,**/__snapshots__/**`
- `test`: `**/tests/**,**/test/**,**/__tests__/**,**/*_test.go,**/*_test.lgo,**/test_*.py,**/*_test.py,**/*.test.ts,**/*.test.tsx,**/*.spec.ts,**/*.spec.tsx,**/*_test.c,**/test_*.c,**/*_test.cc,**/*_test.cpp,**/*_test.cxx,**/*_unittest.cc,**/test_*.cc,**/test_*.cpp,**/*Tests.m,**/*Tests.cs,**/*Test.cs,**/*.Tests/**,**/*_test.dart,**/*Spec.scala,**/*Test.scala,**/*Suite.scala,**/*.test.js,**/*.test.jsx,**/*.test.mjs,**/*.test.cjs,**/*.spec.js,**/*.spec.jsx,**/*.spec.mjs,**/*.spec.cjs,**/*Test.java,**/*Tests.java,**/*Test.kt,**/*Tests.kt,**/Tests/**,**/*Tests.swift,**/*Test.php,**/*_spec.rb,**/*_test.rb,**/spec/**,**/*.bats,**/test_*.sh,**/*_test.sh,**/tests/**/*.sh,**/*.Tests.ps1,**/*.tests.ps1,**/tests/**/*.ps1,**/test.rkt,**/tests.rkt,**/test.rktl,**/tests.rktl,**/test.rktd,**/tests.rktd`

Role shifts lower findings by role (K = keep, L1 = lower one rung, L2 = lower two rungs, O = off); see the [rule reference](#rule-reference) table.

## Packages

A file belongs to the nearest ancestor directory holding a manifest of its own ecosystem: `package.json` names JavaScript and TypeScript packages, while `pyproject.toml` and `setup.py` name Python packages; files of other languages ignore manifests. Without such an ancestor a Rust file under `crates/` groups by `crates/<name>` while any other Rust file forms the empty package; a Go file groups by its parent directory; files in other languages group by the top-level directory; a root-level file forms the empty package. Only manifests at or below the project root count, so the same file can key differently under a narrower `--root`. A manifest hidden by `project.exclude` or `project.data_globs` does not define a package. Non-source manifests do not count as skipped files. Manifest files stay in the lint set, so `setup.py` is still linted as Python, and a cached run re-stamps the key when a manifest appears or disappears.

## Precision

A finding is true when a senior reviewer would ask for a change. The original 27 rules were measured on the same 15 corpora of real open-source code: a sample of their findings was counted as true, noise, or wrong. A rule stays on at `standard` only when at least 80 percent of its sampled findings are true after tightening. A rule below that bar is `Off` at minimal, basic, and standard and runs only at strict, at `info` with `low` confidence. `--min-confidence` and `--fail-on` decide what gates. Counts below are the pre-tightening samples from the per-rule reports.

The new `slop/redundant-else` and `agent/no-sleep-in-tests` rules have not yet been sampled on those corpora. Both stay off through standard and report only `info` with `low` confidence at strict. Their regression fixtures establish behavior, not corpus precision.

| Rule | Sampled | True | Noise | Wrong |
|---|---|---|---|---|
| complexity/boolean-params | 43 | 7 | 28 | 8 |
| complexity/cognitive-depth | 43 | 12 | 29 | 2 |
| complexity/import-fan-out | 41 | 7 | 30 | 4 |
| complexity/mutable-span | 56 | 0 | 52 | 4 |
| complexity/nesting-depth | 45 | 6 | 37 | 2 |
| grep/contiguous-log-literal | 41 | 7 | 24 | 10 |
| grep/generic-name | 5 | 0 | 5 | 0 |
| grep/homonym-definitions | 8 | 0 | 5 | 3 |
| grep/multiline-argument | 42 | 1 | 39 | 2 |
| grep/no-dynamic-symbol | 42 | 4 | 38 | 0 |
| grep/no-rename-import | 128 | 0 | 55 | 73 |
| grep/no-wildcard-import | 43 | 1 | 39 | 3 |
| slop/barrel-file | 1 | 0 | 1 | 0 |
| slop/echo-comment | 45 | 6 | 38 | 1 |
| slop/field-copy | 2 | 0 | 1 | 1 |
| slop/passthrough-wrapper | 42 | 0 | 20 | 22 |
| slop/single-impl-abstraction | 32 | 0 | 31 | 1 |
| slop/zombie-catch | 51 | 1 | 50 | 0 |
| agent/ambient-global-state | 48 | 0 | 32 | 16 |
| agent/catch-all-match | 50 | 4 | 46 | 0 |
| agent/colocated-tests | 42 | 0 | 33 | 9 |
| agent/explicit-boundary-types | 2 | 0 | 2 | 0 |
| agent/file-token-budget | 42 | 10 | 32 | 0 |
| agent/informative-assertions | 44 | 2 | 39 | 3 |
| agent/stringly-api | 65 | 4 | 55 | 5 |
| agent/typed-error-boundary | 0 | 0 | 0 | 0 |
| agent/unique-patch-anchors | 42 | 1 | 41 | 0 |

`grep/no-rename-import` (0 of 128) and `agent/colocated-tests` (0 of 42) run only at strict, at `info` with `low` confidence. `slop/zombie-catch` (1 of 51) runs only at strict, at `info` with `low` confidence. Zero-true tightenings are redefined predicates and are re-sampled; a re-sample below 80 percent true flips the rule to `Off` or `info` without further review.

## Language applicability

Language applicability is separate from severity. The exclusions below apply in
all strictness profiles and remain in effect when a rule is explicitly enabled.
They avoid applying procedural conventions to proof statements, specification
models, or language idioms that the available syntax facts cannot judge reliably.
The rule reference also identifies predicates that apply only to certain languages.

| Language | Excluded rules |
|---|---|
| Alloy | `complexity/cognitive-depth`, `complexity/nesting-depth`, `grep/homonym-definitions`, `slop/passthrough-wrapper`, `agent/colocated-tests`, `agent/unique-patch-anchors` |
| Dafny | `complexity/mutable-span`, `grep/homonym-definitions`, `agent/colocated-tests`, `agent/stringly-api` |
| Go | `agent/catch-all-match` |
| Lean | `grep/contiguous-log-literal`, `grep/no-wildcard-import`, `agent/catch-all-match`, `agent/stringly-api` |
| LoGOs | `grep/homonym-definitions`, `slop/passthrough-wrapper`, `agent/ambient-global-state`, `agent/catch-all-match` |
| Quint | `complexity/cognitive-depth`, `complexity/nesting-depth`, `grep/no-rename-import`, `grep/no-wildcard-import`, `agent/ambient-global-state`, `agent/unique-patch-anchors` |
| Racket | `slop/echo-comment`, `agent/file-token-budget` |
| TLA+ | `complexity/cognitive-depth`, `grep/contiguous-log-literal`, `grep/generic-name`, `grep/homonym-definitions`, `slop/echo-comment`, `agent/colocated-tests`, `agent/unique-patch-anchors` |

LoGOs also uses minimum mutable-span cut points of 30/45/60/90 lines and at
least five repetitions for `agent/unique-patch-anchors`, including when a lower
threshold is configured. Dafny `print` statements remain ordinary calls rather
than log-message evidence.

## Rule reference

Role shifts lower findings by role (K = keep, L1 = lower one rung, L2 = lower two rungs, O = off):

| Rule | minimal | basic | standard | strict | conf | entry | test | support | bench | example | Predicate |
|---|---|---|---|---|---|---|---|---|---|---|---|
| complexity/cognitive-depth | Off | Off | Off | info | low | K | L1 | K | L1 | K | Scores if/loop/catch with depth weight; flags iff score >= cut and max depth >= 2; JS and TS closures bound to a name score as their own function, and unbound callbacks score as unnamed functions |
| complexity/nesting-depth | Off | Off | Off | info | low | K | L1 | K | L1 | K | Flags control nesting depth >= cut; JS and TS closures bound to a name score as their own function, and unbound callbacks score as unnamed functions |
| complexity/mutable-span | Off | Off | Off | info | low | K | L1 | K | O | L1 | Flags bindings written on 2+ lines spanning >= cut lines. Writes count on the innermost binding of a name. In Logos the cutoffs rise to at least 30/45/60/90 lines |
| complexity/boolean-params | Off | Off | Off | info | low | L1 | O | L1 | O | O | Flags exported fns with >= 2 Bool or bool-defaulted params, skipping variadic, keyword-only, and overloads |
| complexity/import-fan-out | Off | Off | minor | major | medium | L1 | L1 | K | L1 | L1 | Counts distinct third-party package roots of module-scope imports; barrel reexport-only files skipped |
| grep/contiguous-log-literal | Off | Off | Off | info | low | K | O | L1 | O | O | Flags message-bearing log calls with missing, dynamic, or short static messages; a literal, template, interpolation, or concatenation passes only when its longest static run reaches 8 chars. In Bash, option words such as `-n` are arguments like any other word |
| grep/generic-name | Off | Off | minor | minor | low | L1 | O | K | O | O | Flags exported fns named from the generic denylist |
| grep/homonym-definitions | Off | Off | Off | info | low | K | O | K | O | O | Flags free fn or type names defined in >= 8 files (5 strict) of one package; definition sites in test, bench or example code are not counted |
| grep/multiline-argument | Off | Off | Off | info | low | K | O | L1 | O | O | Strict-only: flags collection args spanning >= 15 lines outside tests, generated files, and binding initializers |
| grep/no-dynamic-symbol | Off | Off | Off | info | low | K | L1 | K | L1 | K | Flags runtime-built symbols except proxy forwarding and string module imports. In Python, JS, TS and Starlark a lookup inside a loop over an inline literal list of strings is exempt |
| grep/no-rename-import | Off | Off | Off | info | low | K | L1 | K | L1 | K | Flags `import x as y` renames |
| grep/no-wildcard-import | Off | Off | Off | info | low | K | O | L1 | O | L1 | Flags `*` imports binding open names into non-test module scope; preludes and same-file children exempt |
| slop/barrel-file | Off | Off | minor | minor | low | K | O | K | O | O | Flags barrels re-exporting from another barrel with resolvable local candidates |
| slop/echo-comment | Off | Off | minor | minor | medium | K | L1 | K | L1 | K | Flags comments whose stemmed word set exactly equals the function name set |
| slop/field-copy | Off | Off | minor | minor | low | K | O | L1 | O | L1 | Flags constructions copying >= 6 fields (4 strict) from one source; method callees, kwargs bags, same-type rebuilds, and copies from the own receiver are excluded |
| slop/passthrough-wrapper | Off | Off | Off | info | low | K | O | K | O | L1 | Flags sole-call wrappers to simple-path callees outside the receiver, params, deprecated, trait, test, and referenced uses. The receiver includes `self`, `this`, `cls`, `super`, `base`, `parent`, `static`, and the Go named receiver |
| slop/redundant-else | Off | Off | Off | info | low | K | K | K | K | K | C, C++, C#, Go, Java, JS/JSX, Python, Rust, TS/TSX: flags a nonempty else after a direct terminal statement; skips decision chains, binding scopes, and control contexts that prevent safe dedenting. Rust requires discarded statements with a plain identifier condition and explicitly terminated alternative expressions; see [language limits](docs/just-good-practices.md) |
| slop/single-impl-abstraction | Off | Off | minor | minor | medium | K | O | K | O | O | Flags private traits or interfaces with exactly one concrete same-package implementation |
| slop/zombie-catch | Off | Off | Off | info | low | K | L1 | K | L1 | K | Strict-only: flags empty handlers at Info; log-and-rethrow arms reported, bare rethrows dropped |
| agent/ambient-global-state | Off | Off | Off | info | low | O | O | L1 | O | O | Flags package mutables assigned in functions and env reads stored or written through the env object; a write to a shadowing local or parameter is not reported. In R, `<<-` and `->>` in a function count as global writes, and `options()` does not count as an environment write |
| agent/catch-all-match | Off | Off | Off | info | low | K | L1 | K | L1 | L1 | Flags bare wildcards on Ident, Field, or Path scrutinees with no Some, None, Ok, or Err sibling |
| agent/colocated-tests | Off | Off | Off | info | low | O | O | O | O | O | Strict-only: stays silent when a language has no test evidence; flags library files with public functions when no same-language test in the package covers them and no same-language test import covers the file; package is the nearest manifest dir of its own ecosystem, else `crates/<name>` for Rust under `crates/`, the empty package for other Rust files, the parent directory for Go, or the top-level directory for other languages; import keys use the last 4 segments (2 or more, single segment only for a top-level module; Python `src/` roots map to the package); relative imports resolve against the importing file; `@/` and `~/` map to `src/`, `@scope/` stays scoped, Java wildcards and non-suffix Go module prefixes do not cover; `-`/`_` match. A Rust test import of `crate::Name`, or of `<name>::Name` for a crate under `crates/<name>`, that names no module covers the root file of that crate when the root defines `Name`, else the module the root re-exports `Name` from, else each module the root glob re-exports, else the root. A test in the same package that exercises a module without sharing its name or importing it does not count as coverage |
| agent/explicit-boundary-types | Off | minor | minor | minor | medium | L1 | O | L1 | O | O | Flags exported fns in TS, Python, Kotlin, Scala, and Dart with missing annotations |
| agent/file-token-budget | Off | Off | Off | info | low | K | O | O | L1 | L1 | Strict-only: flags files with >= 600 lines outside inline test regions; generated files skipped |
| agent/informative-assertions | Off | Off | Off | info | low | K | K | K | O | L1 | Flags bare-name or top ==/!= asserts (Rust), Name or Attr (Python), and bool-literal True/False (Go) |
| agent/no-sleep-in-tests | Off | Off | Off | info | low | O | K | K | O | O | C#, Go, Java, Python, Rust: flags identified standard-library sleep calls only in test or support roles; excludes known zero delays, ambiguous bindings, and recognized Go synctest callbacks. Duration and alias handling follow each runtime; see [language limits](docs/just-good-practices.md) |
| agent/stringly-api | Off | Off | Off | info | low | L1 | O | L1 | O | O | Flags runs of >= 3 same-kind stringly params (4 to deny); runs break on kind change or defaulted params |
| agent/typed-error-boundary | Off | Off | Off | info | low | O | O | O | O | O | Flags exported fns whose error slot is anyhow, eyre, Box dyn Error, or an untyped throw |
| agent/unique-patch-anchors | Off | Off | Off | info | low | K | L1 | K | L1 | K | Flags 8-line (6 strict) blocks repeated >= 3 times inside function bodies. In Logos the minimum rises to 5 repeats |
| meta/parse-error | minor | minor | minor | minor | high | K | K | K | K | K | Syntax errors, partial parse recovery, and parse-budget timeouts |
| meta/custom-cap | major | major | major | major | high | K | K | K | K | K | Custom-query hits over the per-file work cap; results are partial |
| meta/invalid-suppression | major | major | major | major | high | K | K | K | K | K | Suppression directives that name unknown rules or lack a reason |
| meta/unused-suppression | minor | minor | minor | minor | high | K | K | K | K | K | Suppression directives that match nothing |
| custom/* | spec | spec | spec | spec | spec | K | K | K | K | K | Tree-sitter queries from configuration with per-rule severity |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for building, testing, adding rules, and commit conventions.

The repository lints itself: CI runs `sloplint check crates --format agent --strictness standard --mode threshold --fail-on minor` from the release binary. Run the same command locally before opening a pull request.

## License

Apache-2.0; see [LICENSE](LICENSE).
