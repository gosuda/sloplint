/**
 * @file Tree-sitter grammar for the Lean 4 theorem prover.
 *
 * Node names mirror Lean's own `SyntaxNodeKind`s where doing so costs
 * nothing — e.g. `Lean.Parser.Command.import` becomes `import`,
 * `Lean.Parser.Term.app` becomes `app`. Highlight queries should target
 * the same vocabulary the Lean LSP uses for semantic tokens.
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

/* Operator precedence — higher binds tighter. Numbers loosely follow
   Lean's notation precedences in `Init/Notation.lean`. Each level here
   becomes one prec annotation on a single `binary_op` choice variant. */
const PREC = {
  arrow:  10,
  iff:    20,
  or:     30,
  and:    35,
  cmp:    50,
  append: 60,
  add:    70,
  mul:    80,
  pow:    85,
  comp:   90,
  pipe:   95,
  prefix: 100,
  app:    110,
  proj:   120,
};

const sep1 = (rule, sep) => seq(rule, repeat(seq(sep, rule)));
const sep0 = (rule, sep) => optional(sep1(rule, sep));

/* Both forms anchor layout at their first element. An inline body needs a
   frame too: an explicit delimiter or the next outdented line closes it. */
const body = ($, element) => choice(
  seq($._body_inline,
      sep1(field('inline', element), repeat1(choice(';', $._newline))),
      optional(';'), $._dedent),
  seq($._body_indent,
      sep1(field('stmt', element), repeat1(choice(';', $._newline))),
      optional(';'), $._dedent),
);

export default grammar({
  name: 'lean',

  externals: $ => [
    $._scanned_ident,
    $.block_comment,
    $.doc_comment,
    $.module_doc_comment,
    $.raw_string,
    $._indent,
    $._dedent,
    $._newline,
    $._body_inline,
    $._body_indent,
    $._brace_open,
    $._brace_close,
    $._record_anchor,
    $._record_newline,
    $._record_comma,
    $._for_do,
    $._else,
    $._catch,
    $._finally,
    $._bar,
    $._where,
    /* Norm bars `‖ … ‖`. The same `‖` glyph opens and closes, so a plain
       grammar rule (symmetric delimiter over an unbounded term) blows up
       LR table generation. The external scanner emits these as DISTINCT
       open/close tokens — disambiguated by `valid_symbols` (close when the
       parser expects a close, else open) — so the grammar stays as
       unambiguous as the asymmetric `⌊ … ⌋`. */
    $._norm_open,
    $._norm_close,
    $.error_sentinel,
  ],

  extras: $ => [
    /[ \t\r\n]/,
    $.line_comment,
    $.block_comment,
    $.doc_comment,
  ],

  word: $ => $._plain_ident,

  conflicts: $ => [
    [$._binder_ident, $._term_atom],
    [$._binder_ident, $._term_atom, $.struct_field],
    [$.struct_field],
    /* `public`/`meta` can start an `import`, a `public meta section`
       marker, or a Lean 4 visibility-prefixed declaration. */
    [$.import, $.declaration],
    /* `section`/`namespace` now take `_visibility_mods` (`meta
       section`), so `public meta …` can fork four ways. */
    [$.import, $.namespace, $.section, $.declaration],
    /* After `attributes`, `import` is no longer possible, but
       `public meta …` still chooses between section and declaration. */
    [$.section, $.declaration],
    /* `Foo a.b` — custom_command arg atom vs an `_op_term` that can
       continue into a projection after the atom. decl_doc_cmd's
       atom-repeat shares the generated repeat node with
       custom_command's required arg list. */
    [$.custom_command, $._op_term],
    [$.custom_command, $.decl_doc_cmd, $._op_term],
    /* `package {}` — the lake_cmd brace block vs a struct_lit arg;
       `package { k := v ,` — its `key := value` fields vs struct_lit
       fields. */
    [$.lake_cmd, $.struct_lit],
    [$.lake_cmd, $._term_atom],
    [$.lake_cmd, $.struct_field],
    /* `{ ident <ident> … }` could be a function-style struct_field
       (`method args := body`) or the singleton-set `{ app args }`. We
       always prefer the struct_field when followed by `:=`. */
    [$.struct_field, $._term_atom],
    /* `{ a, b ... }` could be a set literal (`elem` list) or the
       multi-source struct update `{ src₁, src₂ with … }`. `with`
       one token after disambiguates. */
    [$.struct_lit],
  ],

  rules: {
    module: $ => seq(
      optional($.module_doc_comment),
      optional($.moduleTk),
      optional($.prelude),
      repeat($.import),
      repeat($._command),
    ),

    moduleTk: _ => 'module',
    prelude:  _ => 'prelude',

    import: $ => seq(
      optional($.public),
      optional($.meta),
      'import',
      optional($.all),
      field('name', seq(repeat('.'), $.identifier)),
    ),

    public: _ => 'public',
    meta:   _ => 'meta',
    all:    _ => 'all',

    /* ===== commands ====================================================== */

    _command: $ => choice(
      $.check,
      $.eval,
      $.print,
      $.reduce,
      $.namespace,
      $.section,
      $.end,
      $.variable,
      $.universe,
      $.open,
      $.export,
      $.attribute_cmd,
      $.initialize,
      $.set_option,
      $.deriving_cmd,
      $.assert_cmd,
      $.decl_doc_cmd,
      $.grind_pattern_cmd,
      $.alias_cmd,
      $.notation_decl_cmd,
      $.syntax_cmd,
      $.macro_cmd,
      $.macro_rules_cmd,
      $.elab_rules_cmd,
      $.elab_cmd,
      $.omit,
      $.register_cmd,
      $.unif_hint_cmd,
      $.declare_syntax_cat_cmd,
      $.mutual,
      $.hash_cmd,
      $.custom_command,
      $.run_meta_cmd,
      $.include_cmd,
      $.lake_cmd,
      $.declaration,
    ),

    /* `run_meta e` evaluates a `MetaM` term at compile time. */
    run_meta_cmd: $ => prec.right(seq('run_meta', $._term)),

    /* `include h1 h2` makes hypotheses visible to later decls. */
    include_cmd: $ => prec.right(seq('include', repeat1($._binder_ident))),

    /* `end <name>?` inside `end` already exists; nothing needed. */

    /* Lake DSL top-level commands. `lakefile.lean` uses a
       deliberately loose shape: a lowercase command head with atom
       args, `from git "u" @ "rev"` tails, `key := value` brace
       fields and a `: Type := do` target form. The heads are
       enumerated so they never shadow real commands. */
    lake_cmd: $ => prec.right(seq(
      optional($.attributes),
      field('command', choice(
        'package', 'lean_lib', 'lean_exe', 'extern_lib',
        'extern', 'input_file', 'input_dir', 'script',
        'post_update', 'facet', 'require', 'target',
        'lean_lib_shared', 'extern_lib_shared')),
      repeat(field('arg', choice(
        $.identifier, $.str_lit, $.num_lit,
        'from', 'git', '@', 'with'))),
      optional(seq(alias($._brace_open, '{'),
        repeat(seq(
          field('key', choice($.identifier, $.str_lit)),
          choice(':=', '='),
          field('value', choice($.identifier, $.str_lit, $.num_lit,
                                $.array_lit)),
          optional(choice(';', ',', $._newline)))),
        alias($._brace_close, '}'))),
      optional(seq(':', $._term)),
      optional(seq(':=', $._term)),
    )),

    /* `mutual … end` groups mutually-recursive declarations. Like
       `namespace`/`section`/`end`, it is modeled as a flat marker
       rather than a nesting block: the inner declarations parse as
       siblings and the closing `end` is the ordinary `end` command.
       Pairing is left to tooling, consistent with the rest of the
       grammar (and avoids an `end`-disambiguation conflict). */
    mutual: _ => 'mutual',

    /* `declare_syntax_cat name (behavior := both)?` — introduces a new
       syntax category for the parser DSL. */
    declare_syntax_cat_cmd: $ => seq(
      'declare_syntax_cat',
      field('name', $.identifier),
      optional(seq(
        '(', 'behavior', ':=', field('behavior', $.identifier), ')',
      )),
    ),

    /* `register_builtin_option name : Type := value` and
       `register_error_explanation name { … }` — coarse parse of
       Lean's various registration commands. The body is either a
       `:= expr` value or a `{ … }` struct literal. */
    register_cmd: $ => prec.right(seq(
      choice(
        'register_builtin_option',
        'register_option',
        'register_error_explanation',
      ),
      field('name', $.identifier),
      optional($._type_spec),
      optional(choice(
        seq(':=', field('value', $._term)),
        $.struct_lit,
      )),
    )),

    /* `unif_hint name (binders)? where lhs ⊢ rhs` — Lean's
       unification-hint declaration. Body is intentionally permissive. */
    unif_hint_cmd: $ => prec.right(seq(
      'unif_hint',
      optional(field('name', $.identifier)),
      repeat($._bracketed_binder),
      alias($._where, 'where'),
      repeat($._term),
    )),

    /* `assert_not_exists Name`, `assert_not_imported Name`, etc. —
       Lean's lightweight assertion commands taking a single ident. */
    /* `#exit`, `#synth`, and `#version` are covered by `hash_cmd`; the
       letter-named forms each take exactly one name, and the required
       target keeps `assert_not_exists Foo` from forking against a
       following `custom_command`. */
    assert_cmd: $ => seq(
      choice(
        'assert_not_exists', 'assert_not_imported',
        'assert_exists', 'check_assertions',
      ),
      field('target', $._term_atom),
    ),

    /* Lean/Mathlib commands that attach a doc/spelling/grind note or
       a `:=`-bodied cast/coercion to an existing declaration. Takes
       an identifier or string literal, optional atomic args
       (`initialize_simps_projections Foo (a, b)`), and any number of
       `=> e` / `for e` / `in e` / `:= e` clauses
       (`recommended_spelling "iff" for "↔" in [Iff, …]`). */
    decl_doc_cmd: $ => prec.right(seq(
      choice(
        'add_decl_doc', 'library_note', 'recommended_spelling',
        'initialize_simps_projections',
        'to_dual_insert_cast', 'to_dual_insert_cast_fun',
      ),
      field('name', choice($.identifier, $.str_lit)),
      repeat(field('arg', $._term_atom)),
      repeat(seq(
        field('clause', choice('=>', 'for', 'in', ':=')),
        field('rhs', $._term),
      )),
    )),

    /* `grind_pattern Foo => rhs` — registers a `grind` pattern. */
    grind_pattern_cmd: $ => seq(
      'grind_pattern',
      field('name', $.identifier),
      '=>',
      field('pattern', $._term),
    ),

    /* `alias foo := bar` and `alias ⟨l, r⟩ := iff_lemma`. */
    alias_cmd: $ => seq(
      'alias',
      field('name', choice($.identifier, $.anon_ctor)),
      ':=',
      field('value', $._term),
    ),

    /* `infix:20 " <-> " => Iff`, `infixr:30 " ⊕ " => Sum`,
       `prefix:max " - " => Neg.neg`, `notation "⊥" => bot`. The
       `:prec` slot is immediate so it sticks to the keyword. The
       Mathlib `scoped[Foo]` variant pins the notation to a namespace. */
    notation_decl_cmd: $ => seq(
      optional($.attributes),
      optional(choice(
        'scoped',
        'local',
        seq('scoped', '[', field('namespace', $.identifier), ']'),
      )),
      choice('infix', 'infixl', 'infixr', 'prefix', 'postfix', 'notation'),
      optional(seq(
        token.immediate(':'),
        field('prec', choice($.num_lit, $.identifier)),
      )),
      field('op', $.str_lit),
      '=>',
      field('target', $._term),
    ),


    /* `macro_rules | pat => body | …` — declares Lean macro rewrite
       rules. Body is a match-alt list. The optional kind annotation
       (`scoped`/`local`) modifies registration scope. */
    macro_rules_cmd: $ => seq(
      optional(choice('scoped', 'local')),
      'macro_rules',
      optional(seq(
        token.immediate(':'),
        field('prec', choice($.num_lit, $.identifier)),
      )),
      repeat1($.match_alt),
    ),

    /* `elab_rules : tactic | `(…) => body | …` — declares an
       elaborator by rewrite rules for a syntax category. Mirrors
       `macro_rules` but keyed on a `: category` instead of a prec.
       Body terms are usually `do …` blocks or quotations. */
    elab_rules_cmd: $ => seq(
      optional(choice('scoped', 'local')),
      'elab_rules',
      optional($._named_attr),
      ':',
      field('category', $.identifier),
      repeat1($.match_alt),
    ),

    /* `elab [prec]? [(name := X)]? arg* : category => body` —
       declares an elaborator, same header shape as `macro`. The args
       are term atoms (string syntax atoms, quoted terms, etc.). */
    elab_cmd: $ => prec.right(seq(
      optional($.attributes),
      optional(choice('scoped', 'local')),
      'elab',
      optional(seq(
        token.immediate(':'),
        field('prec', choice($.num_lit, $.identifier)),
      )),
      optional($._named_attr),
      repeat($._term_atom),
      ':',
      field('category', $.identifier),
      '=>',
      field('body', $._term),
    )),

    /* `omit [Inst ty]* (ident)* in <cmd>` — scopes instance/parameter
       omission to a following command (`omit [NeZero p] in theorem …`). */
    omit: $ => prec.right(seq(
      'omit',
      repeat1(choice($._bracketed_binder, $.identifier)),
      optional(seq('in', field('body', $._command))),
    )),

    /* `macro [prec]? [(name := X)]? "kw" arg* : category => body` —
       coarse parse of Lean's macro-declaration command. The macro's
       body after `=>` is a single term (typically a `(…)` quotation).
       Argument list before `:` accepts any term-shaped tokens. */
    macro_cmd: $ => prec.right(seq(
      optional($.attributes),
      optional(choice('scoped', 'local')),
      'macro',
      optional(seq(
        token.immediate(':'),
        field('prec', choice($.num_lit, $.identifier)),
      )),
      optional($._named_attr),
      repeat($._term_atom),
      ':',
      field('category', $.identifier),
      '=>',
      field('body', $._term),
    )),

    /* `syntax [prec]? [(name := X)]? arg+ : category` — coarse parse
       of Lean's syntax-declaration command, and also the abbreviation
       form `syntax id := body`. Arguments are `_syntax_atom`s —
       term-shaped tokens plus the syntax-DSL postfixes `*`/`?`/`+`. */
    syntax_cmd: $ => prec.right(seq(
      optional($.attributes),
      optional(choice('scoped', 'local')),
      'syntax',
      optional(seq(
        token.immediate(':'),
        field('prec', choice($.num_lit, $.identifier)),
      )),
      optional($._named_attr),
      choice(
        seq(
          field('name', $.identifier),
          ':=',
          repeat1($._syntax_atom),
        ),
        seq(
          repeat1($._syntax_atom),
          ':',
          field('category', $.identifier),
        ),
      ),
    )),

    /* Coarse syntax-DSL atom for use inside `syntax`/`macro` headers.
       Term atoms suffice for most cases (identifiers, string atoms,
       parens contain richer sub-patterns). Plus the syntax-combinator
       postfixes `*`/`?`/`+` and the `<|>` alternative combinator.
       `&"trace"` marks a non-reserved symbol token. */
    _syntax_atom: $ => choice(
      $._term_atom,
      $.syntax_postfix,
      $.syntax_alt,
      $.nonreserved_atom,
    ),
    syntax_postfix: $ => prec.left(seq(
      field('inner', $._term_atom),
      field('op', choice(
        token.immediate('*'),
        token.immediate('?'),
        token.immediate('+'),
      )),
    )),
    syntax_alt: $ => prec.right(seq(
      field('lhs', choice($._term_atom, $.syntax_postfix, $.nonreserved_atom)),
      '<|>',
      field('rhs', $._syntax_atom),
    )),
    nonreserved_atom: $ => seq('&', field('lit', $.str_lit)),

    /* `deriving instance Foo, Bar for Baz` — standalone deriving. */
    deriving_cmd: $ => seq(
      'deriving', 'instance',
      sep1(field('class', $.identifier), ','),
      'for',
      sep1(field('target', $.identifier), ','),
    ),

    /* `set_option pp.all true` — value is intentionally restricted to
       an atom (literal/identifier) so it can't consume the rest of
       the file as part of an over-eager `_term` parse. The `… in cmd`
       form scopes the option to a single following command. */
    set_option: $ => prec.right(seq(
      'set_option',
      field('name', $.identifier),
      field('value', $._term_atom),
      optional(seq('in', field('body', $._command))),
    )),

    /* `attribute [foo] bar baz` — bracketed attr list followed by
       targets. Different syntax from declaration-leading `@[…]`. */
    attribute_cmd: $ => prec.right(seq(
      'attribute',
      '[', sep1($._attr_instance, ','), ']',
      repeat1(field('target', $.identifier)),
    )),

    /* `initialize` and `builtin_initialize` introduce a side-effectful
       module-level binding evaluated at module-load time. */
    initialize: $ => prec.right(seq(
      choice('initialize', 'builtin_initialize'),
      optional(seq(
        field('name', $._binder_ident),
        optional($._type_spec),
        choice(':=', '←', '<-'),
      )),
      choice(
        $._term,
        seq(
          $._indent,
          sep1($._term, choice(';', $._newline)),
          $._dedent,
        ),
      ),
    )),

    /* `public section` / `public meta section` — Lean 4 module-system
       marker; everything below is exported. It has no body and no
       matching `end`. Optionally preceded by attributes
       (e.g. `@[expose] public section`). */
    check:  $ => seq(choice('#check', '#check_failure'),  field('term', $._term)),
    eval:   $ => seq('#eval',   field('term', $._term)),
    print:  $ => seq(choice('#print', '#print_axioms'),   field('term', $._term)),
    reduce: $ => seq('#reduce', field('term', $._term)),

    /* `#guard e`, `#guard_msgs in cmd`, `#find _ = _`, `#whnf t`,
       `#help tactic`, `#assert …`, `#lint`, and other #commands that
       macros/elaborators register — an open namespace, so the generic
       shape is `#name args* (in cmd)?`. Literal names are preferred
       where a dedicated rule exists (`#check`, `#eval`, …). */
    hash_cmd: $ => prec.right(seq(
      field('command', $.hash_name),
      repeat(field('arg', $._term)),
      optional(seq('in', field('body', $._command))),
    )),
    hash_name: _ => token(/#[a-zA-Z_][a-zA-Z_0-9']*/),

    /* Library/framework commands registered by `syntax`/`elab` in
       other modules (`World "L10"`, `et.semantics.termination "x"`,
       `#doc`-style verso commands) are an open namespace too; a
       Capitalized or dotted head followed by atomic args stays out
       of every keyword-led command's way. */
    custom_command: $ => prec.right(seq(
      field('command', $.custom_command_head),
      repeat(field('arg', $._term_atom)),
      /* `Statement (n : ℕ) : T := by` / `Problem := e` — DSLs that
         mimic the shape of a real declaration tail. The `end Foo` /
         `open Foo Bar` / `section Foo` splits are prevented at the
         named-rule side via `$. _name_or_head`, not here — a required
         tail only manufactured MISSING-arg errors when the head token
         stole a Capitalized word inside a completable command. */
      optional(seq(':', $._term)),
      optional(choice(':=', '=')),
      optional($._term),
    )),
    /* A capitalized head keeps `custom_command` out of the way of
       every keyword-led command (`World`, `Doc.open`, `Circle`); a
       lowercase or underscore head would shadow real commands, so
       those DSLs stay unsupported. */
    custom_command_head: _ => token(choice(
      /[A-Z][a-zA-Z_0-9'!?]*/,
      /[a-zA-Z_][a-zA-Z_0-9'!?]*(\.[a-zA-Z_][a-zA-Z_0-9'!?]*)+/,
    )),

    /* `open Foo in <term>` — scoped-open in term position. */
    open_term: $ => prec.right(seq(
      'open',
      $._open_decl,
      'in',
      field('body', $._term),
    )),

    /* `namespace`, `section`, and `end` are top-level commands rather
       than nested rules. Files in lean4 + Mathlib routinely leave
       namespaces unclosed (relying on EOF as `end`), which a nested
       grammar can't accept. Editor tooling can reconstruct scope by
       pairing namespace/end siblings via queries. */
    namespace: $ => seq(
      optional($._visibility_mods),
      optional($.decl_modifiers),
      'namespace', field('name', $.identifier),
    ),
    /* `prec.right` keeps an optional name inside `section`: without it
       a following Capitalized word could instead be re-read as a new
       `custom_command`, splitting `section Foo` into two commands. */
    section: $ => prec.right(seq(
      optional($.attributes),
      /* `meta section` — the marker lives in _visibility_mods so it
         stays out of decl_modifiers' repeat1 (a 'meta' literal there
         forks `'meta' • 'meta'` against `public meta`). */
      optional($._visibility_mods),
      optional($.decl_modifiers),
      'section', optional(field('name', $._name_or_head)),
    )),
    /* `prec.right` keeps an optional name inside `end` — see
       `section`. */
    end: $ => prec.right(seq(
      'end', optional(field('name', $._name_or_head)))),

    /* `variable [Foo R]` declares one or more vars. `variable (R) in
       <cmd>` scopes the variable just to a single following command. */
    variable: $ => prec.right(seq(
      choice('variable', 'variables'),
      /* `variable (n : ℕ)` bracketed binders, `variable x y` bare
         names, and the newer `variable n : T` bare-typed form. */
      repeat1(choice(
        $._bracketed_binder,
        seq($._binder_ident, optional($._type_spec)))),
      optional(seq('in', field('body', $._command))),
    )),

    universe: $ => prec.right(seq(
      choice('universe', 'universes'),
      repeat1(field('name', $._name_or_head)),
    )),

    open: $ => prec.right(seq(
      'open',
      $._open_decl,
      optional(seq('in', field('body', $._command))),
    )),
    /* `prec.right` keeps `open Foo Bar` one decl — see `section`. */
    _open_decl: $ => prec.right(choice(
      seq(
        field('namespace', $._name_or_head),
        '(',
        repeat1(field('only', $.identifier)),
        ')',
      ),
      seq('scoped', repeat1(field('scoped', $._name_or_head))),
      seq(
        repeat1(field('namespace', $._name_or_head)),
        optional(seq(
          'renaming',
          sep1(field('renaming', $.renaming_pair), ','),
        )),
      ),
    )),

    renaming_pair: $ => seq(
      $.identifier,
      choice('→', '->'),
      $.identifier,
    ),

    export: $ => seq(
      'export',
      field('namespace', $.identifier),
      '(',
      repeat1(field('only', $.identifier)),
      ')',
    ),

    /* ----- declarations ------------------------------------------------- */

    declaration: $ => seq(
      optional($.attributes),
      optional($._visibility_mods),
      optional($.decl_modifiers),
      choice(
        $.def,
        $.theorem,
        $.example,
        $.abbrev,
        $.instance,
        $.axiom,
        $.opaque,
        $.constant,
        $.structure,
        $.inductive,
      ),
    ),

    /* Lean 4 module system visibility markers `public`/`meta` —
       structured separately from decl_modifiers to avoid conflict
       with the `import` prefix forms. May appear before or after
       decl_modifiers (`protected meta def …`). */
    _visibility_mods: $ => repeat1(choice($.public, $.meta)),

    decl_modifiers: $ => repeat1(choice(
      'noncomputable',
      'partial',
      'nonrec',
      'private',
      'protected',
      'unsafe',
      'scoped',
      'local',
    )),

    attributes: $ => seq('@[', sep1($._attr_instance, ','), ']'),
    _attr_instance: $ => seq(
      optional(choice('scoped', 'local')),
      /* `attribute [-simp] Foo` and `@[-simp]` remove an attribute. */
      optional('-'),
      $._attribute,
    ),
    /* Attributes can carry arguments (`@[simp 1000]`,
       `@[combinator_parenthesizer foo, expose]`, `@[simps -isSimp]`,
       `@[grind =]`, `@[grind →]`). Args restricted to atoms,
       unary-prefixed atoms, or bare directive tokens to keep state
       count bounded. */
    _attribute: $ => seq(
      field('name', $.identifier),
      repeat(field('arg', choice(
        $._term_atom,
        $.unary_op,
        '=', '←', '→', '↔',
      ))),
    ),


    /* `Eq.ndrec.{u1, u2}` — declaration name with explicit universe
       parameters. The `.{` token must be immediate (no space). */
    _decl_universes: $ => seq(
      token.immediate('.{'),
      sep1(field('level', choice($.identifier, $.num_lit)), ','),
      '}',
    ),

    def: $ => prec.right(seq(
      'def',
      field('name', $.identifier),
      optional($._decl_universes),
      optional($._binders),
      optional($._type_spec),
      $._decl_val,
      optional($._deriving_clause),
    )),

    theorem: $ => seq(
      choice('theorem', 'lemma'),
      field('name', $.identifier),
      optional($._decl_universes),
      optional($._binders),
      $._type_spec,
      $._decl_val,
    ),

    example: $ => seq(
      'example',
      optional($._binders),
      optional($._type_spec),
      $._decl_val,
    ),

    abbrev: $ => prec.right(seq(
      'abbrev',
      field('name', $.identifier),
      optional($._decl_universes),
      optional($._binders),
      optional($._type_spec),
      $._decl_val,
      optional($._deriving_clause),
    )),

    /* `(priority := N)` / `(name := myName)` — named-attribute prefix
       on instance declarations, before any binders. */
    _named_attr: $ => seq(
      '(',
      field('attr', choice('priority', 'name')),
      ':=',
      field('value', $._term),
      ')',
    ),

    /* Two distinct shapes so tree-sitter doesn't have to disambiguate
       `instance ident (` against `instance (binder)`. With a name, the
       binder list can be `_binders` (bare-ident or bracketed). Without a
       name, only bracketed binders are accepted — the name slot doesn't
       compete with bare-ident binders. */
    instance: $ => choice(
      seq(
        'instance',
        repeat($._named_attr),
        field('name', $.identifier),
        optional($._decl_universes),
        optional($._binders),
        $._type_spec,
        $._decl_val,
      ),
      seq(
        'instance',
        repeat($._named_attr),
        repeat($._bracketed_binder),
        $._type_spec,
        $._decl_val,
      ),
    ),

    axiom: $ => seq(
      'axiom',
      field('name', $.identifier),
      optional($._decl_universes),
      optional($._binders),
      $._type_spec,
    ),

    opaque: $ => seq(
      'opaque',
      field('name', $.identifier),
      optional($._decl_universes),
      optional($._binders),
      $._type_spec,
      optional($._decl_val),
    ),

    constant: $ => seq(
      'constant',
      field('name', $.identifier),
      optional($._decl_universes),
      optional($._binders),
      $._type_spec,
      optional($._decl_val),
    ),

    structure: $ => prec.right(seq(
      choice('structure', 'class'),
      field('name', $.identifier),
      optional($._decl_universes),
      optional($._binders),
      optional($._type_spec),
      optional($._extends_clause),
      optional(seq(
        alias($._where, 'where'),
        /* Indented field block separates fields with NEWLINE so a
           greedy `_term`-typed type doesn't eat the next field.
           `deriving` may also live inside the indent block at the
           same indent as the fields. */
        optional(choice(
          seq(optional($.ctor), repeat1($.field),
              optional($._deriving_clause)),
          seq(
            $._indent,
            optional($.ctor),
            sep1($.field, $._newline),
            optional(seq($._newline, $._deriving_clause)),
            $._dedent,
          ),
        )),
      )),
      optional($._deriving_clause),
    )),

    inductive: $ => prec.right(seq(
      choice('inductive', seq('class', 'inductive')),
      field('name', $.identifier),
      optional($._decl_universes),
      optional($._binders),
      optional($._type_spec),
      optional($._extends_clause),
      optional(choice(
        seq(choice(alias($._where, 'where'), ':='), repeat($.ctor_alt)),
        repeat1($.ctor_alt),
      )),
      optional($._deriving_clause),
    )),

    /* `extends Parent` or `extends Parent, name : Type, …` — the
       named-projection form attaches an explicit field to a parent
       projection (used in `MinimalAxioms` patterns). */
    _extends_clause: $ => seq('extends', sep1(
      choice(
        $._term,
        seq(field('name', $.identifier), ':', $._term),
      ),
      ',')),

    /* `MkStruct ::` declares a named constructor (followed by fields).
       Optionally preceded by decl modifiers (e.g. `private mk ::`). */
    ctor: $ => seq(
      optional($.decl_modifiers),
      field('name', $.identifier),
      '::',
    ),

    /* Fields do not allow inline attributes/doc-comments or multi-name
       grouping: both create unresolvable ambiguities with the
       surrounding declaration. Real Lean code overwhelmingly uses one
       name per line in structure fields. doc_comment is in `extras`
       so it still attaches to the field as trivia. Function-style
       fields with binders are common (`f (a : α) : β`). `(name : T)`
       and `[name : T]` allow explicit / instance-implicit field forms.
       Modifiers like `protected`/`private` are recognised. */
    field: $ => choice(
      seq(
        optional($.decl_modifiers),
        field('name', $._binder_ident),
        optional($._binders),
        $._type_spec,
        optional(field('default', seq(':=', $._term))),
      ),
      seq(
        '(',
        field('name', $._binder_ident),
        optional($._binders),
        $._type_spec,
        optional(field('default', seq(':=', $._term))),
        ')',
      ),
      seq(
        '[',
        field('name', $._binder_ident),
        optional($._binders),
        $._type_spec,
        ']',
      ),
    ),

    ctor_alt: $ => seq(
      alias($._bar, '|'),
      field('name', $.identifier),
      optional($._binders),
      optional($._type_spec),
    ),

    _deriving_clause: $ => seq('deriving', sep1($.identifier, ',')),

    /* A declaration body is either `:= term`, a sequence of match-
       style equations `| pat => body | …`, or a `where`-style struct
       body of `field := value` lines (instance/anonymous-structure
       def form). */
    _decl_val: $ => choice(
      seq(field('body', seq(':=', $._term)), optional($.where_decls)),
      seq(repeat1($.match_alt), optional($.where_decls)),
      seq($.where_struct, optional($.where_decls)),
    ),
    _type_spec: $ => field('type', seq(':', $._term)),

    /* `where f := v g := w …` body of a `def name : T where …` —
       declares a value of a structure/instance type by field. The
       indented form uses scanner-emitted INDENT/NEWLINE/DEDENT so a
       field whose RHS is a multi-line term doesn't accidentally
       swallow the next field's name. The body is optional: an empty
       `where` clause is legal when the structure has no fields.
       `where ...` is Lean's "fill all required fields with sorry"
       elision marker.

       The same `where` clause also introduces auxiliary helper
       definitions on a regular `def`/`theorem`:
         def f x := body
         where
           helper := …
           another | pat => …
       A `where_item` covers both forms: a struct field
       (`name [binders] := value`) or a function-style helper
       (`name | pat => … | pat => …`). */
    where_struct: $ => prec.right(seq(
      alias($._where, 'where'),
      optional(choice(
        '...',
        $.struct_field,
        seq(
          $._indent,
          sep1($.struct_field, $._newline),
          $._dedent,
        ),
      )),
    )),
    /* `where` block of auxiliary helper definitions trailing a
       `def`/`theorem`. Distinct from `where_struct`: the items here
       are full sub-declarations (function-style match alts), not
       struct field assignments. */
    where_decls: $ => seq(
      alias($._where, 'where'),
      choice(
        $.where_aux_def,
        seq(
          $._indent,
          sep1($.where_aux_def, $._newline),
          $._dedent,
        ),
      ),
    ),
    where_aux_def: $ => prec.right(seq(
      field('name', $.identifier),
      optional(alias(repeat1(choice(
        $._binder_ident,
        $.implicit_binder,
        $.explicit_binder,
      )), $.binders)),
      optional($._type_spec),
      choice(
        seq(':=', field('body', $._term)),
        repeat1($.match_alt),
      ),
    )),

    /* ===== binders ======================================================= */

    _binders: $ => alias(repeat1($._binder), $.binders),
    _binder: $ => choice(
      $._binder_ident,
      $._bracketed_binder,
      $.tuple_binder,
      $.anon_ctor_binder,
    ),

    /* `fun (x, y) => …` — destructured tuple as a single binder.
       Distinct from `explicit_binder` (which has `:`). */
    tuple_binder: $ => seq('(',
      $._binder_ident, ',', sep1($._binder_ident, ','),
      ')'),

    /* `fun ⟨x, y⟩ => …` — anonymous-constructor destructuring binder.
       Supports nested forms like `⟨x, ⟨y, z⟩⟩`. */
    anon_ctor_binder: $ => seq(
      '⟨',
      /* `-` drops a constructor field (`⟨a, -⟩`); it's a wildcard,
         only inside this angle-bracket pattern. */
      sep1(choice($._binder_ident, $.anon_ctor_binder, '-'), ','),
      '⟩',
    ),

    _binder_ident: $ => choice($.identifier, $.hole),

    _bracketed_binder: $ => choice(
      $.explicit_binder,
      $.implicit_binder,
      $.strict_implicit_binder,
      $.instance_binder,
    ),

    explicit_binder: $ => seq(
      '(',
      choice(
        repeat1(field('name', $._binder_ident)),
        field('name', $.anon_ctor_binder),
      ),
      optional($._type_spec),
      optional(field('default', seq(':=', $._term))),
      ')',
    ),

    implicit_binder: $ => seq(
      alias($._brace_open, '{'),
      repeat1(field('name', $._binder_ident)),
      optional($._type_spec),
      alias($._brace_close, '}'),
    ),

    strict_implicit_binder: $ => seq(
      choice('⦃', '{{'),
      repeat1(field('name', $._binder_ident)),
      optional($._type_spec),
      choice('⦄', '}}'),
    ),

    instance_binder: $ => seq(
      '[',
      optional(seq(field('name', $.identifier), ':')),
      field('type', $._term),
      ']',
    ),

    /* ===== terms =========================================================
     *
     * Split into three tiers to keep LR state count under control:
     *
     *   `_term`         — any term; the entry point for declarations,
     *                     fun bodies, paren contents, etc.
     *   `_lead_term`    — terms that begin with a keyword and consume
     *                     everything to their right at low precedence
     *                     (fun/let/have/show/if/match/forall/exists/
     *                     by/do). Not allowed as binary-op operands.
     *   `_op_term`      — terms that participate in operator chains:
     *                     atoms, application, projection, binary ops,
     *                     unary ops.
     *
     * Restricting binary-op operands to `_op_term` rather than `_term`
     * stops every operator state from forking through every lead-term
     * alternative. Lead terms can still wrap an op-term (`fun x => x + 1`)
     * because the `_term` rule under fun's body covers both.
     */

    _term: $ => choice($._lead_term, $._op_term),

    _lead_term: $ => choice(
      $.fun, $.let, $.have, $.suffices, $.show, $.if_then_else,
      $.forall, $.exists, $.sigma, $.big_op_binder, $.by, $.do_block,
      $.match, $.calc, $.open_term,
    ),

    _op_term: $ => choice(
      $._term_atom, $.app, $.proj, $.subscript, $.postfix_op,
      $.binary_op, $.unary_op, $.universe_app,
    ),

    _term_atom: $ => choice(
      $.identifier,
      $.num_lit,
      $.scientific_lit,
      $.char_lit,
      $.str_lit,
      $.raw_string,
      $.interpolated_str,
      $.name_lit,
      $.hole,
      $.synth_hole,
      $.named_hole,
      $.cdot,
      $.dot_ident,
      $.type_const,
      $.sort_const,
      $.prop_const,
      $.true_const,
      $.false_const,
      $.bot_const,
      $.top_const,
      $.empty_const,
      $.goal_const,
      $.sorry,
      $.paren,
      $.anon_ctor,
      $.anon_subterm,
      $.quotient_lit,
      $.floor_lit,
      $.ceil_lit,
      $.norm_lit,
      $.dbl_angle_lit,
      $.list_lit,
      $.range_lit,
      $.array_lit,
      $.struct_lit,
      $.set_builder,
      $.subtype_lit,
      $.rest_pat,
      $.prec_annotated,
      $.pct_macro,
      $.antiquot,
      $.explicit_mode,
      $.quotation,
    ),

    /* `.foo` is Lean's anonymous-namespace projection (used for
       constructors and methods). Distinct from `e.foo` projection. */
    dot_ident: $ => seq('.', field('name', $.identifier)),

    /* `..` rest-pattern, used inside match arms. */
    rest_pat: _ => '..',

    /* `name:precNum` — precedence annotation on a parser combinator
       like `leading_parser:leadPrec`. The `:` is immediate (no space
       allowed). */
    prec_annotated: $ => seq(
      field('name', $.identifier),
      token.immediate(':'),
      field('prec', choice($.identifier, $.num_lit)),
    ),

    /* `ident%` — Lean built-in macro postfix (e.g. `decl_name%`). */
    pct_macro: $ => seq(
      field('name', $.identifier),
      token.immediate('%'),
    ),

    /* `$ident`, `$(expr)`, `$[…]` — antiquotation marker inside a
       Lean backtick-quotation, splicing a term into the quoted
       syntax. Distinct from `$` as a binary operator (right-assoc
       function application). The leading `$` is lexed as part of a
       single token so `f $ x` (binary form) is unambiguous.

       Antiquotations carry optional type annotations (`$kind:attrKind`)
       and DSL-postfix repeaters (`$pre?`, `$bs*`, `$ids,*`). */
    antiquot: $ => prec.right(seq(
      choice(
        $._antiquot_ident,
        seq($._antiquot_open, $._term, ')'),
        seq($._antiquot_obrack, $._term, ']'),
      ),
      optional(seq(
        token.immediate(':'),
        field('kind', $.identifier),
      )),
      optional(field('repeat', choice(
        token.immediate('?'),
        token.immediate('*'),
        token.immediate('+'),
        token.immediate(',*'),
        token.immediate(',+'),
        token.immediate(',?'),
      ))),
    )),
    _antiquot_ident: _ => token(seq('$', /[A-Za-z_α-ωΑ-Ω][A-Za-z_α-ωΑ-Ω0-9]*/)),
    _antiquot_open: _ => token(seq('$', '(')),
    _antiquot_obrack: _ => token(seq('$', '[')),

    /* `@expr` — disables implicit-argument insertion (explicit args
       mode). Applies to arbitrary terms, not just identifiers
       (`@fun x => …`, `@(f x)`). Excludes `[…]` follow-ups to keep
       `@[…]` attributes unambiguous. */
    explicit_mode: $ => prec(PREC.prefix, seq(
      '@',
      field('term', choice(
        $.identifier,
        $.dot_ident,
        $.paren,
        $.fun,
        $.let,
        $.have,
        $.if_then_else,
        $.match,
      )),
    )),


    hole:        _ => '_',
    synth_hole:  _ => '?_',
    named_hole:  $ => seq('?', $.identifier),
    /* `·` (cdot) — placeholder for the next argument of an anonymous
       function. `(·.foo bar)` reads as `(fun x => x.foo bar)`. */
    cdot:        _ => '·',

    /* `Type`, `Type u`, `Type *`. The explicit-universe form
       `Type.{u, v}` is matched by the generic `universe_app` postfix. */
    type_const: $ => prec.right(1, seq('Type', optional(choice(
      $.identifier,
      '*',
    )))),
    sort_const: $ => prec.right(1, seq('Sort', optional(choice(
      $.identifier,
      '*',
    )))),
    prop_const: _ => 'Prop',
    /* `⊥`/`⊤` are the bottom/top elements of a (semi)lattice. */
    bot_const: _ => '⊥',
    top_const: _ => '⊤',
    /* `∅` is the empty-set/empty-collection literal. */
    empty_const: _ => '∅',
    /* `⊢` is the tactic-language goal marker (used after `at` in
       `rw [h] at h₁ ⊢`). */
    goal_const: _ => '⊢',
    /* Capital `True`/`False` are Prop; lowercase `true`/`false` are
       the boolean inductive's constructors. */
    true_const:  _ => choice('True', 'true'),
    false_const: _ => choice('False', 'false'),
    sorry:       _ => choice('sorry', 'admit'),

    paren: $ => seq(
      '(',
      optional(choice(
        /* Named-argument form: `(name := value)` with an optional
           trailing arg list (`(attr := simps, implicit_reducible)`). */
        seq(
          field('arg_name', $.identifier),
          ':=',
          field('arg_value', $._term),
          repeat(seq(',', $._term)),
        ),
        /* Operator section: `(↑)`, `(↓)`, `(↥)` — bare prefix/coercion
           operator inside parens, treated as a function value. */
        field('section', choice('↑', '↓', '↥')),
        /* Regular parenthesized term, optionally type-ascribed, with
           an optional tuple tail. */
        seq(
          $._term,
          optional($._type_spec),
          repeat(seq(',', $._term)),
        ),
      )),
      ')',
    ),

    anon_ctor: $ => seq('⟨', sep0($._term, ','), '⟩'),

    /* `‹T›` — Lean's anonymous-subterm reference, filled by typeclass
       resolution. Used like `‹Group α›` to grab the ambient instance. */
    anon_subterm: $ => seq('‹', $._term, '›'),

    /* `⟦x⟧` — quotient-class bracket (`Quotient.mk`). */
    quotient_lit: $ => seq('⟦', $._term, '⟧'),

    /* `⌊x⌋` floor and `⌈x⌉` ceil, each with an optional `₊`
       non-negative variant (`⌊x⌋₊`). These use distinct open/close
       brackets, so they are safe delimited atoms. */
    floor_lit: $ => seq('⌊', $._term, choice('⌋', '⌋₊')),
    ceil_lit: $ => seq('⌈', $._term, choice('⌉', '⌉₊')),

    /* `‖x‖` norm / `‖x‖₊` non-negative norm. The `‖` open and close
       are external scanner tokens (see externals) so the symmetric
       glyph doesn't blow up generation. The scanner folds the optional
       trailing `₊` into the close token. */
    norm_lit: $ => seq($._norm_open, $._term, $._norm_close),

    /* `⟪x, y⟫` — double-angle-bracket constructor (inner products,
       spans, pairings). Distinct open/close brackets, so safe. */
    dbl_angle_lit: $ => seq('⟪', sep0($._term, ','), '⟫'),

    /* `*` is valid as a simp-set element (`simp [*, foo]`). */
    list_lit: $ => seq('[', sep0(choice($._term, $.star), ','), ']'),
    star: _ => '*',

    /* `[a : b]` or `[a : b : step]` — range/subarray slice. */
    range_lit: $ => seq(
      '[',
      optional(field('start', $._term)),
      ':',
      optional(field('stop', $._term)),
      optional(seq(':', field('step', $._term))),
      ']',
    ),

    /* `#[a, b, c]` — array literal. */
    array_lit: $ => seq('#[', sep0($._term, ','), ']'),

    /* `{ field := value, ... }` (anonymous structure) and the
       `{ src with field := value, ... }` update form. `{}` is empty.
       `{ src₁, src₂ with }` (multi-source, no new fields) is the
       Mathlib pattern for structure-extension. `{a, b, c}` is the
       finite-set / insert-chain notation. */
    struct_lit: $ => prec.right(1, seq(
      alias($._brace_open, '{'),
      optional(choice(
        seq(sep1(field('source', $._term), ','), 'with',
            $._record_anchor, optional($._record_fields)),
        $._record_fields,
        sep1(field('elem', $._term), ','),
      )),
      optional('..'), optional($._type_spec),
      alias($._brace_close, '}'),
    )),

    /* `{ x : T | predicate }` or `{ x | predicate }` — set-builder. */
    set_builder: $ => seq(
      alias($._brace_open, '{'),
      field('binder', $._binder_ident),
      optional($._type_spec),
      alias($._bar, '|'),
      field('pred', $._term),
      alias($._brace_close, '}'),
    ),

    /* `{ x : T // predicate }` — subtype notation. */
    subtype_lit: $ => seq(
      alias($._brace_open, '{'),
      field('binder', $._binder_ident),
      $._type_spec,
      '//',
      field('pred', $._term),
      alias($._brace_close, '}'),
    ),
    /* `name := value`, `name binders := value`, or `name {a b} :=
       value` (function-style fields in `instance F where method args
       := body`). Bare-ident, implicit-`{}`, and explicit-`()` binders
       are accepted. Type ascription `name : T := value` is
       intentionally omitted because the leading `{ ident :` would
       conflict with `set_builder` / `subtype_lit`. */
    struct_field: $ => choice(
      seq(
        field('name', $.identifier),
        optional(alias(repeat1(choice(
          $._binder_ident, $.implicit_binder, $.explicit_binder,
        )), $.binders)),
        optional($._type_spec), ':=', field('value', $._term),
      ),
      field('name', $.identifier),
    ),

    /* `f x` — left-associative application. The argument can be an
       atom or a lead term (`f fun x => x`, `g by simp`), which is
       common in monadic code. The fn must stay in `_op_term` to keep
       binary-op states from forking through every lead alternative. */
    app: $ => prec.left(PREC.app, seq(
      field('fn', $._op_term),
      field('arg', choice($._term_atom, $._lead_term)),
    )),

    proj: $ => prec.left(PREC.proj, seq(
      field('term', $._op_term),
      token.immediate('.'),
      field('field', choice($.identifier, $.num_lit)),
    )),

    /* `Foo.{u₁, u₂}` or `NonemptyType.{0}` — explicit universe-level
       application. The `.{` is one immediate token so it wins the
       longest-match race against the proj-dot. Numeric levels appear
       for `Type 0` etc. */
    universe_app: $ => prec.left(PREC.proj, seq(
      field('term', $._op_term),
      token.immediate('.{'),
      sep1(field('level', choice($.identifier, $.num_lit)), ','),
      '}',
    )),

    /* Mathlib-style postfix operators: `Aᵒᵖ` (opposite), `Aᵐᵒᵖ` (mul
       opposite), `x⁻¹` (inverse), `Aᵀ` (transpose). Tree-sitter sees
       these as separate tokens after the term, not as part of the
       identifier (the modifier-letter unicode chars aren't in
       `isIdRest`). */
    postfix_op: $ => prec.left(PREC.proj, seq(
      field('term', $._op_term),
      field('op', choice(
        'ᵒᵖ', 'ᵐᵒᵖ', 'ᵒᵈ', '⁻¹', 'ᵀ', '†',
        'ᶜ', 'ˣ', '✝',
        '⁺',  /* positive part (`a⁺`) */
      )),
    )),

    /* `arr[idx]` — array/dict indexing. Distinguished from `f [list]`
       by the immediate `[` (no whitespace between fn and bracket).
       `arr[idx]?` and `arr[idx]!` are option-returning and panicking
       variants. `arr[idx]'proof` supplies an in-bounds proof. */
    subscript: $ => prec.left(PREC.proj, seq(
      field('term', $._op_term),
      token.immediate('['),
      field('index', $._term),
      ']',
      optional(choice(
        token.immediate('?'),
        token.immediate('!'),
        seq(token.immediate("'"), field('proof', $._term_atom)),
      )),
    )),

    /* ----- operators ---------------------------------------------------- */

    unary_op: $ => prec(PREC.prefix, seq(
      field('op', choice('-', '¬', '!', '←', '<-', '#',
        /* Coercion prefixes. */
        '↥', '↑', '↓', '⇑',
        /* Heyting negation, co-Heyting boundary (Mathlib). */
        '￢', '∂',
        /* Lean's "borrow ref" prefix on argument types. */
        '@&')),
      field('rhs', $._op_term),
    )),

    binary_op: $ => choice(
      prec.right(PREC.arrow, seq(
        field('lhs', $._op_term),
        field('op', choice(
          '→', '->', '⟶',
          /* Mathlib homomorphism arrows (without bracket param —
             `→ₗ[R]` parses as `→ₗ` + `[R]` applied to rhs).
             `≃` itself is in the cmp set; only the variants here. */
          '→+', '→*', '→+*', '→ₗ', '→ₐ', '→ₒ',
          '→ₙ+', '→ₙ*', '→ₙ+*', '→ₑ', '→ₑ+', '→ₑ*',
          '≃+', '≃*', '≃+*', '≃ₗ', '≃ₐ', '≃ₒ',
          '→ᵃ', '≃ᵃ',
          '↪', '↠', '↪o', '↪+', '↪*', '↪+*',
          '⇨',       /* Heyting implication */
          '⥤',       /* Mathlib functor arrow */
          '≌',       /* category equivalence */
          '⟹',       /* natural transformation (`F ⟹ G`) */
        )),
        field('rhs', $._term),
      )),
      prec.right(PREC.iff, seq(
        field('lhs', $._op_term),
        field('op', choice('↔', '<->', '⇔')),
        /* `p ↔ ∃ x, …` is extremely common in Mathlib. */
        field('rhs', $._term),
      )),
      prec.left(PREC.or, seq(
        field('lhs', $._op_term),
        field('op', choice('∨', '||', '⊔', '⊻')),  /* ⊻ = xor */
        /* Lattice-sup RHS often holds a big-op or `if`. */
        field('rhs', $._term),
      )),
      prec.left(PREC.and, seq(
        field('lhs', $._op_term),
        field('op', choice('∧', '&&', '⊓', '⊼')),  /* ⊼ = nand */
        /* Lattice-inf RHS often holds a big-op or `if`. */
        field('rhs', $._term),
      )),
      prec.left(PREC.cmp, seq(
        field('lhs', $._op_term),
        field('op', choice(
          '=', '==', '≠', '!=',
          '<', '≤', '<=',
          '>', '≥', '>=',
          '∈', '∉', '⊆', '⊂', '⊇', '⊃', '⊑', '⊒',
          '≡', '≢', '~', '≃', '≅', '≈', '≉',
          '≍', '≃ₘ', '≃ᵤ',  /* heq, measureable-equiv, uniform-equiv */
          '∣', '∤',  /* divides, not-divides */
          '⋖', '⋗',  /* covby (atomic-cover) and its dual */
          '⊨', '⇒',  /* entailment / relation-implication (Relator) */
          '≺', '≼',  /* strict and non-strict order relations */
          '<+:', '<:+', '<+>',
          '⊣',       /* adjunction (`F ⊣ G`) */
        )),
        /* `a = fun x => …` and `a ≤ if p then x else y` are common
           enough that comparison RHS must allow lead terms. */
        field('rhs', $._term),
      )),
      prec.right(PREC.append, seq(
        field('lhs', $._op_term),
        field('op', choice('::', '::ᵣ', '::ₘ')),
        field('rhs', $._op_term),
      )),
      prec.right(PREC.append, seq(
        field('lhs', $._op_term),
        field('op', '++'),
        field('rhs', $._term),
      )),
      prec.left(PREC.add, seq(
        field('lhs', $._op_term),
        field('op', choice(
          '+', '-', '∪', '\\',
          /* `∆` — set symmetric difference; `⊞` — boxplus;
             `↾` — restriction. (`⨁` is a big_op_binder prefix, not
             an infix operator — adding it here conflicts.) */
          '∆', '⊞', '↾',
          /* Mathlib torsor & substitution operators. */
          '-ᵥ', '+ᵥ', '▸',
        )),
        field('rhs', $._op_term),
      )),
      prec.left(PREC.mul, seq(
        field('lhs', $._op_term),
        field('op', choice(
          '*', '/', '%', '∩', '×', "×'", '×ˢ', '×ₗ', '•', '∙',
          /* `∫ x, f x ∂μ` — the measure marker binds the integrand
             to its measure. */
          '∂',
          /* Tensor and related Mathlib operators (without bracket
             param — `⊗ₜ[R]` parses as `⊗ₜ` + `[R]` consumed by app). */
          '⊗', '⊗ₜ', '⊗ₛ',
          /* Sum/direct-sum operators (Mathlib). */
          '⊕', '⊕ₗ',
          /* Product, convolution/star, and matrix-dot products. */
          '⨯', '∗', '⋆', '⬝',
          /* Circled/boxed products (applicative seq, external product). */
          '⊛', '⊠',
          /* Kronecker product, matrix-vector dot, Hadamard product,
             box product. */
          '⊗ₖ', '⬝ᵥ', '⊙', '◫',
        )),
        field('rhs', $._op_term),
      )),
      prec.right(PREC.pow, seq(
        field('lhs', $._op_term),
        field('op', '^'),
        field('rhs', $._op_term),
      )),
      prec.right(PREC.comp, seq(
        field('lhs', $._op_term),
        field('op', choice(
          '∘',
          /* Mathlib's set image / preimage operators. */
          "''", "⁻¹'",
          /* Category-theory morphism / functor / iso composition
             (Mathlib). */
          '≫', '⋙', '≪≫',
          /* Whiskering (bicategory) and circle-composition. */
          '◁', '⊚',
          /* Linear-map composition. */
          '∘ₗ',
        )),
        field('rhs', $._op_term),
      )),
      prec.left(PREC.pipe, seq(
        field('lhs', $._op_term),
        field('op', choice('|>', '<|')),
        /* `f <| have h := …; body` is common Mathlib. */
        field('rhs', $._term),
      )),
      /* `$` is right-associative function application without parens
         (`f $ g x = f (g x)`). Low precedence. */
      prec.right(PREC.arrow, seq(
        field('lhs', $._op_term),
        field('op', '$'),
        field('rhs', $._op_term),
      )),
      /* Monad/applicative and parser-combinator operators. Grouped
         together at "append" precedence since their actual notation
         precedences vary per definition. */
      prec.left(PREC.append, seq(
        field('lhs', $._op_term),
        field('op', choice(
          '>>', '<<', '>>>', '<<<',
          '>>=', '=<<',
          '<$>', '<*>', '<|>', '<&>', '<&&>', '<||>',
          '&&&', '|||', '^^^',
        )),
        field('rhs', $._op_term),
      )),
    ),

    /* ----- binder-using terms ------------------------------------------- */

    /* `fun arg1 arg2 => body`  or  `fun | pat => body | …` (match
       style). The latter is sugar for `fun x => match x with | … | …`. */
    fun: $ => prec.right(seq(
      choice('fun', 'λ'),
      choice(
        seq(
          $._binders,
          optional($._type_spec),
          choice('=>', '↦'),
          field('body', $._term),
        ),
        repeat1($.match_alt),
      ),
    )),

    /* `∀ x, body`, `∀ x : T, body`, `∀ x ∈ s, body` (sugar) — and
       Mathlib's `∀ b ≠ ⊥, body` / `∀ c ⊆ S, body` predicate-tail
       sugar. */
    forall: $ => prec.right(seq(
      /* `Π` is a deprecated Mathlib synonym for `∀` for product types. */
      choice('∀', 'forall', 'Π'),
      $._binders,
      optional(choice(
        $._type_spec,
        seq(
          field('rel', choice('∈', '∉', '≠', '⊆', '⊂',
                              '>', '<', '≥', '≤', '=')),
          field('bound', $._op_term),
        ),
      )),
      ',',
      field('body', $._term),
    )),

    exists: $ => prec.right(seq(
      choice('∃', 'exists'),
      $._binders,
      optional(choice(
        $._type_spec,
        seq(
          field('rel', choice('∈', '∉', '≠', '⊆', '⊂',
                              '>', '<', '≥', '≤', '=')),
          field('bound', $._op_term),
        ),
      )),
      ',',
      field('body', $._term),
    )),

    /* `Σ x, T x` / `Σ' x, T x` — sigma / non-dependent-pair type binder. */
    sigma: $ => prec.right(seq(
      choice('Σ', "Σ'"),
      $._binders,
      optional($._type_spec),
      ',',
      field('body', $._term),
    )),

    /* `⨆ x, f x` / `⨅ x, f x` / `∑ x, f x` / `∏ x, f x` — big-operator
       binder notation. Mathlib also writes `⨆ x ∈ s, P x` (sugar),
       `∑ x ∈ s, f x`, and `⨆ x : T, f x`. Indexed unions/intersections
       and lattice ops take the same shape, as do the measure-theoretic
       quantifiers (`∀ᵐ x ∂μ, p x`) and integrals (`∫ x in s, f x ∂μ`,
       where the binder range is spelled with `in`). */
    big_op_binder: $ => prec.right(seq(
      choice(
        '⨆', '⨅', '∑', '∏',
        '⋃', '⋂', '⋀', '⋁', '⨁', '⨂', '∐',
        '∀ᵐ', '∃ᵐ',
        '∫', '∫⁻', '⨍',
      ),
      $._binders,
      optional(choice(
        $._type_spec,
        seq(choice('∈', '⊆', '⊂', 'in'), $._op_term),
        /* `∀ᵐ x ∂μ, p` — "for almost every x w.r.t. measure μ". */
        seq('∂', $._op_term),
      )),
      ',',
      field('body', $._term),
    )),

    /* `let` and `have` accept either the term form `let x := v; body`
       (or `let x := v body` via the block's NEWLINE separator) or the
       do-block statement form `let x := v` with the surrounding block
       supplying the body implicitly. Use `_` for the anonymous name.
       Inline binders are not modeled — write `let f := fun x => …`
       instead. The `←`/`<-` form is monadic bind for do-blocks. */
    let: $ => prec.right(seq(
      /* `letI` is Mathlib's "let instance" variant; `obtain` and
         `set` are tactic-language siblings for pattern-binding from
         a result. */
      choice('let', 'letI', 'obtain', 'set'),
      optional(choice('mut', 'rec')),
      field('name', choice(
        $._binder_ident,
        $.tuple_binder,
        $.anon_ctor_binder,
      )),
      /* `let rec foo (x : T) : U := …` — function-style binders. */
      optional($._binders),
      optional($._type_spec),
      choice(':=', '←', '<-'),
      field('value', $._term),
      optional(seq(';', field('body', $._term))),
    )),

    have: $ => prec.right(seq(
      /* `haveI` is Mathlib's "have instance" variant. */
      choice('have', 'haveI'),
      /* Destructuring `have ⟨a, b⟩ := …` is common in Mathlib;
         anonymous `have : T` binds `this`. */
      optional(field('name', choice($._binder_ident, $.anon_ctor_binder))),
      optional($._type_spec),
      /* Tactic `have h : T` (goal without a proof term) leaves the
         `:=` off; `have h : T from e` is the older spelling. */
      optional(choice(
        seq(':=', field('value', $._term)),
        seq('from', field('value', $._term)),
        $.by,
      )),
      optional(seq(';', field('body', $._term))),
    )),

    /* `suffices g : T from proof` / `suffices g : T by tac` — a
       tactic-language reframing. The `from`/`by` proves the new
       suffices-goal, then the original goal is discharged via `g`. */
    suffices: $ => prec.right(seq(
      'suffices',
      field('name', $._binder_ident),
      $._type_spec,
      choice(
        seq('from', field('value', $._term)),
        $.by,
      ),
    )),

    /* `by_cases h : cond` — splits a tactic goal on the truth of
       `cond`, binding `h` to a proof in each branch. */
    by_cases: $ => prec.right(seq(
      'by_cases',
      field('name', $._binder_ident),
      $._type_spec,
    )),

/* `induction x [using ind]? [generalizing ys]? [with | … ]?` —
       Lean's structural-induction tactic. Same shape with `cases` for
       the case-analysis tactic. Both are written as terms inside a
       `by`-block, so they belong with the other lead terms. The
       `with`-clause holds match-style branches. */
    induction: $ => prec.right(seq(
      choice('induction', 'cases'),
      sep1(seq(
        optional(seq(field('discr_name', $._binder_ident), ':')),
        field('target', $._term),
      ), ','),
      optional(seq('using', field('elim', $._term))),
      optional(seq('generalizing', repeat1(field('gen', $._binder_ident)))),
      optional(seq('with', repeat1(alias($._tactic_match_alt, $.match_alt)))),
    )),

    show: $ => prec.right(seq(
      'show',
      field('type', $._term),
      /* Bare `show T` is the tactic form (re-states the goal);
         `show T from e` / `show T by tac` are the term forms. */
      optional(choice(
        seq('from', field('value', $._term)),
        $.by,
      )),
    )),

    /* Term conditionals require else. Do conditionals have their own rule. */
    if_then_else: $ => prec.right(seq(
      'if', $._if_condition, 'then', field('then', $._term),
      alias($._else, 'else'), field('else', $._term),
    )),

    /* Tactic and do sequences retain their own statement grammar. */
    by: $ => seq('by', $._tactic_body),
    do_block: $ => seq('do', $._do_body),

    _block_body: $ => body($, $._term),

    /* `try stmts catch (pat (: T)?)? => term (| pat => term)*
       finally stmts` — do-block exception handling. Catch bodies are
       single terms (as in `match_alt`): an indented `do …` in that
       position is a `do_block` term with its own layout. */
    do_try: $ => prec.right(seq(
      'try', $._do_body,
      optional(seq(alias($._catch, 'catch'), choice(
        seq(field('name', $._binder_ident), optional($._type_spec),
            choice('=>', '↦'), field('body', $._do_body)),
        repeat1(seq(alias($._bar, '|'), field('pattern', $._term),
                    choice('=>', '↦'), field('body', $._do_body))),
      ))),
      optional(seq(alias($._finally, 'finally'), $._do_body)),
    )),

    /* `for x in xs do …` / `for h : x in xs, y in ys do …` — do-block
       iteration. The binder may carry an accumulator hypothesis
       name (`for h : x in xs do`) and several comma-separated
       iterators share one body. */
    /* `prec.right(PREC.app + 1)` keeps the `do` bound to the loop
       even though `do`-blocks are also legal app args (the `xs do`
       app-read loses the precedence race). */
    for_loop: $ => prec.right(PREC.app + 1, seq(
      'for',
      sep1(seq(
        optional(seq(field('acc', $._binder_ident), ':')),
        field('name', choice(
          $._binder_ident, $.tuple_binder, $.anon_ctor_binder)),
        'in', field('range', $._term),
      ), ','),
      alias($._for_do, 'do'), $._do_body,
    )),

    /* `x := v` mutation, valid only inside a do-block. */
    block_assign: $ => seq(
      field('name', $.identifier),
      ':=',
      field('value', $._term),
    ),

    match: $ => prec.right(seq(
      'match',
      sep1(seq(
        optional(seq(field('discr_name', $._binder_ident), ':')),
        field('scrutinee', $._term),
      ), ','),
      'with',
      repeat1($.match_alt),
    )),
    match_alt: $ => seq(
      alias($._bar, '|'),
      /* `| a | b | c => …` folds several alternatives into one arm;
         `,`-separated patterns are constructor args. */
      sep1(field('pattern', $._term), choice(',', alias($._bar, '|'))),
      /* Mathlib prefers `↦` over `=>` in match arms. */
      choice('=>', '↦'),
      field('body', $._term),
    ),

    /* `conv [at h]? => rw [e]` — conversion tactics take an optional
       location then a `=>` conversion block. `conv_lhs`/`conv_rhs`/
       `conv_fun`/`conv_arg`/`conv_apply` select inside. */
    conv: $ => prec.right(seq(
      choice('conv', 'conv_lhs', 'conv_rhs', 'conv_fun', 'conv_arg', 'conv_apply'),
      optional(seq(optional(choice('at', 'in')), $._term)),
      '=>', $._tactic_body,
    )),

    /* A calc step is a relation term followed by its proof. Keeping the
       step list inside calc avoids competing with every ordinary block
       expression at each statement boundary. */
    calc: $ => seq('calc', body($, $.calc_step)),
    calc_step: $ => seq(
      field('relation', $._term),
      ':=',
      field('proof', $._term),
    ),

    /* `case name args => body` — Lean tactic-language case selection
       (analogous to a `match_alt` but goal-keyed). Stand-alone tactic
       statement; commonly chained as sibling at the same indent. */
    tactic_case: $ => prec.right(seq(
      choice('case', "case'"),
      field('name', $.identifier),
      repeat(field('arg', $._binder_ident)),
      '=>', field('body', $._tactic_body),
    )),




    _tactic_body: $ => body($, $._tactic),
    _do_body: $ => body($, $._do_statement),
    _tactic: $ => choice($._tactic_atom, alias($._tactic_chain, $.binary_op)),
    _tactic_chain: $ => prec.left(PREC.pipe, seq(
      field('lhs', $._tactic), field('op', '<;>'), field('rhs', $._tactic_atom),
    )),
    _tactic_atom: $ => choice(
      $.induction, $.tactic_case, $.by_cases, $.conv, $.tactic_simpa,
      $.tactic_rcases, $.tactic_bullet, $.tactic_block, $.tactic_first,
      alias($._tactic_match, $.match), $._op_term,
      $.fun, $.let, $.have, $.suffices, $.show, $.if_then_else,
      $.forall, $.exists, $.sigma, $.big_op_binder, $.by, $.do_block,
      $.calc, $.open_term,
    ),
    tactic_bullet: $ => seq('·', $._tactic_body),
    tactic_block: $ => seq(
      alias($._brace_open, '{'), $._tactic_body, alias($._brace_close, '}'),
    ),
    tactic_first: $ => seq('first', repeat1(seq(alias($._bar, '|'), $._tactic_body))),
    tactic_simpa: $ => prec.right(seq(
      field('fn', alias(choice('simpa', 'simpa!', 'simpa?'), $.identifier)),
      repeat(field('arg', $.paren)), optional('only'),
      optional(field('arg', $.list_lit)),
      optional(seq('using', field('arg', $._term))),
    )),
    tactic_rcases: $ => prec.right(seq(
      field('fn', alias('rcases', $.identifier)),
      sep1(seq(
        optional(seq(field('hyp', $._binder_ident), ':')),
        field('arg', $._term),
      ), ','),
      optional(seq('with', $.rcases_pattern)),
    )),
    rcases_pattern: $ => choice(
      $.identifier, $.hole, '-',
      seq(optional('@'), '⟨', sep0($.rcases_pattern, ','), '⟩'),
      seq('(', $.rcases_pattern, ')'),
      prec.left(1, seq($.rcases_pattern, alias($._bar, '|'), $.rcases_pattern)),
      prec.right(2, seq($.rcases_pattern, ':', $._term)),
    ),
    _tactic_match: $ => prec.right(seq(
      'match',
      sep1(seq(optional(seq(field('discr_name', $._binder_ident), ':')),
               field('scrutinee', $._term)), ','),
      'with', repeat1(alias($._tactic_match_alt, $.match_alt)),
    )),
    _tactic_match_alt: $ => seq(
      alias($._bar, '|'), sep1(field('pattern', $._term), choice(',', alias($._bar, '|'))),
      choice('=>', '↦'), field('body', $._tactic_body),
    ),
    _do_statement: $ => choice(
      alias($._do_if, $.if_then_else), alias($._do_match, $.match),
      $.do_try, $.for_loop, $.block_assign, $._op_term,
      $.fun, $.let, $.have, $.suffices, $.show,
      $.forall, $.exists, $.sigma, $.big_op_binder, $.by, $.do_block,
      $.calc, $.open_term,
    ),
    _if_condition: $ => choice(
      seq('let', field('pattern', $._term), ':=', field('cond', $._term)),
      seq(optional(seq(field('hyp', $._binder_ident), ':')),
          field('cond', $._term)),
    ),
    _do_if: $ => prec.right(seq(
      'if', $._if_condition, 'then', field('then', $._do_body),
      optional(seq(alias($._else, 'else'), field('else', $._do_body))),
    )),
    _do_match: $ => prec.right(seq(
      'match',
      sep1(seq(optional(seq(field('discr_name', $._binder_ident), ':')),
               field('scrutinee', $._term)), ','),
      'with', repeat1(alias($._do_match_alt, $.match_alt)),
    )),
    _do_match_alt: $ => seq(
      alias($._bar, '|'), sep1(field('pattern', $._term), choice(',', alias($._bar, '|'))),
      choice('=>', '↦'), field('body', $._do_body),
    ),
    _record_fields: $ => seq(
      sep1($.struct_field, choice(alias($._record_comma, ','), $._record_newline)),
      optional(alias($._record_comma, ',')),
    ),

    /* ===== lexicals (carried from stage 1) =============================== */

    num_lit: _ => token(choice(
      /0[xX][0-9a-fA-F](_?[0-9a-fA-F])*/,
      /0[oO][0-7](_?[0-7])*/,
      /0[bB][01](_?[01])*/,
      /[0-9](_?[0-9])*/,
    )),

    scientific_lit: _ => token(choice(
      /[0-9](_?[0-9])*\.[0-9]?(_?[0-9])*([eE][+-]?[0-9](_?[0-9])*)?/,
      /[0-9](_?[0-9])*[eE][+-]?[0-9](_?[0-9])*/,
    )),

    char_lit: $ => seq(
      '\'',
      choice($.escape_sequence, /[^'\\\n]/),
      '\'',
    ),

    str_lit: $ => seq(
      '"',
      repeat(choice(
        $.escape_sequence,
        token.immediate(prec(1, /[^"\\]+/)),
      )),
      '"',
    ),

    escape_sequence: _ => token.immediate(seq('\\', choice(
      /[\\"'rnt0]/,
      /x[0-9a-fA-F]{2}/,
      /u[0-9a-fA-F]{4}/,
      /u\{[0-9a-fA-F]+\}/,
    ))),

    interpolated_str: $ => seq(
      $._interp_open,
      repeat(choice(
        $.escape_sequence,
        $.interpolation,
        token.immediate(prec(1, /[^"\\\{]+/)),
      )),
      '"',
    ),
    _interp_open: _ => choice('s!"', 'm!"'),
    interpolation: $ => seq(
      token.immediate('{'),
      $._term,
      '}',
    ),

    /* `Foo  — quote a name. ``Foo  — quote and resolve against the
       current scope (`SyntaxNodeKind.full`). Both forms are common
       in Lean source. */
    name_lit: $ => seq(choice('`', '``'), field('name', $.identifier)),

    /* `(term)  or  `(category| term)  — syntax quotation. The
       category-bar is `prec.dynamic(1, ...)` so tree-sitter prefers
       it over treating `name |` as a stray pipe. */
    quotation: $ => seq(
      '`', token.immediate('('),
      choice(
        seq(field('category', alias('tactic', $.identifier)), alias($._bar, '|'), $._tactic),
        seq(optional(prec.dynamic(1, seq(field('category', $.identifier), alias($._bar, '|')))),
            $._term),
      ),
      ')',
    ),

    line_comment: _ => token(seq('--', /[^\n]*/)),

    /* Identifier is split into two paths:
     *   `_plain_ident` — internal regex for the ASCII single-component
     *     case. This is the `word` token, so tree-sitter performs
     *     keyword extraction against grammar literals automatically.
     *   `_scanned_ident` — external token emitted by the scanner only
     *     when the identifier needs richer handling: dotted parts,
     *     `«quoted»` escapes, or non-ASCII characters.
     * The scanner returns false (rewind to start) for the plain case,
     * letting internal lexing handle word/keyword arbitration. */
    identifier: $ => choice($._plain_ident, $._scanned_ident),

    /* A Capitalized word where a command could also start lexes as
       `custom_command_head`, not `identifier` — `end Foo`, `section Foo`,
       `open Foo Bar`, `universe U` would otherwise split into a bare
       marker plus a phantom command. Positions that take such a name
       accept `$. _name_or_head` so the enclosing rule can consume it. */
    _name_or_head: $ => choice(
      $.identifier,
      alias($.custom_command_head, $.identifier),
    ),
    _plain_ident: _ => /[a-zA-Z_][a-zA-Z_0-9'!?]*/,
  },
});

