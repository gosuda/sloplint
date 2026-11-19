# tree-sitter-ocaml (vendored by sloplint)

## Origin

- Published crate: `tree-sitter-ocaml` 0.26.0 (crates.io). Name and version are unchanged.
- Upstream: https://github.com/tree-sitter/tree-sitter-ocaml.
- License: MIT (`LICENSE` is the upstream file, fetched verbatim).
- Files kept from the crate: `Cargo.toml` (normalized by cargo), `LICENSE`, `bindings/`, `common/`, `grammars/` (grammar.js patched), `queries/`, `tree-sitter.json`.
- `Cargo.toml` has one addition: an empty `[workspace]` table after `[lib]`. Without it `cargo fmt --all` reports the vendored package as belonging to the parent workspace and fails. No other manifest change.
- Files dropped: `Cargo.toml.orig`, `Cargo.lock`, `.cargo_vcs_info.json`, `.cargo-ok`.
- The crate ships three grammars — `grammars/ocaml` (scope `source.ocaml`, `.ml`), `grammars/interface` (`source.ocaml.interface`, `.mli`), and `grammars/type` (`source.ocaml.type`). `interface` and `type` `import` the ocaml `grammar.js`, so all grammar edits live in `grammars/ocaml/grammar.js` and all three `src/` trees must be regenerated together.
- `ocaml.so` at the crate root is the compiled `grammars/ocaml` parser (built with `tree-sitter build`); it exists for external tool dogfooding only — the Rust crate compiles all three parsers statically via `bindings/rust/build.rs`.

## Why

The published grammar reports a parse error on valid OCaml written for camlp5-style preprocessors; a parse error can hide most of a file's lint findings. The largest corpus cluster is HOL Light (camlp5 quotations everywhere): against the corpus the published grammar errors on ~881 of 980 `.ml` files in github-hol-light; the patched grammar errors on ~831 files by raw `tree-sitter parse` (825 files carry `meta/parse-error` findings under `sloplint check`) — the residual is documented below.

## Regeneration

`src/` is regenerated with tree-sitter-cli 0.27.x (`tree-sitter-0.27.1 generate`); the grammar uses `reserved`, which CLI 0.24 cannot emit. Run `generate` inside each of `grammars/ocaml`, `grammars/interface`, and `grammars/type`. `grammar.js` is the only patched input; `src/grammar.json`, `src/node-types.json`, and `src/parser.c` are regenerated artifacts and are not part of `sloplint-grammar.patch`.

## Changes (`grammars/ocaml/grammar.js`, see `sloplint-grammar.patch`)

Each change is an OCaml construct the published grammar rejects.

1. **camlp5 tagged quotations.** `<:expr< ... >>`, `<:patt< ... >>`, `<:ctyp< ... >>`, `<:str_item< ... >>`, `<:sig_item< ... >>`, `<:class_expr< ... >>`, `<:vala< ... >>`, `<:module_expr< ... >>`, `<:module_type< ... >>`, `<:class_str_item< ... >>` — any `<:ident< ... >>` parses as an opaque `quotation` node with nested quotations allowed. `quotation` is an alternative wherever a camlp5 quote can appear: `_structure_item`, `_signature_item`, `_module_type`, `_type`, `_simple_expression`, `_simple_pattern`.
2. **Backtick term quotations.** `` `?n. n + 0 = n` ``-style quotes parse as an opaque `term_quotation` token — but only when the first character cannot start an identifier or raw identifier. `` `foo `` is lexically identical to a polymorphic-variant tag (`match x with `A -> 1`), so quotes that open with a letter (Unicode included, since the grammar's identifiers are Unicode-aware), `_`, `'`, or `\` (raw-identifier tags like `` `\#let ``) are deliberately not covered; digits and punctuation do open a quotation.
3. **Quotation as a binding pattern.** `let <:patt< `A >> = f 1` — `_simple_binding_pattern` admits `quotation`/`term_quotation`, matching `_simple_pattern`.
4. **Tag charset widened** to `[a-zA-Z_][a-zA-Z_0-9.:]*` — location-binding tags (`<:expr:<`), module-qualified tags (`<:m.n<`), and registered custom expanders (`<:xhtml<`, `<:sexp_of<`, `<:reloc_expr<` all occur in the corpus).
5. **Declared conflicts** for the ambiguity the quotation alternatives introduce: `[$._structure_item, $._simple_expression]`, `[$.let_binding]`, `[$.class_binding]`, `[$.application_expression]`, `[$._signature_item, $._type]`, `[$._class_expression, $.class_application]`, `[$.class_application]`.

## Known remaining gaps

- **Bare `<<...>>` quotations are not supported.** `<<` is a legal custom infix operator in real OCaml (bap defines `let (<<) p q = ...` and uses `p << q`), so a `<<` quotation opener regresses valid operator code. ~40 hol-light files use bare `<<`.
- **Identifier-opening backtick quotes** (`` `foo ...` ``) are textually identical to polymorphic-variant tags; longest-match token stealing regresses every `match` arm in standard OCaml (measured: bap 0→240). Documented camlp5 ceiling — these need real preprocessing.
- **`expr <:tag< expr` infix chains** (`a <:x< b`) collide with the quotation opener when a `>>` follows later — custom `<:`-prefixed operators are a documented ceiling (zero occurrences in the ~4,200-repo corpus; bap-style `<<`/`>>` ops are unaffected).
- Preprocessor-driven code (`#<If:...>`/`#<End>` mpp directives in opalang) still errors, same as upstream.
- hol-light residual: ~831/980 `.ml` files by raw parse, almost all ident-opening term quotations and bare `<<`.
- Standard-OCaml spot checks match upstream exactly: yojson 6/43, ocamlformat 42/350, batteries-included 64/216, opalang 79/407, lwt 0/83, owl 0/301, bap 0/533, mldonkey 1/392, atd 1/162.
