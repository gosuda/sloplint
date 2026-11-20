/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

// Tree-sitter grammar for Alloy 6
// Based on the authoritative CUP parser (Alloy.cup) and lexer (Alloy.lex)

// Precedence levels (lowest to highest), matching CUP grammar ordering
const PREC = {
  QUANTIFIER: -1,
  SEQ: 0,
  OR: 1,
  IFF: 2,
  IMPLIES: 3,
  AND: 4,
  TEMP_BINARY: 5,
  NOT: 6,
  COMPARE: 7,
  MULTIPLICITY: 8,
  SHIFT: 9,
  PLUS_MINUS: 10,
  CARDINALITY: 11,
  OVERRIDE: 12,
  INTERSECT: 13,
  ARROW: 14,
  DOMAIN: 15,
  RANGE: 16,
  BRACKET: 17,
  DOT: 18,
  UNOP: 19,
  PRIME: 20,
};

module.exports = grammar({
  name: "alloy",

  extras: $ => [
    /\s/,
    $.line_comment,
    $.block_comment,
  ],

  word: $ => $.name,

  conflicts: $ => [
    // command ref vs next paragraph
    [$.command],
    // scope: "for 5 State" is typescope, not "for 5" + new paragraph
    [$.scope, $.type_scope_number],
    // name can reduce to name_list or qual_name depending on context
    [$.name_list, $.qual_name],
    // `lone x, y` inside `[...]` can be a decl list or an expr list
    [$.qual_name, $._decl_name],
    // `no trashed'` is multiplicity(no, prime(t)) while `no t': T` is a
    // decl — `'` inside _decl_name and prime_expression is the same token,
    // so the shared GLR lex disambiguates on what follows (: | ,)
    [$.name_list, $.prime_expression],
    [$.qual_name, $.prime_expression],
  ],

  rules: {
    // Real models scatter `;;;;` separator decorations between
    // paragraphs; a run of two or more `;` lexes as one decoration so
    // it never collides with the `;` trace-sequence operator.
    source_file: $ => repeat(choice($._paragraph, $._semicolon_decoration)),

    _semicolon_decoration: $ => /;{2,}/,

    _paragraph: $ => choice(
      $.module_decl,
      $.open_decl,
      $.sig_decl,
      $.fact_decl,
      $.pred_decl,
      $.fun_decl,
      $.assert_decl,
      $.command,
      $.enum_decl,
      $.let_decl,
    ),

    // ---- Module declaration ----
    module_decl: $ => seq(
      'module',
      field('name', $.qual_name),
      optional(seq('[', $.name_list_exact, ']')),
    ),

    // ---- Open (import) ----
    open_decl: $ => seq(
      optional($.visibility),
      'open',
      field('name', $.qual_name),
      optional(seq('[', optional($.sig_ref_list), ']')),
      optional(seq('as', field('alias', $.name))),
    ),

    // ---- Enum ----
    enum_decl: $ => seq(
      optional($.visibility),
      'enum',
      field('name', $.name),
      '{',
      optional($.name_list),
      '}',
    ),

    // ---- Signature ----
    sig_decl: $ => seq(
      repeat($.sig_qualifier),
      'sig',
      field('names', $.name_list),
      optional($.sig_extension),
      choice(
        seq(
          optional(seq('=', $._expression)),
          '{',
          optional($.sig_body),
          '}',
          optional($.block_expression),
        ),
        // `sig NonSymlink = Dir + File` — a cross-signature subset
        // declaration may carry no brace body at all.
        seq('=', $._expression),
      ),
    ),

    sig_qualifier: $ => choice(
      'abstract',
      'lone',
      'one',
      'some',
      'private',
      'var',
    ),

    sig_extension: $ => choice(
      seq('extends', $.sig_ref),
      seq('in', $.sig_ref_union),
    ),

    sig_body: $ => seq(
      $.field_decl,
      repeat(seq(',', $.field_decl)),
      optional(','),
    ),

    field_decl: $ => seq(
      optional('var'),
      optional('private'),
      optional(choice('disj', 'disjoint')),
      $.name_list,
      ':',
      optional(choice('disj', 'disjoint')),
      $._expression,
    ),

    // ---- Fact ----
    fact_decl: $ => seq(
      'fact',
      optional(field('name', choice($.name, $.string))),
      $.block_expression,
    ),

    // ---- Predicate ----
    pred_decl: $ => seq(
      optional($.visibility),
      'pred',
      optional(seq($.sig_ref, '.')),
      field('name', $.name),
      optional(choice(
        seq('(', optional($.decl_list), ')'),
        seq('[', optional($.decl_list), ']'),
      )),
      $.block_expression,
    ),

    // ---- Function ----
    fun_decl: $ => seq(
      optional($.visibility),
      'fun',
      optional(seq($.sig_ref, '.')),
      field('name', $.name),
      optional(choice(
        seq('(', optional($.decl_list), ')'),
        seq('[', optional($.decl_list), ']'),
      )),
      ':',
      field('return_type', $._expression),
      $.block_expression,
    ),

    // ---- Assert ----
    assert_decl: $ => seq(
      'assert',
      optional(field('name', choice($.name, $.string))),
      $.block_expression,
    ),

    // ---- Let (macro) ----
    let_decl: $ => seq(
      optional($.visibility),
      'let',
      field('name', $.name),
      optional(choice(
        seq('(', optional($.name_list), ')'),
        seq('[', optional($.name_list), ']'),
      )),
      choice(
        $.block_expression,
        seq('=', $._expression),
      ),
    ),

    // ---- Command (run/check) ----
    command: $ => seq(
      optional(seq(field('label', $.name), ':')),
      choice('run', 'check'),
      optional(field('name', $.qual_name)),
      optional(field('body', $.block_expression)),
      optional($.scope),
      optional($.expect_clause),
    ),

    scope: $ => seq(
      'for',
      choice(
        seq($.number, optional(seq('but', $.typescope_list))),
        $.typescope_list,
      ),
    ),

    typescope_list: $ => seq(
      $.typescope,
      repeat(seq(',', $.typescope)),
    ),

    typescope: $ => seq(
      optional('exactly'),
      $.type_scope_number,
      $.scope_sig_ref,
    ),

    type_scope_number: $ => choice(
      seq($.number, '..', optional($.number)),
      $.number,
    ),

    scope_sig_ref: $ => choice(
      $.qual_name,
      'univ',
      'Int',
      'int',
      'none',
      'String',
      'steps',
      'seq',
    ),

    expect_clause: $ => seq('expect', $.number),

    // ---- Shared helpers ----

    visibility: $ => 'private',

    decl_list: $ => seq(
      $.decl,
      repeat(seq(',', $.decl)),
      optional(','),
    ),

    decl: $ => seq(
      optional(choice('disj', 'disjoint')),
      $.name_list,
      ':',
      optional(choice('disj', 'disjoint')),
      $._expression,
    ),

    name_list: $ => seq(
      $._decl_name,
      repeat(seq(',', $._decl_name)),
    ),

    name_list_exact: $ => seq(
      $._name_or_exactly,
      repeat(seq(',', $._name_or_exactly)),
    ),

    _name_or_exactly: $ => choice(
      $._decl_name,
      seq('exactly', $._decl_name),
    ),

    sig_ref: $ => choice(
      $.qual_name,
      'univ',
      'Int',
      'none',
      'String',
      'steps',
      seq('seq', '/', 'Int'),
    ),

    sig_ref_list: $ => seq(
      $.sig_ref,
      repeat(seq(',', $.sig_ref)),
    ),

    sig_ref_union: $ => seq(
      $.sig_ref,
      repeat(seq('+', $.sig_ref)),
    ),

    // ---- Expressions ----

    _expression: $ => choice(
      $.seq_expression,
      $.or_expression,
      $.iff_expression,
      $.implies_expression,
      $.and_expression,
      $.temporal_binary_expression,
      $.unary_expression,
      $.compare_expression,
      $.multiplicity_expression,
      $.shift_expression,
      $.plus_minus_expression,
      $.cardinality_expression,
      $.override_expression,
      $.intersect_expression,
      $.arrow_expression,
      $.domain_expression,
      $.range_expression,
      $.dot_expression,
      $.box_join_expression,
      $.closure_expression,
      $.prime_expression,
      $.quantified_expression,
      $.let_expression,
      $._primary_expression,
    ),

    // ; (trace sequence) - lowest precedence, right-assoc
    seq_expression: $ => prec.right(PREC.SEQ, seq(
      $._expression,
      ';',
      $._expression,
    )),

    // || or
    or_expression: $ => prec.left(PREC.OR, seq(
      $._expression,
      choice('||', 'or'),
      $._expression,
    )),

    // <=> iff
    iff_expression: $ => prec.left(PREC.IFF, seq(
      $._expression,
      choice('<=>', 'iff'),
      $._expression,
    )),

    // => implies (right-assoc, with optional else for if-then-else)
    implies_expression: $ => prec.right(PREC.IMPLIES, seq(
      $._expression,
      choice('=>', 'implies'),
      $._expression,
      optional(seq('else', $._expression)),
    )),

    // && and
    and_expression: $ => prec.left(PREC.AND, seq(
      $._expression,
      choice('&&', 'and'),
      $._expression,
    )),

    // Temporal binary: until since releases triggered
    temporal_binary_expression: $ => prec.left(PREC.TEMP_BINARY, seq(
      $._expression,
      field('operator', choice('until', 'since', 'releases', 'triggered')),
      $._expression,
    )),

    // Unary prefix: ! not always eventually after before historically once
    unary_expression: $ => prec(PREC.NOT, seq(
      field('operator', choice(
        '!', 'not',
        'always', 'eventually', 'after', 'before', 'historically', 'once',
      )),
      $._expression,
    )),

    // Binary comparisons
    compare_expression: $ => prec.left(PREC.COMPARE, seq(
      $._expression,
      field('operator', choice(
        '=', '!=', 'in', '!in',
        // `x not = y` / `x ! in y` — Alloy accepts `not`/`!` before
        // every comparator with whitespace between the words.
        token(prec(1, seq(choice('not', '!'), /\s+/, choice('in', '=', '<', '>', '<=', '>=')))),
        '<', '>', '<=', '>=', '=<',
        '!<', '!>', '!<=', '!>=',
      )),
      $._expression,
    )),

    // Multiplicity tests (unary prefix, higher precedence than binary compare)
    multiplicity_expression: $ => prec(PREC.MULTIPLICITY, seq(
      field('operator', choice('no', 'some', 'lone', 'one', 'set', 'seq')),
      $._expression,
    )),

    // Shift: << >> >>>
    shift_expression: $ => prec.left(PREC.SHIFT, seq(
      $._expression,
      field('operator', choice('<<', '>>>', '>>')),
      $._expression,
    )),

    // Plus/minus: + -
    plus_minus_expression: $ => prec.left(PREC.PLUS_MINUS, seq(
      $._expression,
      field('operator', choice('+', '-')),
      $._expression,
    )),

    // Cardinality/cast: # expr, int expr, sum expr (unary prefix)
    // CUP grammar: NumUnopExpr level, between MulExpr and OverrideExpr
    cardinality_expression: $ => prec(PREC.CARDINALITY, seq(
      field('operator', choice('#', 'int', 'sum', '-')),
      $._expression,
    )),

    // Override: ++
    override_expression: $ => prec.left(PREC.OVERRIDE, seq(
      $._expression,
      '++',
      $._expression,
    )),

    // Intersection: &
    intersect_expression: $ => prec.left(PREC.INTERSECT, seq(
      $._expression,
      '&',
      $._expression,
    )),

    // Arrow: -> with optional multiplicity annotations
    // Left multiplicities are combined into single lexer tokens ('some ->', etc.)
    // to avoid shift-reduce conflict with 'some'/'one'/'lone' starting new
    // expressions (quantified/multiplicity) after another expression in blocks.
    arrow_expression: $ => prec.right(PREC.ARROW, seq(
      $._expression,
      field('operator', choice(
        '->',
        $.some_arrow_op,
        $.one_arrow_op,
        $.lone_arrow_op,
        $.set_arrow_op,
      )),
      optional(field('right_mult', choice('some', 'one', 'lone'))),
      $._expression,
    )),

    some_arrow_op: $ => token(prec(1, seq('some', /[\s]*/, '->'))),
    one_arrow_op: $ => token(prec(1, seq('one', /[\s]*/, '->'))),
    lone_arrow_op: $ => token(prec(1, seq('lone', /[\s]*/, '->'))),
    set_arrow_op: $ => token(prec(1, seq('set', /[\s]*/, '->'))),

    // Domain restriction: <:
    domain_expression: $ => prec.left(PREC.DOMAIN, seq(
      $._expression,
      '<:',
      $._expression,
    )),

    // Range restriction: :>
    range_expression: $ => prec.left(PREC.RANGE, seq(
      $._expression,
      ':>',
      $._expression,
    )),

    // Box join: expr[exprs] — the callee can be a field named like an
    // operator word (`sum[i + j]` calls a `sum` relation).
    box_join_expression: $ => prec.left(PREC.BRACKET, seq(
      choice($._expression, $.member_keyword),
      '[',
      optional($.expr_list),
      ']',
    )),

    // Dot join: . — the member can be a field whose name is an
    // operator word: `e.before`, `x.after`, `y.set`.
    dot_expression: $ => prec.left(PREC.DOT, seq(
      $._expression,
      '.',
      choice($._expression, $.member_keyword),
    )),

    // Operator/keyword words allowed as member names after `.`. The
    // bump prefers the member read: `x.lone y` is `(x.lone)` then `y`,
    // never `x.(lone y)` — Alloy member names may reuse operator words.
    member_keyword: $ => alias(prec(20, choice(
      'before', 'after', 'always', 'eventually', 'historically', 'once',
      'until', 'since', 'releases', 'triggered', 'not', 'and', 'or',
      'implies', 'iff', 'else', 'let', 'all', 'no', 'some', 'lone', 'one',
      'sum', 'disj', 'disjoint', 'in', 'set', 'seq', 'int', 'steps',
      'but', 'expect', 'exactly', 'var', 'private', 'extends', 'module',
      'open', 'abstract', 'sig', 'pred', 'fun', 'fact', 'assert', 'run',
      'check', 'enum', 'for',
    )), $.name),

    // Closure/transpose: ~ ^ *
    closure_expression: $ => prec(PREC.UNOP, seq(
      field('operator', choice('~', '^', '*')),
      $._expression,
    )),

    // Prime: postfix '
    prime_expression: $ => prec(PREC.PRIME, seq(
      $._expression,
      "'",
    )),

    // ---- Quantified expressions ----
    quantified_expression: $ => prec.right(PREC.QUANTIFIER, seq(
      choice('all', 'no', 'some', 'lone', 'one', 'sum'),
      $.decl_list,
      choice(
        seq('|', $._expression),
        $.block_expression,
      ),
    )),

    // ---- Let expression ----
    let_expression: $ => prec.right(PREC.QUANTIFIER, seq(
      'let',
      $.let_binding,
      choice(
        seq('|', $._expression),
        $.block_expression,
      ),
    )),

    let_binding: $ => seq(
      $._decl_name,
      '=',
      $._expression,
      repeat(seq(',', $._decl_name, '=', $._expression)),
    ),

    // ---- Primary expressions ----
    _primary_expression: $ => choice(
      $.number,
      $.string,
      $.qual_name,
      $.at_name,
      $.this_expr,
      $.iden_expr,
      $.none_expr,
      $.univ_expr,
      $.int_expr,
      $.paren_expression,
      $.block_expression,
      $.set_comprehension,
    ),

    this_expr: $ => 'this',
    iden_expr: $ => 'iden',
    none_expr: $ => 'none',
    univ_expr: $ => 'univ',
    int_expr: $ => 'Int',

    at_name: $ => seq('@', $.name),

    paren_expression: $ => seq('(', $._expression, ')'),

    block_expression: $ => seq(
      '{',
      repeat($._expression),
      '}',
    ),

    // `{x: A | e}`, `{x: A { e }}`, `{x: A}` and `{x: A, y: B}` —
    // the `|` predicate (or its `{ }` block spelling) is optional.
    set_comprehension: $ => seq(
      '{',
      $.decl_list,
      optional(choice(
        seq('|', $._expression),
        $.block_expression,
      )),
      '}',
    ),

    expr_list: $ => seq(
      $._expression,
      repeat(seq(',', $._expression)),
    ),

    // ---- Qualified names ----
    qual_name: $ => seq(
      optional(choice(
        seq('this', '/'),
        seq('seq', '/'),
      )),
      $.name,
      repeat(seq('/', $.name)),
    ),

    // ---- Terminals ----
    // `'` is deliberately not part of `name`: the postfix prime operator
    // (`x'` next-state) must keep lexing. Primed declarator spellings
    // (`t'`, `s'`) are covered by `_decl_name`, valid only where a
    // declaration name is expected.
    // The `"` tail matches upstream: `s"` is a primed identifier.
    name: $ => /[\p{L}\p{Nl}_$][\p{L}\p{Nl}\p{Nd}\p{Pc}_$"]*/,

    // A primed declarator name (`t'`, `s''`) — name + `'` literals, the
    // same `'` token prime_expression uses, so `no t': T` (decl) and
    // `no trashed'` (multiplicity + prime) lex identically and GLR
    // picks the surviving branch. A `name` node precedes the primes so
    // adapters keep reading the base spelling.
    _decl_name: $ => seq($.name, repeat("'")),

    number: $ => choice(
      /[0-9][0-9_]*/,             // decimal (with optional underscores)
      /0x([0-9A-Fa-f_])+/,       // hexadecimal
      /0b[01_]+/,                 // binary
    ),

    string: $ => /"([^"\\]|\\.)*"/,

    line_comment: $ => token(choice(
      seq('//', /[^\r\n]*/),
      seq('--', /[^\r\n]*/),
    )),

    block_comment: $ => token(seq(
      '/*',
      /[^*]*\*+([^/*][^*]*\*+)*/,
      '/',
    )),
  },
});
