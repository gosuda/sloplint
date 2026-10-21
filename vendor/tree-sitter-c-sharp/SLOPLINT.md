# tree-sitter-c-sharp, patched for sloplint

Date: 2026-10-20.

## Origin

- Upstream repo: https://github.com/tree-sitter/tree-sitter-c-sharp
- Upstream tag: v0.23.5 (commit `cac6d5fb595f5811a076336682d5d595ac1c9e85`).
  The published crate's `grammar.js` is byte-identical to the tag.
- Published crate: tree-sitter-c-sharp 0.23.5. Name and version are unchanged.
- Files copied unchanged from the published crate: `Cargo.toml`,
  `Cargo.toml.orig`, `LICENSE`, `README.md`, `tree-sitter.json`,
  `bindings/rust/lib.rs`, `bindings/rust/build.rs`, `queries/*`,
  `src/scanner.c`, `src/tree_sitter/parser.h`. No `Cargo.lock` is vendored
  (workspace `exclude = ["vendor"]` covers it; same convention as wave-1).
- Changed: `grammar.js` (patch below), `src/` regenerated
  (`parser.c`, `src/grammar.json`, `src/node-types.json`,
  `src/tree_sitter/array.h`; `scanner.c` is byte-identical to the crate).
- Added: `test/corpus/` (the crate ships no tests; vendored from the
  v0.23.5 tag), `test/corpus/sloplint_cases.txt` (12 cases),
  `sloplint-grammar.patch`, this file.
- The crate's `Cargo.lock` and the upstream repo's `test/` deltas are the
  only intentional omissions/additions versus crate+tag.

## How src/ is generated

- Tool: tree-sitter-cli 0.27.1
  (`.../complexity-linter/.outline/tools/tree-sitter-cli/bin/tree-sitter`).
- Command, run in this directory: `tree-sitter generate`
  (JS runtime: node v26.12.0, the default).
- Regenerating the unpatched upstream grammar with 0.27.1 does not
  reproduce the published `src/` byte for byte: header comment, `array.h`,
  and codegen differ (STATE_COUNT 8058 regenerated vs 8053 published,
  parser.c 29708535 vs 29664333 bytes). One behavioral drift was caught by
  the corpus: `case int when i < 5:` parses as `declaration_pattern` +
  ERROR with a plain 0.27.1 regen but as `type_pattern` + `when_clause`
  with the published `src/`; the latter matches Roslyn, so the patch pins
  it with `prec.right(1, ...)` on `type_pattern` (verified against
  `dotnet`/Roslyn CSC: zero CS1xxx). No upstream expectation file changed.
- Patched: STATE_COUNT 13129, parser.c 52977877 bytes. The growth over the
  pristine regen comes from the assignment-target and dereference
  alternatives below pulling the expression automaton into more states;
  per-hunk attribution showed no single hunk responsible (reserved words,
  extension rule, and misc fixes each add <150 states).

## Grammar.js changes

`sloplint-grammar.patch` is the unified diff against the v0.23.5
`grammar.js`. Every fix below was validated with dotnet SDK 10 Roslyn CSC
(syntax-only: only CS1xxx counts as invalid); constructs with any CS1xxx
were left failing on purpose (see controls).

- CS-ALLOWS: `type_parameter_constraint` accepts
  `seq('allows', 'ref', 'struct')`, so `where T : allows ref struct` and
  `where T : notnull, allows ref struct` parse. `allows` was added to
  `_reserved_identifier` so a type literally named `allows` still parses
  as a type. (~27 runtime-libs files.)
- CS-REFPARTIAL: `_struct_declaration_initializer` accepts `ref` followed
  by `partial` (`public ref partial struct`, `public unsafe ref partial
  struct`, `internal ref partial struct`). Roslyn rejects
  `partial ref struct` (CS1585) but the model accepts both orders; the
  over-acceptance is documented here rather than encoded with a second
  modifier list (FileSystemEntry x3, Utf8JsonReader x3, TypedReference,
  ValueStringBuilder).
- CS-PTRDEREF (reads): `_pointer_indirection_expression` operand widens
  from `$.lvalue_expression` to lvalue, cast, postfix, parenthesized, and
  invocation operands, so `*(T*)x`, `*(a + i)`, `*p++` (reads), `*(T*)&x`,
  and `*f(x)` parse in value positions. Bare `prefix_unary` was tried and
  removed (no corpus case needs `*!x`-style operands). `*p`, `*a.b`,
  `*a[i]` trees are unchanged.
- Assignment targets: `assignment_expression.left` also accepts
  `$.invocation_expression` (ref-return calls: `Unsafe.Add(ref x, i) =
  v`), `$.conditional_access_expression` (C# 14 null-conditional
  assignment: `a?.B = v`, `a?[i] = v`), and `$.parenthesized_expression`
  (`(c ? ref a : ref b) = v`); the rule carries `prec.right(PREC.ASSIGN)`,
  which the `*lvalue =` LR ghost requires. `a?[i]` reads already parsed.
- CS-METHODELEM: `_reserved_identifier` gains the attribute-target words
  that are also valid identifiers (`field`, `method`, `param`,
  `property`, `type`, `typevar`; `event`/`return` stay reserved-only),
  plus one narrow conflict `[attribute_target_specifier,
  _reserved_identifier]`, so `[method]` and `[type, x]` parse as
  collection elements while `[method: X]` still parses as a target.
- CS-EXTBLOCK: new `extension_block` (`extension`, optional type
  parameters, `(type [name])`, where-clauses, `declaration_list`), wired
  into `declaration`, plus `extension` in `_reserved_identifier` with one
  narrow conflict `[extension_block, _reserved_identifier]`.
- CS-U202F: extras whitespace class gains `\u202F` (Roslyn trivia;
  zero corpus hits; synthetic case only).
- Unsafe destructors: `destructor_declaration` takes `repeat($.modifier)`
  before the kept `optional('extern')`, so `unsafe ~C() {}` parses.
- CS-SPLITPP, method/property/arrow bodies: `preproc_if_in_function_body`
  (branches: block, `=>`-body, bare `;`, or `expr;`) is accepted as a
  `_function_body`, as a property body, and after `=>` (the arrow form
  carries its own `;` inside the branches: `=> #if A(); #else B();
  #endif`). Fixes arrow-vs-block splits (RuntimeHelpers.CreateSpan),
  `=>`-with-`#if` bodies (CoverageGapTests.EnumValues), property arrow
  splits (RuntimeFeature), `GetDefaultMessage` bodies,
  `OnCollectionModified`, and `static ()=>` field initializers.
- CS-SPLITPP, const initializers: `field_declaration` gains an
  alternative `type name = preproc_if_in_field_value` (branches `expr;`, no trailing declarator `;`), for `= #if 10; #else 300;
  #endif` (JsonConstants is fixed; NumberFormatting still fails later on
  a statement-level split).
- CS-SPLITPP, accessors: `accessor_list` accepts
  `preproc_if_in_accessor` (branches: modifiers, accessor name, body, no
  leading attributes), for `#if`-split get/set (Decimal.DecCalc).
- Switch `case int when i < 5`: `prec.right(1, ...)` on `type_pattern`
  (see toolchain note above); the now-redundant
  `[tuple_element, type_pattern]` conflict entry was removed.

## Adapter stability

`crates/sloplint-syntax/src/lang/csharp.rs` is unchanged. New node kinds
seen by the adapter: `extension_block` (a declaration container; members
inside are ordinary declarations the hooks already walk),
`type_parameter_constraint` (the `allows` form; hooks do not read
constraints), and   `preproc_if` in body/initializer/accessor positions is skipped by the
  fact hooks. One consequence: a method whose whole body is a `#if`
  split is not recorded as a function (only block/arrow bodies count),
  which affects about 8 methods corpus-wide. Those files previously
  contributed no facts at all, so partial facts strictly dominate.

## Corpus results with tree-sitter-cli 0.27.1 `tree-sitter test`

- Before (pristine v0.23.5 checkout + regen): 168/168 pass.
- After (patched): 168/168 pass, plus 12/12 new sloplint cases.
- No upstream expectation file changed.

## Differential sweep (raw trees, never analyze-only)

All 2767 runtime-libs `src/**/*.cs` plus all 735 Humanizer `**/*.cs`
(3502 files), parsed with the stock registry grammar and with this crate
via throwaway binaries keying only on `root.has_error()`:

- Before: 280 files with parse errors.
- After: 168 files with parse errors.
- Fixed: 112. Clean-to-error regressions: 0.
- Per-cluster drops: `allows ref struct` (~24 first-error files),
  `ref partial struct` (8), pointer-deref reads (~40 incl. whole-file
  Enum.cs/Span.cs/ReadOnlySpan.cs recoveries), ref-return assigns (~15:
  Unsafe.Add/AsRef/As patterns, `=> F(...) = null`),
  `?.`/`?[]` assigns (~10), `[method]`/`[type, x]` (2),
  `extension(...)` unconstrained forms (PolyfillShims goes fully clean),
  U+202F (synthetic, zero corpus hits), unsafe dtors (1),
  `=>`/block/const/accessor `#if` splits (~12: RuntimeHelpers,
  CoverageGapTests, RuntimeFeature, OutOfMemory/InsufficientMemory/
  ThreadInterruptedException, JsonConstants, Utf8JsonReader.MultiSegment).

## Constructs known still failing (deliberate; each verified)

- Preview/invalid, kept as regression controls: `safe` members (CS8652
  preview + CS9388), `closed` classes (CS8652 preview), `union`
  declarations (CS1001), `#:property` file-based directives (CS9298).
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
  split, OfType.SpeedOpt `};`, EventListener `{`.
- CS-SPLITPP, multi-attribute `#if` branches (`#if X [A] [B] [C]
  #endif`, EnumDehumanizeExtensions): branch-vs-outer repeat ambiguity.
- CS-SPLITPP, accessor attribute-only branches (`#if MONO [Intrinsic]
  #endif` before `get`, RuntimeFeature.NonNativeAot).
- CS-PTRDEREF writes with cast/postfix operands (`*(T*)p = v`,
  `*p++ = v`): reads parse; the write glues `p = s` inside the cast
  value (shift-favor; Roslyn precedence needs climbing, not CFG).
  `(*(T*)p) = v` parenthesized form parses.
- CS-INTERP `$"{{{f("..")}}}"` (api-access.cs): `{{`-escape + hole +
  inner string needs multi-token lookahead; a pairing prototype regressed
  `$"{{nope}}"` and was reverted. Upstream master scanner (checked
  2026-10-20) has no interpolation fix either.
- Disabled-branch invalid code, valid per Roslyn, unfixable blind:
  `?using System;` (EnumHelper.cs), `typeof(ICollection>)`
  (TrimmingTests).
