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
5. **Declared conflicts** for the ambiguity the quotation alternatives introduce: `[$._structure_item, $._simple_expression]`, `[$.let_binding]`, `[$.class_binding]`, `[$.application_expression]`, `[$._signature_item, $._type]`, `[$._class_expression, $.class_application]`, `[$.class_application]`, `[$._camlp4_any_directive, $.camlp4_conditional]`, `[$._match_cases]`.
6. **cppo directives** (`#define`, `#include`, `#if`, `#ifdef`, `#endif`, `#else`, `#elif`, `#undef`, `#error`, `#warning`, `#pragma`, `#line`, `#ext`, `#endext`, `#stop`, `#localization`). One opaque `cppo_directive` token swallows the whole line, including a `\`-newline line continuation (`#define FOO a \ <newline> b c`). Valid at structure/signature level and inside `_match_cases`, where directives may appear before `|`, between `|` and its arm, and after the last arm (`#ifdef STRINGLIT | `Stringlit _ -> ... #endif` in yojson's `util.ml`). Inside expressions `#` remains the method-call operator (`obj #m`), so cppo directives are deliberately not expression-level.
7. **camlp4 directives and conditional sections.** `#<Name>` / `#<Name:args>` / `#<Name args>` (`#<If:TESTING>`, `#<Else>`, `#<Ifstatic:OCAML_VERSION_MAJOR 4>`, `#<If$minlevel 10>`) parse as an opaque `camlp4_directive` token; `#<End>` is a dedicated higher-precedence `camlp4_end_directive` token so a conditional unambiguously terminates. A `#<X> expr #<Y> expr #<End>` run reduces to `camlp4_conditional`, which is an expression (`let to_string = #<If:TESTING> a #<Else> function | ... #<End>` in opalang's `ident.ml`). Arm expressions are required — a `#<If>` arm whose body is itself a conditional parses unambiguously nested (`#<If:A> #<If:B> x #<End> #<Else> y #<End>`). Bare directives also stand alone at structure/signature level. `_match_cases` accepts only `cppo_directive` separators: a `#<Else>` shifted into the case list would steal the arm separator of an enclosing conditional.
8. **opa/qml bypass forms.** `##name args` bypass directives and `#<< ... >>#;` bypass quotations (opalang) parse as opaque `opa_directive` / `opa_quotation` tokens; `opa_quotation` is also a `_simple_expression` and a `_parameter`, where opa splices instrumentation (`let plot #<< debugid >># prefix = ...`). `opa_directive` requires an identifier char after `##` so Js_of_ocaml-style `##.` can never lex as a directive.

## Known remaining gaps

- **Bare `<<...>>` quotations are not supported.** `<<` is a legal custom infix operator in real OCaml (bap defines `let (<<) p q = ...` and uses `p << q`), so a `<<` quotation opener regresses valid operator code. ~40 hol-light files use bare `<<`.
- **Identifier-opening backtick quotes** (`` `foo ...` ``) are textually identical to polymorphic-variant tags; longest-match token stealing regresses every `match` arm in standard OCaml (measured: bap 0→240). Documented camlp5 ceiling — these need real preprocessing.
- **`expr <:tag< expr` infix chains** (`a <:x< b`) collide with the quotation opener when a `>>` follows later — custom `<:`-prefixed operators are a documented ceiling (zero occurrences in the ~4,200-repo corpus; bap-style `<<`/`>>` ops are unaffected).
- **Directives inside application argument lists** (`f #<Ifstatic:X> ~labeled:arg #<End> arg`) — letting directives into `_argument` mis-resolves `#<End>` arms and regresses conditionals (measured: opalang objectFiles.ml 0→150 errors); needs real preprocessing to know which arm survives.
- **opa splices in arbitrary keyword positions** (`module M (P : T) #<< (X : T) >># = struct` — a quotation between the functor parameter list and `=`); 2 residual errors in opalang's debugTracer.ml/.mli.
- **camlp4 `parser`/`lexer` stream syntax** (`let aux = parser | [< e = p; s >] -> ... | [< >] -> ...`) — a separate stream-pattern grammar; ~3 opalang files (jsParse, indexer).
- **camlp4 markers inside sequence/list elements** (`[ "a"; #<Ifstatic> "b"; #<End> "c" ]` and `Printf f; #<End> let x = ... in`) — `#<End>` as a mid-sequence separator conflicts with its role as conditional terminator; ~3 opalang files (qml2ocamlOptions, qmlCpsServerLib).
- hol-light residual: ~831/980 `.ml` files by raw parse, almost all ident-opening term quotations and bare `<<`.
- Spot checks after the directive round (files with `meta/parse-error` findings): opalang 10 `.ml`/`.mli`, yojson 1, batteries-included 29 (mostly `[? ... ?]` comprehension), ocamlformat 44 (mostly `test/failing/` intentionally-broken fixtures), lwt 0, bap 0.
