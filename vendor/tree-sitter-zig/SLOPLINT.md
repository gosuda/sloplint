# tree-sitter-zig (vendored by sloplint)

## Origin

- Published crate: `tree-sitter-zig` 1.1.2 (crates.io). Name and version are unchanged.
- Upstream: https://github.com/tree-sitter-grammars/tree-sitter-zig. Base moved
  from tag `v1.1.2` (b670c8df) to master `6479aa13f32f701c383083d8b28360ebd682fb7d`
  (2025-09-10), which adds the `arguments` node to `call_expression` that
  `crates/sloplint-syntax/src/lang/zig` already reads. No release since
  v1.1.2; master is the newest upstream tree.
- Upstream fix: PR #43 ("feat: bitpack", open since 2026-11-11, head
  5ce23f05bd852efcc00a352db1314e6263d8125f). Its grammar commit
  a940925bbc91089ea8857e37d37657c1f07807ac (Robbie Lyman, 2026-11-10) is
  applied verbatim (the four `bitpack` declaration lines); its queries
  commit 5ce23f05 (same author, adds the `bitpack` keyword, drops two
  duplicated operators) updates `queries/highlights.scm`. Both cherry-picked
  with upstream authorship in a scratch clone.
- License: MIT (`LICENSE` is the upstream master file, fetched verbatim).
- Not shipped upstream: `test/corpus` is absent at master too, so the only
  corpus that runs here is `test/corpus/sloplint_*.txt`.
- Files kept from the upstream tree: `Cargo.toml` (normalized by cargo),
  `README.md`, `bindings/rust/`, `grammar.js` (patched), `queries/`,
  `src/` (regenerated), `tree-sitter.json`, `LICENSE`.
- `Cargo.toml` has one addition: an empty `[workspace]` table after `[lib]`. Without it `cargo fmt --all` reports the vendored package as belonging to the parent workspace and fails. No other manifest change.
- Files dropped: `Cargo.toml.orig`, `.cargo_vcs_info.json`, `.cargo-ok`.

## Why

The published grammar reports a parse error on valid Zig. sloplint skips a whole file on any error node, so each error hides the whole file. Against the Zig stdlib corpora in `.outline/corpora` the published grammar errors on 39 of 1203 files: 37 in `zig` (master) and 2 in `zig-0.14-lib`.

## Regeneration

`src/` is regenerated with tree-sitter-cli 0.27.1 (`tree-sitter generate` in
this directory). `LANGUAGE_VERSION` is 15 at master and stays 15.
`node-types.json` marks the `comment` kind `"extra": true`.

Regenerating the unpatched grammar also changes the tree of 5 files that parse clean under the published crate. The change is in how `N..(expr)` is parsed inside a `for` header. The published parse reads `(expr)` as call arguments on a range (`call_expression` with a `range_expression` callee). The regenerated parse reads `(expr)` as the range's right operand. Example: `for (0..(a.len - 1) / Scan.size)` in `zig-0.14-lib/std/mem.zig:780`. Neither parse matches Zig's precedence, where the range covers the whole division. The files:

- `zig/lib/std/crypto/ff.zig`
- `zig/lib/std/hash/xxhash.zig`
- `zig-0.14-lib/std/crypto/ff.zig`
- `zig-0.14-lib/std/hash/xxhash.zig`
- `zig-0.14-lib/std/mem.zig`

The paragraph above was measured on the v1.1.2 base. After the base move to
master, every call expression gains the upstream `arguments` wrapper node by
design, so tree-identity with the old base no longer holds anywhere a call
appears; the 1203-file corpora are not present in this checkout, so the
error-count sweep was not re-run. What is proved instead: the 8 sloplint
corpus cases and the 14 `grammar_gaps/zig.rs` tests pass on the new tree,
and the adapter (which reads `arguments`) is aligned with it.

## Changes (`grammar.js`, see `sloplint-grammar.patch`)

The patch is the diff against upstream master `6479aa1`. Item 4 below is
upstream PR #43 (commit a940925), kept verbatim; the rest is the retained
self-patch. One more upstream change arrives with the base move and is not
in the patch: `call_expression` on master wraps call args in
`field('arguments', $.arguments)` (commit 6479aa1), which is the node the
adapter reads; the `sloplint_bitpack_identifier.txt` expectation was updated
for it.

1. **Z-C, bare error set as a container field.** `container_field` accepts `error_set_declaration` as its name alternative, so `struct { error{foo} }` parses. Zig 0.14.1 accepts this.
2. **Z-A, struct-literal asm clobbers.** `asm_clobbers` accepts `.{ .name = expr, ... }` as well as a string list, so `: .{ .memory = true }` parses. Only `.name = expr` items are accepted; `:: .{ foo }` still errors.
3. **Z-B, `async` and `await` as identifiers.** `identifier` accepts the bare tokens `'async'` and `'await'`. Two conflicts are declared: `[$.async_expression, $.identifier]` and `[$.await_expression, $.identifier]`. The bare tokens keep the identifier node flat; an aliased form nested an extra `identifier` node and changed trees of valid code.
4. **Z-D, `bitpack` container declarations (upstream PR #43).** `struct`, `union`, `enum`, and `opaque` declarations accept `'bitpack'` where they already accepted `'packed'`. Zig 0.18 renamed `packed` to `bitpack`; during the migration both spellings appear in real code (e.g. arocc `bitpack struct(u32)`), so the grammar accepts both. `identifier` also accepts the bare `'bitpack'` token (retained hunk, not in PR #43), so `bitpack` stays usable as a plain name in code written for older zig releases (same over-acceptance as Z-B).
Controls that already parsed and still parse: `(-> T)` asm outputs, `[_]` output names, multiline `\\` templates with string clobbers.

## Results

| Corpus | Files | Published errors | Patched errors | Clean to error |
|---|---|---|---|---|
| `zig` (master stdlib) | 555 | 37 | 0 | 0 |
| `zig-0.14-lib` | 648 | 2 | 0 | 0 |
| total | 1203 | 39 | 0 | 0 |

Per-class, by error site in the published parse:

- Z-A, struct-literal clobbers: 30 files. The clobber-only edit alone clears 27 of them. `sha2.zig`, `mem.zig` and `Io/Kqueue.zig` need more than that edit. `Kqueue.zig` also uses `async`/`await` identifiers (Z-B). I did not isolate why `sha2.zig` and `mem.zig` need the full patch.
- Z-B, `async`/`await` identifiers: 6 files. `builtin.zig`, `Io.zig`, `Io/IoUring.zig`, `Io/Threaded.zig`, `zig/llvm/Builder.zig`, and `Io/Kqueue.zig` (also counted under Z-A).
- Z-C, bare error set field: 4 files. `zon/parse.zig` and `zon/Serializer.zig` (master), `zon/parse.zig` and `zon/stringify.zig` (0.14-lib).

Zig 0.14.1 `ast-check` exit 0 on the two `zig-0.14-lib` zon files that the Z-C fix clears (`std/zon/parse.zig`, `std/zon/stringify.zig`), so that fix is version-matched.

Corpus cases: `test/corpus/sloplint_*.txt`, 8 cases, all passing (`tree-sitter test`). Regression tests: `crates/sloplint-syntax/tests/grammar_gaps/zig.rs`, 14 tests. Against the vendored grammar all 14 pass. Against the published crate the 8 gap tests fail and the 6 control and invalid-input tests pass, which is the intended fail proof. (The table above was measured on the v1.1.2 base and is kept as history; the base move to master was not re-swept over the 1203 files.)

## Validator evidence (zig 0.14.1 `ast-check`)

| Input | zig 0.14.1 | Grammar |
|---|---|---|
| `struct { error{foo} }` | accepts | accepts (Z-C) |
| `struct { x: error{bar}, y: u8 }` | accepts | rejects (residual, below) |
| `.async = async`, `async,` enum tag, `async: *const fn` | rejects | accepts (Z-B, over-acceptance) |
| `.await = await` | rejects | accepts (Z-B, over-acceptance) |
| `::: .{ .memory = true }` | rejects | accepts (Z-A, over-acceptance) |
| `:: .{ foo }` | rejects | rejects |
| `const v = await x;` in a plain fn | rejects | accepts (also accepted by the published grammar) |
| `1e1f`, `1e1x`, `0b102`, `0o8`, `1__0`, `1.0e` | rejects | rejects |
| `1e1`, `1.5e3` | accepts | accepts |
| asm `(-> u64)` output, `[_]` output, multiline template with string clobber | accepts | accepts |

## Over-acceptance (documented, accepted on purpose)

Zig master accepts the forms that Zig 0.14.1 rejects, and sloplint parses master in practice, so the following are accepted:

- Z-A: any `.name = expr` list as asm clobbers, which 0.14.1 rejects.
- Z-B: `async` and `await` as identifiers, which 0.14.1 rejects because they are keywords there.
- `await expr`: accepted by both the published and the patched grammar, and rejected by 0.14.1.

## Residuals (not fixed)

- Error set as a typed field, `struct { x: error{bar} }`. Zig 0.14.1 accepts it; the grammar rejects it. This is a different production from the bare field fixed by Z-C, so it was not in scope.
- Prefix `!` on a value misparses silently: `const x = !y;` yields `(variable_declaration (identifier) (error_union_type ok: (identifier)))` with no ERROR node, and `const z = !f(a);` puts the `error_union_type` in the callee position. Verified on the new tree this lane (`tree-sitter parse` of the two lines). This matches upstream issues #7/#42; no upstream fix exists, so it stays documented.
- Upstream corpus not run; see Origin.
- The 5 shape changes listed under Regeneration.

## Reproduce

- Build the tests: `cargo test -p sloplint-syntax --test grammar_gaps`. The root file `crates/sloplint-syntax/tests/grammar_gaps.rs` is a single `#[path]` module for `grammar_gaps/zig.rs`.
- Corpus: `TREE_SITTER_LIBDIR=<fresh dir> tree-sitter test` inside `vendor/tree-sitter-zig`. Use a fresh `TREE_SITTER_LIBDIR`; the CLI caches compiled grammars per language name.
- Regenerate: `tree-sitter generate` inside `vendor/tree-sitter-zig`.
