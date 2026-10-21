# tree-sitter-zig (vendored by sloplint)

## Origin

- Published crate: `tree-sitter-zig` 1.1.2 (crates.io). Name and version are unchanged.
- Upstream: https://github.com/tree-sitter-grammars/tree-sitter-zig, tag `v1.1.2`.
- License: MIT (`LICENSE` is the upstream v1.1.2 file, fetched verbatim).
- Not shipped in the crate: the upstream `test/corpus`. A GitHub API lookup of that path at `v1.1.2` returned 404, so the upstream corpus was not run. The only corpus that passes here is `test/corpus/sloplint_*.txt`.
- Files kept from the crate: `Cargo.toml` (normalized by cargo), `README.md`, `bindings/`, `grammar.js` (patched), `queries/`, `src/` (regenerated), `tree-sitter.json`, `LICENSE`.
- `Cargo.toml` has one addition: an empty `[workspace]` table after `[lib]`. Without it `cargo fmt --all` reports the vendored package as belonging to the parent workspace and fails. No other manifest change.
- Files dropped: `Cargo.toml.orig`, `.cargo_vcs_info.json`, `.cargo-ok`.

## Why

The published grammar reports a parse error on valid Zig. sloplint skips a whole file on any error node, so each error hides the whole file. Against the Zig stdlib corpora in `.outline/corpora` the published grammar errors on 39 of 1203 files: 37 in `zig` (master) and 2 in `zig-0.14-lib`.

## Regeneration

`src/` is regenerated with tree-sitter-cli 0.27.1 (`~/lint/complexity-linter/.outline/tools/tree-sitter-cli/bin/tree-sitter generate`). Two effects follow from regenerating, independent of the patch:

- `LANGUAGE_VERSION` moves from 14 to 15.
- `node-types.json` marks the `comment` kind `"extra": true`.

Regenerating the unpatched grammar also changes the tree of 5 files that parse clean under the published crate. The change is in how `N..(expr)` is parsed inside a `for` header. The published parse reads `(expr)` as call arguments on a range (`call_expression` with a `range_expression` callee). The regenerated parse reads `(expr)` as the range's right operand. Example: `for (0..(a.len - 1) / Scan.size)` in `zig-0.14-lib/std/mem.zig:780`. Neither parse matches Zig's precedence, where the range covers the whole division. The files:

- `zig/lib/std/crypto/ff.zig`
- `zig/lib/std/hash/xxhash.zig`
- `zig-0.14-lib/std/crypto/ff.zig`
- `zig-0.14-lib/std/hash/xxhash.zig`
- `zig-0.14-lib/std/mem.zig`

Comparing the regenerated unpatched grammar with the patched one over all 1203 files, the trees that differ are exactly the 39 files the published grammar rejected. No other file's tree changes under the patch.

## Changes (`grammar.js`, see `sloplint-grammar.patch`)

Each change is a Zig construct that the published grammar rejects.

1. **Z-C, bare error set as a container field.** `container_field` accepts `error_set_declaration` as its name alternative, so `struct { error{foo} }` parses. Zig 0.14.1 accepts this.
2. **Z-A, struct-literal asm clobbers.** `asm_clobbers` accepts `.{ .name = expr, ... }` as well as a string list, so `: .{ .memory = true }` parses. Only `.name = expr` items are accepted; `:: .{ foo }` still errors.
3. **Z-B, `async` and `await` as identifiers.** `identifier` accepts the bare tokens `'async'` and `'await'`. Two conflicts are declared: `[$.async_expression, $.identifier]` and `[$.await_expression, $.identifier]`. The bare tokens keep the identifier node flat; an aliased form nested an extra `identifier` node and changed trees of valid code.

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

Corpus cases: `test/corpus/sloplint_*.txt`, 6 cases, all passing (`tree-sitter test`). Regression tests: `crates/sloplint-syntax/tests/grammar_gaps/zig.rs`, 14 tests. Against the vendored grammar all 14 pass. Against the published crate the 8 gap tests fail and the 6 control and invalid-input tests pass, which is the intended fail proof.

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
- Upstream corpus not run; see Origin.
- The 5 shape changes listed under Regeneration.

## Reproduce

- Build the tests: `cargo test -p sloplint-syntax --test grammar_gaps`. The root file `crates/sloplint-syntax/tests/grammar_gaps.rs` is a single `#[path]` module for `grammar_gaps/zig.rs`.
- Corpus: `TREE_SITTER_LIBDIR=<fresh dir> tree-sitter test` inside `vendor/tree-sitter-zig`. Use a fresh `TREE_SITTER_LIBDIR`; the CLI caches compiled grammars per language name.
- Regenerate: `tree-sitter generate` inside `vendor/tree-sitter-zig`.
