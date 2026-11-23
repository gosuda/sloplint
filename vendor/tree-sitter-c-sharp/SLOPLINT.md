# tree-sitter-c-sharp, patched for sloplint

Date: 2026-11-23 (re-vendor; first vendored 2026-10-20).

Upstream v0.24.0 absorbs the ref-partial, pointer-deref, methodelem,
extension, `#:`-directive and dangling-else hunks; the kept patch is the
11-case self-patch (allows-struct, ref-conditional assignment, U+202F
trivia, unsafe destructors, async identifier, `#if`-split bodies,
consts, accessors, ctor initializers, base/argument lists, class/method
heads) plus the `#line` filename fix cherry-picked from upstream master
(`77145f6`). Node renames versus the old tree: `extension_block` is now
upstream's `extension_declaration`/`extension_body`/`receiver_parameter`,
and `preproc_colon` is now upstream's `ignored_directive` (an extra, so
`#:` lines stay invisible in trees).

## Origin

- Upstream repo: https://github.com/tree-sitter/tree-sitter-c-sharp
- Upstream tag: v0.24.0 (commit `d4bbe18103c3380c6454085e589d2610938da142`).
  The published crate's `grammar.js` is byte-identical to the tag, and the
  tag commit matches the crate `.cargo_vcs_info.json` sha1.
- Published crate: tree-sitter-c-sharp 0.24.0. Name and version are unchanged.
- Files copied unchanged from the published crate: `Cargo.toml`,
  `Cargo.toml.orig`, `LICENSE`, `README.md`, `tree-sitter.json`,
  `bindings/rust/lib.rs`, `bindings/rust/build.rs`, `queries/*`,
  `src/scanner.c`, `src/tree_sitter/parser.h`. No `Cargo.lock` is vendored
  (workspace `exclude = ["vendor"]` covers it; same convention as wave-1).
- Cherry-pick (upstream authorship kept in the scratch history):
  `77145f6` ("fix: read `#line` file names without escape sequences",
  master-only past v0.24.0) applied to `grammar.js` and
  `test/corpus/preprocessor.txt`.
- Changed: `grammar.js` (patch below), `src/` regenerated
  (`parser.c`, `src/grammar.json`, `src/node-types.json`;
  `src/tree_sitter/*` and `src/scanner.c` are byte-identical to the crate).
- Added: `test/corpus/` (vendored from the v0.24.0 tag, including the
  cherry-picked `#line` test), `test/corpus/sloplint_cases.txt` (19 cases,
  expectations follow the renames above),
  `sloplint-grammar.patch`, this file.
- The crate's `Cargo.lock` is the only intentional omission versus
  crate+tag.

## How src/ is generated

- Tool: tree-sitter-cli 0.27.1
  (`/dev/shm/GrammarLaneCsharp/bin/tree-sitter`, copied from the audit
  lane's pinned CLI; the host default 0.24.7 rejects ABI 15).
- Command, run in this directory: `tree-sitter generate`
  (JS runtime: node v26.13.1).
- Patched: STATE_COUNT 11221. Three generate rounds failed first on
  unresolved GLR conflicts where the kept hunks meet new upstream rules;
  each was resolved with the conflict entry the CLI suggested
  (`extension_declaration`/`_reserved_identifier`,
  `type_parameter_constraints_clause`/`_reserved_identifier`, and the
  `preproc_if/else/elif_in_top_level` + `_in_initializer` + `_in_class_head`
  triples). Two carried-over conflict entries proved unnecessary on the
  new base and were dropped (`attribute_target_specifier`/`_reserved_identifier`,
  `preproc_if_in_top_level`/`preproc_if_in_expression`/`preproc_if_in_class_head`).

## Grammar.js changes

`sloplint-grammar.patch` (451 lines, 29 hunks) is the unified diff against
the 0.24.0 crate `grammar.js`. Every fix below was validated with dotnet
SDK 10 Roslyn CSC (syntax-only: only CS1xxx counts as invalid);
constructs with any CS1xxx were left failing on purpose (see controls).

Dropped from the old patch because upstream v0.24.0 carries an equivalent:
CS-REFPARTIAL (`ref`/`partial` order; upstream spells it
`optional(seq('ref', optional('partial')))` with no modifier alias),
CS-PTRDEREF (upstream widens only to parenthesized and a guarded
`_pointer_cast_expression`, which parses every kept corpus case),
CS-METHODELEM (upstream lists the attribute-target words in
`_reserved_identifier` itself), CS-EXTBLOCK (replaced by upstream's
contextual `extension_declaration`), CS-SPLITPP dangling-else/post-if
(replaced by upstream's scanner-gated `preproc_if_in_else`), CS-#:-directives
(replaced by `ignored_directive`), and the `type_pattern` pin (upstream
now uses `prec.right(2, ...)`).

Kept, rebased onto v0.24.0 (upstream has no equivalent; each still fails on
the plain upgrade per the 19-input before/after table):

- CS-ALLOWS: `type_parameter_constraint` accepts
  `seq('allows', 'ref', 'struct')`, so `where T : allows ref struct` and
  `where T : notnull, allows ref struct` parse. `allows` was added to
  `_reserved_identifier` so a type literally named `allows` still parses
  as a type. (~27 runtime-libs files.)
- Assignment targets: `assignment_expression.left` also accepts
  `$.invocation_expression` (ref-return calls: `Unsafe.Add(ref x, i) =
  v`), `$.conditional_access_expression` (C# 14 null-conditional
  assignment: `a?.B = v`, `a?[i] = v`), and `$.parenthesized_expression`
  (`(c ? ref a : ref b) = v`); the rule carries `prec.right(PREC.ASSIGN)`.
  `a?[i]` reads already parsed.
- CS-U202F: extras whitespace class gains `\u202F` (Roslyn trivia;
  synthetic case only).
- Unsafe destructors: `destructor_declaration` takes `repeat($.modifier)`
  before the kept `optional('extern')`, so `unsafe ~C() {}` parses.
- CS-SPLITPP, method/property/arrow bodies: `preproc_if_in_function_body`
  (branches: block, `=>`-body, bare `;`, or `expr;`; plus a `=>`-headed
  form for `=> #if` bodies) is accepted as a `_function_body`, as a
  property body, and after `=>`. Upstream's own `_in_function_body`
  variant (constraints prefix) is kept alongside as a union.
- CS-SPLITPP, const initializers: `field_declaration` gains an
  alternative `type name = preproc_if_in_field_value` (branches `expr;`,
  no trailing declarator `;`), for `= #if 10; #else 300; #endif`.
- CS-SPLITPP, accessors: `accessor_list` accepts
  `preproc_if_in_accessor` (branches: modifiers, accessor name, body),
  for `#if`-split get/set.
- CS-SPLITPP, ctor initializers: `preproc_if_in_constructor_initializer`
  (branches: `constructor_initializer`) is accepted, so
  `public C() #if X : this() #else : base() #endif {}` parses.
- CS-SPLITPP, initializer/collection elements: kept for the `#if`-split
  base/argument lists only (`preproc_if_in_base_list`,
  `preproc_if_in_argument_list`, with the separator living inside the
  guard). Upstream's `_collection_items`/`_initializer_items` now cover
  the `{...}`/`[...]` element splits, so the old comma-optional rewrites
  there are dropped.
- CS-SPLITPP, base lists and argument lists: restructured so each slot is
  `type + optional(argument_list)` or the preproc arm, for
  `class X : B #if , I #endif {}` and `F(x #if , y #endif)`.
- CS-SPLITPP, declaration heads: the five type declarations and
  `method_declaration` wrap their initializer as
  `choice($._X_declaration_initializer, alias($.preproc_if_in_class_head /
  _in_method_head, $.preproc_if))` — `#if` arms carry attrs+modifiers+
  keyword+name+partial tail, so `#if`-split class/method heads parse.
- CS-ASYNCIDENT: `async` and `extension` join `_reserved_identifier`, so
  `bool async` parameters, `if (async)`, `(async, x)` tuples and
  `x.async` members parse, and `extension` stays usable as an identifier
  outside declaration position. The lambda/local-function `static`+`async`
  modifier forks keep `prec(1)` on the alias with the narrow conflict
  entries (a dozen `modifier`/`_local_function_declaration`/
  `_lambda_expression_init` entries).
- CS-PREPLINE (from upstream `77145f6`, kept verbatim): `preproc_line`
  file names use `_preproc_file_name` (no escape sequences, aliased to
  `$.string_literal` so existing queries keep working), so Razor
  `#line 2 "..\..\.."` paths parse.
- CS-LAMBDA `(out T x) => e`: unresolved upstream limitation — the
  `(`-position merged LR state statically reduces `out float x` to
  `argument` (`argument` is `prec(1)`, beats `parameter`) before `=>`
  can disambiguate, and no GLR fork forms. `in`/`out`/`ref`/`scoped`/
  `readonly` modifiers parse in every other position (method params,
  call args, tuple elements, non-lambda lambdas); only lambda-position
  `in`/`out` params fail. Verified identical failure on pristine
  upstream v0.24.0 — ceiling, not a sloplint regression.

## Adapter stability

`crates/sloplint-syntax/src/lang/csharp.rs` is unchanged. New node kinds
seen by the adapter: `extension_declaration` (a declaration container;
members inside are ordinary declarations the hooks already walk, exactly
as the old `extension_block` was),
`type_parameter_constraint` (the `allows` form; hooks do not read
constraints), `receiver_parameter`/`extension_body` (walked through
generically), and `preproc_if` in body/initializer/accessor positions,
which is skipped by the fact hooks. `ignored_directive` is an extra and
never appears in trees. One consequence is unchanged: a method whose
whole body is a `#if` split is not recorded as a function (only
block/arrow bodies count). Those files previously contributed no facts
at all, so partial facts strictly dominate.

## Corpus results with tree-sitter-cli 0.27.1 `tree-sitter test`

- New tree (this directory): 213/213 pass — 194 upstream corpus tests
  (including the cherry-picked `#line file names with backslashes` test)
  plus 19 sloplint cases with expectations rewritten for the renames
  above (6 cases: ref-partial, extension, preproc-ctor, preproc-elseless,
  line-path, hash-colon). Zero ERROR/MISSING in any expectation.
- No upstream expectation file changed (the `-u` whitespace-only rewrite
  of 6 upstream files was reverted).

## Differential sweep (raw trees, never analyze-only)

Historical (v0.23.5 era, corpora not in this checkout so not re-run):
all 2767 runtime-libs `src/**/*.cs` plus all 735 Humanizer `**/*.cs`
(3502 files), parsed with the stock registry grammar and with this crate
via throwaway binaries keying only on `root.has_error()`:

- Before: 280 files with parse errors.
- After: 168 files with parse errors.
- Fixed: 112. Clean-to-error regressions: 0.

Superseded by the 19-input before/after table in the lane evidence log
(old vendored 19/19 clean; plain v0.24.0 7 clean / 12 ERROR matching the
audit; new tree 19/19 clean, so no shape that parsed before fails now).

## Constructs known still failing (deliberate; each verified on the old base)

The 3502-file sweep cannot be re-run here (corpora absent), so items
upstream plausibly addressed are annotated NEEDS-RE-SWEEP instead of
being claimed fixed:

- Preview/invalid, kept as regression controls: `safe` members (CS8652
  preview + CS9388), `closed` classes (CS8652 preview), `union`
  declarations (CS1001).
- CS-SPLITPP, modifier splits (`[Attr] #if public #else internal #endif
  sealed class`, ~29 files): balanced and real, but tree-sitter LR(1)
  cannot house the `#if` variant beside the attribute/top-level variants
  without forking every declaration (four attempted shapes, all of which
  conflict or mis-resolve: choice-in-repeat, fixed slots, shared rule,
  modifier-rule membership). Needs a real preprocessor.
- CS-SPLITPP, declaration splits (`public Lock(...) #else internal
  Lock(...) #endif : this()`, ctor/method declarators across `#if`).
- CS-SPLITPP, statement/expression-fragment splits (dangling `else`
  after `#if`, `if` conditions, binary arms, `case`/`return` interiors,
  `();`-level and collection/attribute-argument interiors, namespace-name
  `namespace X[.Y]` splits, `32;`-style value splits outside const
  fields). Representative: CalendarData.Icu `else`, ByteSize.TryParse
  args, PipeReadBufferState args, StreamReadBufferState arity splits,
  Volatile.cs, OperatingSystem.cs returns, ResourceReader.cs namespace
  split, OfType.SpeedOpt `};`, EventListener `{`. NEEDS-RE-SWEEP:
  upstream added `_in_statement_tail` plus collection/initializer `#if`
  machinery that overlaps this list.
- CS-SPLITPP, multi-attribute `#if` branches (`#if X [A] [B] [C]
  #endif`, EnumDehumanizeExtensions): branch-vs-outer repeat ambiguity.
- CS-SPLITPP, accessor attribute-only branches (`#if MONO [Intrinsic]
  #endif` before `get`, RuntimeFeature.NonNativeAot).
- CS-PTRDEREF writes with cast/postfix operands (`*(T*)p = v`,
  `*p++ = v`): reads parse; the write glues `p = s` inside the cast
  value (shift-favor; Roslyn precedence needs climbing, not CFG).
  `(*(T*)p) = v` parenthesized form parses. NEEDS-RE-SWEEP: upstream
  added a guarded `_pointer_cast_expression` explicitly to keep
  `*(T*)p = 1` an assignment to the dereference.
- CS-INTERP `$"{{{f("..")}}}"` (api-access.cs): `{{`-escape + hole +
  inner string needs multi-token lookahead; a pairing prototype regressed
  `$"{{nope}}"` and was reverted. NEEDS-RE-SWEEP: upstream has since
  fixed two adjacent interpolation shapes (#448, verbatim end-quote).
- Disabled-branch invalid code, valid per Roslyn, unfixable blind:
  `?using System;` (EnumHelper.cs), `typeof(ICollection>)`
  (TrimmingTests).
