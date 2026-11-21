# tree-sitter-quint (vendored by sloplint)

Date: 2026-11-19.

## Origin

- Upstream: `https://github.com/LegacyCodeHQ/tree-sitter-quint`, revision
  `f3b26e68c4f9a9b1153f7198c23c628c57b6dece` (HEAD `main`,
  2026-11-06, `fix: give Zed the scopes its bracket pairs are disabled in`),
  which is the commit published as npm `0.33.0`.
- Licence: Apache-2.0 (`LICENSE` is the upstream file, fetched verbatim;
  `NOTICE` is kept verbatim beside it).
- Files kept from the upstream tree: `Cargo.toml` (plus one empty
  `[workspace]` table after `[lib]`; without it `cargo fmt --all` reports
  the vendored package as belonging to the parent workspace and fails. No
  other manifest change), `LICENSE`, `NOTICE`, `grammar.js`,
  `grammar/definition-boundaries.js` (imported by `grammar.js`; required to
  regenerate), `bindings/rust/{build.rs,lib.rs}`, `queries/*.scm`,
  `src/` (`parser.c`, `scanner.c`, `quint_033_atn.h`, `quint_boundaries.h`,
  `quint_scanner_boundary_state.h`, `grammar.json`, `node-types.json`,
  `tree_sitter/parser.h`), `test/corpus/*.txt`, `tree-sitter.json`.
- Files dropped: `README.md`, `package.json`, `binding.gyp`, `Makefile`,
  `CMakeLists.txt`, `build.zig`, `build.zig.zon`, `go.mod`, `pom.xml`,
  `pyproject.toml`, `setup.py`, `bun.lock`, `AGENTS.md`, `DEVELOPMENT.md`,
  `docs/`, `editors/`, `scripts/`, `test/{audit,benchmark,fixtures,highlight,query,reference-quint033,scanner}`,
  and the non-Rust bindings (`bindings/{c,go,java,node,python,swift,zig}`).
  None of these is read by the Rust build, the corpus runner, or regeneration.
- `sloplint-grammar.patch` is empty: `grammar.js` and
  `grammar/definition-boundaries.js` are vendored verbatim, so the diff
  versus the upstream revision is zero. If a future fix touches the grammar,
  the patch must hold exactly that diff.
- `src/quint_boundaries.h` carries one local deviation: `max_align_t
  data[]` in `QArena` is replaced by a `QAlign` union (`long long`,
  `long double`, `void *`, `void (*)(void)`) and `q_allocate` uses
  `sizeof(QAlign)` for the stride. MSVC's C mode has no `max_align_t` even
  under `/std:c11` (`error C2061`), which broke every Windows lane. The
  union still covers the widest alignment the memo/frame/result payload
  types need; on i386 it gives a weaker guarantee than `max_align_t`
  (no 16-byte SIMD alignment), which none of the current payload types
  require. A refresh from the pinned upstream revision must re-apply
  this change.
- The npm tarball `@legacycodehq/tree-sitter-quint@0.33.0` (`dist-tags`:
  `latest 0.33.0`) carries the same `parser.c` byte for byte, but its
  `quint_scanner_boundary_state.h` still uses `qsort` where the pinned git
  revision uses the vendored `q_sort` heap sort (commit `7a2e2cd`, Zed Wasm
  has no `qsort`), and its `queries/{highlights,indents,outline}.scm` predate
  the `assumption_declaration` highlight, `@end` indent, and
  `module_{import,export}_declaration` outline rows. The vendored tree follows
  the pinned git revision, not the tarball.

## How src/ is generated

- Tool: tree-sitter-cli 0.27.0 (upstream `package.json` devDependencies).
  Upstream regeneration is `bun run generate` (`tree-sitter generate`) with
  `node scripts/generate-reference-atn.mjs --check` asserting the generated
  tree matches `src/`.
- sloplint did not regenerate: `grammar.js` is verbatim, so the committed
  `src/parser.c` (4,529,130 bytes, `LANGUAGE_VERSION 15`, `STATE_COUNT 3886`,
  `EXTERNAL_TOKEN_COUNT 14`) is the upstream artifact. Verified 2026-11-19:
  running `tree-sitter generate` with CLI 0.27.1 in a scratch copy of this
  directory reproduces `src/` byte for byte (only `src/tree_sitter/*.h`
  are rewritten, identically), so the committed tree is also what the newer
  CLI emits.
- `src/scanner.c` is a C external scanner plus three header files the
  scanner includes (`quint_033_atn.h`, `quint_boundaries.h`,
  `quint_scanner_boundary_state.h`); all four are kept, and
  `bindings/rust/build.rs` (upstream, verbatim) compiles `parser.c` and
  `scanner.c` with `cc`, `std("c11")`, and `-utf-8` on MSVC.
- Quirks: there are no tags or releases on the grammar repository; the
  release channel is npm dist-tags (`latest 0.33.0`);
  `tree-sitter.json` also accepts the `.quint` extension, but sloplint
  routes only the language's standard `.qnt` extension
  (https://quint-lang.org/docs/content/docs/lang).

## Construct mapping (Quint to linter concepts)

Quint is a specification language; the mapping below is what the sloplint
Quint adapter (`crates/sloplint-syntax/src/lang/quint.rs`) reads. No new
rule and no new concept was added for Quint.

| Quint construct | Linter concept |
|---|---|
| `operator_definition` (`def`, `pure def`, `action`, `temporal`, `run`, `nondet` via the `qualifier` field) | Function-like unit (`Function`) |
| `value_definition` (`val`, `pure val`) | Function-like unit (`Function`) |
| `lambda_expression` | Closure (`Closure`) |
| `parameter`, `annotated_parameter` | Parameter (`Param`, with `int`/`str`/`bool` primitive kinds) |
| `if_expression` | Branch (`If`) |
| `match_expression`, `match_arm` (wildcard `_` is the `hole` variant) | Branch (`Match`, `MatchArm`) |
| `all_expression`, `any_expression` (n-ary choice/conjunction blocks) | Loop (`Loop`; the unbounded-branching construct where the linter counts nesting) |
| `block_expression` nondet bindings | Flow-scoped names (quantified names bind like loop targets) |
| `call_expression`, `ufcs_call_expression` | Call (`Call`) |
| `module_import_declaration`, `named_import_declaration`, `wildcard_import_declaration`, `instance_declaration` (parameterized instantiation) | Import (`Import`; wildcard kind preserved) |
| `module_export_declaration`, `named_export_declaration`, `wildcard_export_declaration` | Export (`Export`, re-export facts) |
| `type_alias_declaration`, `uninterpreted_type_declaration` | Type definition (`TypeDef`) |
| `variable_declaration` | Mutable global declaration (`Decl` + `GlobalMutable` ambient) |
| `constant_declaration` | Declaration (`Decl`) |
| `assignment_expression` (target is always `primed_identifier`) | State write (`Write`) |
| `record_literal` / `record_literal_field` | Struct literal (`StructLit`, single-source copies only) |
| `comment`, `documentation_comment` | Comment (`Comment`; `///` feeds `echo-comment` doc facts) |
| `string_literal` | String (`String` arg shape) |

Quint has no visibility syntax and every definition is importable by name,
so every function-like unit and type alias is `Exported`. That lets
`grep/generic-name`, `grep/homonym-definitions`, `agent/stringly-api`, and
`complexity/boolean-params` fire on Quint the way they do on other
languages.

Quint has no logging primitives and no `print` builtin, so the `quint`
log-callee row is `print`: it matches only a user-defined debug operator of
that name with consecutive literal arguments, and `grep/contiguous-log-literal`
otherwise stays silent.

Unmapped (left silent on purpose; a rule that needs one of these never
fires on Quint): definition qualifiers (`pure`, `action`, `temporal`, `run`
carry no call-graph meaning the linter counts); `assumption_declaration`
(proof obligation, not code); `and_block_expression` /
`or_block_expression` (pure boolean-operator blocks, no choice); nondet
binding values; the `_type` family beyond parameter/return text;
scalar/pair/tuple/list literals; `field_access_expression`,
`index_expression`, `namespace_access_expression` (member paths, not calls);
module paths (no path-based barrel chaining); `hashbang`, `chain_break`,
`operator_break`, `hole` outside match arms, `invalid_*` recovery nodes.
Silent rules: `grep/contiguous-log-literal` (no logging),
`agent/informative-assertions` (no `assert`), `grep/no-dynamic-symbol` (no
dynamic lookup), `slop/zombie-catch` (no catch), `complexity/mutable-span`
(mutation is module-level state, covered by `agent/ambient-global-state`
instead of function-scoped spans).

## Corpora (upstream, run from this directory with tree-sitter-cli 0.27.x)

- `test/corpus`: 24 files, all pass under `tree-sitter test`.
- `test/fixtures/benchmark` (not vendored): 348 `.qnt` programs behind
  `bun run test:audit:quick`; the grammar tracks the Quint 0.33.0 compiler
  one to one (`Package versions track Quint compiler releases directly.
  Version 0.33.0 supports Quint 0.33.0.`).

## Known limits

- The grammar is expression-oriented: `if` is `if_expression` (no
  statement form, no `else if` node), and there is no `for`/`while`
  statement node; repetition is `all`/`any` blocks and recursion is a
  `call_expression` self-reference (see the mapping table).
- `match` on non-sum types with a `_` arm parses but is not flagged: the
  adapter reports a wildcard arm only when sibling arms carry
  constructor-like variants.
- Upstream carries no `TODO`/`FIXME` in `grammar.js`.
