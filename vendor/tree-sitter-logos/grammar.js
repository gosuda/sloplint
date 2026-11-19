/**
 * @file Go grammar for tree-sitter
 * @author Max Brunsfeld <maxbrunsfeld@gmail.com>
 * @author Amaan Qureshi <amaanq12@gmail.com>
 * @license MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const PREC = {
  primary: 7,
  unary: 6,
  multiplicative: 5,
  additive: 4,
  comparative: 3,
  and: 2,
  or: 1,
  composite_literal: -1,
};

const multiplicativeOperators = ['*', '/', '%', '<<', '>>', '&', '&^'];
const additiveOperators = ['+', '-', '|', '^'];
const comparativeOperators = ['==', '!=', '<', '<=', '>', '>='];
const assignmentOperators = multiplicativeOperators.concat(additiveOperators).map(operator => operator + '=').concat('=');


const newline = /\n/;
const terminator = choice(newline, ';', '\0');

const hexDigit = /[0-9a-fA-F]/;
const octalDigit = /[0-7]/;
const decimalDigit = /[0-9]/;
const binaryDigit = /[01]/;

const hexDigits = seq(hexDigit, repeat(seq(optional('_'), hexDigit)));
const octalDigits = seq(octalDigit, repeat(seq(optional('_'), octalDigit)));
const decimalDigits = seq(decimalDigit, repeat(seq(optional('_'), decimalDigit)));
const binaryDigits = seq(binaryDigit, repeat(seq(optional('_'), binaryDigit)));

const hexLiteral = seq('0', choice('x', 'X'), optional('_'), hexDigits);
const octalLiteral = seq('0', optional(choice('o', 'O')), optional('_'), octalDigits);
const decimalLiteral = choice('0', seq(/[1-9]/, optional(seq(optional('_'), decimalDigits))));
const binaryLiteral = seq('0', choice('b', 'B'), optional('_'), binaryDigits);

const intLiteral = choice(binaryLiteral, decimalLiteral, octalLiteral, hexLiteral);

const decimalExponent = seq(choice('e', 'E'), optional(choice('+', '-')), decimalDigits);
const decimalFloatLiteral = choice(
  seq(decimalDigits, '.', optional(decimalDigits), optional(decimalExponent)),
  seq(decimalDigits, decimalExponent),
  seq('.', decimalDigits, optional(decimalExponent)),
);

const hexExponent = seq(choice('p', 'P'), optional(choice('+', '-')), decimalDigits);
const hexMantissa = choice(
  seq(optional('_'), hexDigits, '.', optional(hexDigits)),
  seq(optional('_'), hexDigits),
  seq('.', hexDigits),
);
const hexFloatLiteral = seq('0', choice('x', 'X'), hexMantissa, hexExponent);

const floatLiteral = choice(decimalFloatLiteral, hexFloatLiteral);

const imaginaryLiteral = seq(choice(decimalDigits, intLiteral, floatLiteral), 'i');

module.exports = grammar({
  name: 'logos',

  extras: $ => [
    $.comment,
    /\s/,
  ],

  externals: $ => [
    // `$"..."` interpolated string literal; the interior may nest strings
    // and braces, so an external scanner finds the closing quote.
    $.interp_string_literal,
  ],

  inline: $ => [
    $._type,
    $._type_identifier,
    $._field_identifier,
    $._selector_field,
    $._label_name,
    $._package_identifier,
    $._top_level_declaration,
    $._string_literal,
    $._interface_elem,
  ],

  word: $ => $.identifier,

  conflicts: $ => [
    [$._simple_type, $._expression],
    [$._simple_type, $.generic_type, $._expression],
    [$.qualified_type, $._expression],
    [$.generic_type, $._simple_type],
    [$.parameter_declaration, $._simple_type],
    [$.type_parameter_declaration, $._simple_type, $._expression],
    [$.type_parameter_declaration, $._expression],
    [$.type_parameter_declaration, $._simple_type, $.generic_type, $._expression],
    // `func (T) m[P any]()` is a generic method; `func (T) m[A, B]` is a func
    // literal whose result is a generic type.
    [$.type_parameter_declaration, $._simple_type],
    // `(*struct{ J })(x)` is a conversion; `(*struct{ J }).M` is a method
    // expression.
    [$._simple_type, $._type_literal],
    // LoGOs forms.
    // `x => e` is a lambda; `(a, b) => e` competes with parenthesized
    // expressions and tuple literals.
    [$.identifier, $._expression],
    [$.parenthesized_expression, $.tuple_literal, $.lambda_expression],
    // `f()?` is a try expression; `T?` in type position is an option type.
    [$.try_expression, $._simple_type],
    // `errdefer`, `yield`, `scope` and `group` are contextual words: they
    // also remain ordinary identifiers.
    [$.errdefer_statement, $._simple_statement],
    [$.errdefer_statement, $._expression],
    [$.yield_statement, $._simple_statement],
    [$.yield_statement, $._expression],
    [$.scope_defer_statement, $._simple_statement],
    [$.go_group_statement, $.go_statement],
    [$.go_group_statement, $._expression],
    // `switch` opens both the statement and the expression form.
    [$.expression_switch_statement, $.switch_expression],
    // `const` opens a declaration, a `const func`, and the const
    // statements; `const if`/`for`/`switch` cannot be specs because the
    // following word is reserved.
    [$.const_declaration, $.const_if_statement],
    [$.const_declaration, $.const_for_statement],
    [$.const_declaration, $.const_switch_statement],
    // Interface elements: `M()` method, `const M()` readonly method,
    // `F T`/`const F T` property, `T | U` embedded union.
    [$.method_elem, $.property_elem],
    [$.type_elem, $.property_elem],
    // `case X => e` arms vs `case X:` statement arms share the `switch`
    // prefix only through distinct parent rules; the element lists differ.
    [$.expression_case, $.switch_arm],
    // `(name T, U)` tuple types vs parenthesized expressions/types.
    [$._tuple_type_elem, $._simple_type, $._expression],
    [$._tuple_type_elem, $._expression],
    [$._tuple_type_elem, $._simple_type, $.generic_type, $._expression],
    // `const x (T) = v`: the optional type in a const spec vs expressions.
    [$.const_spec, $._simple_type],
    [$.const_spec, $._simple_type, $.generic_type],
    [$.const_spec, $._simple_type, $._expression],
    [$.option_type, $.const_view_type],
    [$.parameter_declaration, $._paren_type_content],
    [$.parameter_declaration, $._tuple_type_elem],
    [$._expression, $.lambda_expression],
    [$._type_switch_header, $.switch_expression],
    // `(x) => e` lambdas vs parenthesized expressions.
    // `func (r T) name(` is a method name; `func(x T) R(y)` is a
    // conversion whose type is a function type with an identifier result.
    [$.method_declaration, $.function_type],
    [$._simple_type, $.generic_type, $._field_identifier],
    [$._simple_type, $._expression, $.lambda_expression],
    [$._simple_type, $._field_identifier],
    // `struct { m [N]int }` names a field of array type; `struct { T[U] }`
    // is an anonymous generic-type field.
    [$.generic_type, $._field_identifier],
    // `interface { M(` is a method obligation; `F T` is a property whose
    // type is a bare identifier.
    [$.property_elem, $._field_identifier],
  ],

  reserved: {
    global: $ => [
      // https://go.dev/ref/spec#Keywords
      'break', 'default', 'func', 'interface', 'select',
      'case', 'defer', 'go', 'map', 'struct',
      'chan', 'else', 'goto', 'package', 'switch',
      'const', 'fallthrough', 'if', 'range', 'type',
      'continue', 'for', 'import', 'return', 'var',
    ],
  },

  supertypes: $ => [
    $._expression,
    $._type,
    $._simple_type,
    $._statement,
    $._simple_statement,
  ],

  rules: {
    source_file: $ => seq(
      repeat(choice(
        // Unlike a Go compiler, we accept statements at top-level to enable
        // parsing of partial code snippets in documentation (see #63).
        seq($._statement, terminator),
        seq($._top_level_declaration, terminator),
      )),
      optional($._top_level_declaration),
    ),

    _top_level_declaration: $ => choice(
      $.package_clause,
      $.function_declaration,
      $.method_declaration,
      $.import_declaration,
      $.const_func_declaration,
    ),

    package_clause: $ => seq(
      'package',
      $._package_identifier,
    ),

    import_declaration: $ => seq(
      'import',
      choice(
        $.import_spec,
        $.import_spec_list,
      ),
    ),

    import_spec: $ => seq(
      optional(field('name', choice(
        $.dot,
        $.blank_identifier,
        $._package_identifier,
      ))),
      field('path', $._string_literal),
    ),
    dot: _ => '.',
    blank_identifier: _ => '_',

    import_spec_list: $ => seq(
      '(',
      optional(seq(
        $.import_spec,
        repeat(seq(terminator, $.import_spec)),
        optional(terminator),
      )),
      ')',
    ),

    _declaration: $ => choice(
      $.const_declaration,
      $.type_declaration,
      $.var_declaration,
    ),

    // `const func f(...) ...` is a LoGOs const-evaluated function
    // declaration (SPEC T-201). It reuses the ordinary function and
    // method declaration rules after the `const` keyword.
    const_func_declaration: $ => seq(
      'const',
      choice(
        $.function_declaration,
        $.method_declaration,
      ),
    ),

    const_declaration: $ => seq(
      'const',
      choice(
        $.const_spec,
        seq(
          // The last spec may be followed directly by `)`: `const ( F = 1 )`.
          '(',
          optionalTrailingSep($.const_spec, terminator),
          ')',
        ),
      ),
    ),

    const_spec: $ => prec.left(seq(
      field('name', commaSep1($.identifier)),
      optional(seq(
        optional(field('type', $._type)),
        '=',
        field('value', $.expression_list),
      )),
    )),

    var_declaration: $ => seq(
      'var',
      choice(
        $.var_spec,
        $.var_spec_list,
      ),
    ),

    var_spec: $ => seq(
      commaSep1(field('name', $.identifier)),
      choice(
        seq(
          field('type', $._type),
          optional(seq('=', field('value', $.expression_list))),
        ),
        seq('=', field('value', $.expression_list)),
      ),
    ),

    var_spec_list: $ => seq(
      '(',
      repeat(seq($.var_spec, optional(terminator))),
      ')',
    ),

    function_declaration: $ => prec.right(1, seq(
      'func',
      field('name', $.identifier),
      field('type_parameters', optional($.type_parameter_list)),
      field('parameters', $.parameter_list),
      field('result', optional(choice($.parameter_list, $._simple_type))),
      field('body', optional($.block)),
    )),

    method_declaration: $ => prec.right(1, seq(
      'func',
      field('receiver', $.parameter_list),
      field('name', $._field_identifier),
      field('type_parameters', optional($.type_parameter_list)),
      field('parameters', $.parameter_list),
      field('result', optional(choice($.parameter_list, $._simple_type))),
      field('body', optional($.block)),
    )),

    type_parameter_list: $ => seq(
      '[',
      commaSep1($.type_parameter_declaration),
      optional(','),
      ']',
    ),

    type_parameter_declaration: $ => seq(
      commaSep1(field('name', $.identifier)),
      field('type', alias($.type_elem, $.type_constraint)),
    ),

    parameter_list: $ => seq(
      '(',
      optional(seq(
        commaSep(choice($.parameter_declaration, $.variadic_parameter_declaration)),
        optional(','),
      )),
      ')',
    ),

    parameter_declaration: $ => seq(
      commaSep(field('name', $.identifier)),
      field('type', $._type),
    ),

    variadic_parameter_declaration: $ => seq(
      field('name', optional($.identifier)),
      '...',
      field('type', $._type),
    ),

    type_alias: $ => seq(
      field('name', $._type_identifier),
      field('type_parameters', optional($.type_parameter_list)),
      '=',
      field('type', $._type),
    ),

    type_declaration: $ => seq(
      'type',
      choice(
        $.type_spec,
        $.type_alias,
        seq(
          '(',
          optionalTrailingSep(choice($.type_spec, $.type_alias), terminator),
          ')',
        ),
      ),
    ),

    type_spec: $ => seq(
      field('name', $._type_identifier),
      field('type_parameters', optional($.type_parameter_list)),
      field('type', $._type),
    ),

    field_name_list: $ => commaSep1($._field_identifier),

    expression_list: $ => commaSep1($._expression),

    _type: $ => choice(
      $._simple_type,
      $.parenthesized_type,
    ),

    // `(T)` stays a parenthesized type; `(T, U)` and `(name T, ...)` are
    // LoGOs tuple types (SPEC T-204). The parentheses belong to
    // parenthesized_type; tuple_type is the comma-separated content and
    // needs at least two elements.
    parenthesized_type: $ => seq('(', $._paren_type_content, ')'),

    _paren_type_content: $ => choice(
      $._type,
      $.tuple_type,
    ),

    tuple_type: $ => seq(
      $._tuple_type_elem,
      ',',
      commaSep1($._tuple_type_elem),
      optional(','),
    ),

    // One element of a tuple type: a plain type, `name: type`, or
    // `name type` (a named field without the colon).
    _tuple_type_elem: $ => choice(
      $._type,
      seq(
        field('name', $.identifier),
        ':',
        field('type', $._type),
      ),
      seq(
        field('name', $.identifier),
        field('type', $._type),
      ),
    ),

    // `T?` is the LoGOs option type (SPEC T-202). `T??` cannot parse
    // because `??` lexes as the coalesce operator, not two `?` marks.
    // `(T?)?` nests through parenthesized_type.
    option_type: $ => seq($._type, '?'),

    // `const T` is a const view (SPEC T-201): `const []int`, `const *T`,
    // `const map[K]V`, `const int`, and so on.
    const_view_type: $ => seq('const', $._type),

    _simple_type: $ => choice(
      prec.dynamic(-1, $._type_identifier),
      $.generic_type,
      $.qualified_type,
      $.pointer_type,
      $.struct_type,
      $.interface_type,
      $.array_type,
      $.slice_type,
      prec.dynamic(3, $.map_type),
      $.channel_type,
      $.function_type,
      $.negated_type,
      $.option_type,
      $.const_view_type,
    ),

    generic_type: $ => prec.dynamic(1, seq(
      field('type', choice($._type_identifier, $.qualified_type, $.negated_type)),
      field('type_arguments', $.type_arguments),
    )),

    type_arguments: $ => prec.dynamic(2, seq(
      '[',
      commaSep1($.type_elem),
      optional(','),
      ']',
    )),

    pointer_type: $ => prec(PREC.unary, seq('*', $._type)),

    array_type: $ => prec.right(seq(
      '[',
      field('length', $._expression),
      ']',
      field('element', $._type),
    )),

    implicit_length_array_type: $ => seq(
      '[',
      '...',
      ']',
      field('element', $._type),
    ),

    slice_type: $ => prec.right(seq(
      '[',
      ']',
      field('element', $._type),
    )),

    // `struct : I, J { ... }` is the LoGOs asserted-struct form
    // (SPEC T-203.3); the interface list sits between `struct` and `{`.
    struct_type: $ => seq(
      'struct',
      optional(seq(
        ':',
        commaSep1($._type),
      )),
      $.field_declaration_list,
    ),

    negated_type: $ => prec.left(seq(
      '~',
      $._type,
    )),

    field_declaration_list: $ => seq(
      '{',
      optional(seq(
        $.field_declaration,
        repeat(seq(terminator, $.field_declaration)),
        optional(terminator),
      )),
      '}',
    ),

    field_declaration: $ => seq(
      choice(
        seq(
          commaSep1(field('name', $._field_identifier)),
          field('type', $._type),
        ),
        seq(
          optional('*'),
          field('type', choice(
            $._type_identifier,
            $.qualified_type,
            $.generic_type,
          )),
        ),
      ),
      // LoGOs field default: `Port int = 8080` before the tag (SPEC T-104).
      field('default', optional(seq(
        '=',
        $._expression,
      ))),
      field('tag', optional($._string_literal)),
    ),

    interface_type: $ => seq(
      'interface',
      '{',
      optional(seq(
        $._interface_elem,
        repeat(seq(terminator, $._interface_elem)),
        optional(terminator),
      )),
      '}',
    ),

    _interface_elem: $ => choice(
      $.method_elem,
      $.type_elem,
      $.property_elem,
    ),

    // `M()` is a method obligation; `const M()` is a readonly method
    // obligation (SPEC T-203.2). Both share one rule.
    method_elem: $ => seq(
      optional('const'),
      field('name', $._field_identifier),
      field('parameters', $.parameter_list),
      field('result', optional(choice($.parameter_list, $._simple_type))),
    ),

    type_elem: $ => sep1($._type, '|'),

    // `F T` and `const F T` are LoGOs property elements (SPEC T-203.2).
    property_elem: $ => seq(
      optional('const'),
      field('name', $.identifier),
      field('type', $._type),
    ),

    map_type: $ => prec.right(seq(
      'map',
      '[',
      field('key', $._type),
      ']',
      field('value', $._type),
    )),

    channel_type: $ => prec.left(choice(
      seq('chan', field('value', $._type)),
      seq('chan', '<-', field('value', $._type)),
      prec(PREC.unary, seq('<-', 'chan', field('value', $._type))),
    )),

    function_type: $ => prec.right(seq(
      'func',
      field('parameters', $.parameter_list),
      field('result', optional(choice($.parameter_list, $._simple_type))),
    )),

    block: $ => seq(
      '{',
      optional($.statement_list),
      '}',
    ),

    // An empty statement is its own terminator: it needs no further
    // terminator before the next statement (`{ ; f() }`, `f();; g()`).
    statement_list: $ => choice(
      seq(
        repeat1(choice(
          seq($._non_empty_statement, terminator),
          $.empty_statement,
        )),
        optional($._last_statement),
      ),
      $._last_statement,
    ),

    _last_statement: $ => choice(
      $._non_empty_statement,
      alias($.empty_labeled_statement, $.labeled_statement),
    ),

    _statement: $ => choice(
      $._non_empty_statement,
      $.empty_statement,
    ),

    _non_empty_statement: $ => choice(
      $._declaration,
      $._simple_statement,
      $.return_statement,
      $.go_statement,
      $.go_group_statement,
      $.group_child_statement,
      $.defer_statement,
      $.errdefer_statement,
      $.scope_defer_statement,
      $.yield_statement,
      $.const_if_statement,
      $.const_for_statement,
      $.const_switch_statement,
      $.if_statement,
      $.for_statement,
      $.expression_switch_statement,
      $.type_switch_statement,
      $.select_statement,
      $.labeled_statement,
      $.fallthrough_statement,
      $.break_statement,
      $.continue_statement,
      $.goto_statement,
      $.block,
    ),

    empty_statement: _ => ';',

    _simple_statement: $ => choice(
      $.expression_statement,
      $.send_statement,
      $.inc_statement,
      $.dec_statement,
      $.assignment_statement,
      $.short_var_declaration,
    ),

    expression_statement: $ => $._expression,

    send_statement: $ => seq(
      field('channel', $._expression),
      '<-',
      field('value', $._expression),
    ),

    receive_statement: $ => seq(
      optional(seq(
        field('left', $.expression_list),
        choice('=', ':='),
      )),
      field('right', $._expression),
    ),

    inc_statement: $ => seq(
      $._expression,
      '++',
    ),

    dec_statement: $ => seq(
      $._expression,
      '--',
    ),

    assignment_statement: $ => seq(
      field('left', $.expression_list),
      field('operator', choice(...assignmentOperators)),
      field('right', $.expression_list),
    ),

    short_var_declaration: $ => seq(
      // TODO: this should really only allow identifier lists, but that causes
      // conflicts between identifiers as expressions vs identifiers here.
      field('left', $.expression_list),
      ':=',
      field('right', $.expression_list),
    ),

    // The contextual words `errdefer`, `yield`, `scope` and `group` stay
    // ordinary names, so they also label statements (contextual_words.lgo).
    labeled_statement: $ => seq(
      field('label', $._label_name),
      ':',
      $._statement,
    ),

    // Labels at the end of a block nest: `L:` newline `L1:` newline `}`.
    empty_labeled_statement: $ => seq(
      field('label', $._label_name),
      ':',
      optional(alias($.empty_labeled_statement, $.labeled_statement)),
    ),

    // This is a hack to prevent `fallthrough_statement` from being parsed as
    // a single token. For consistency with `break_statement` etc it should
    // be parsed as a parent node that *contains* a `fallthrough` token.
    fallthrough_statement: _ => prec.left('fallthrough'),

    break_statement: $ => seq('break', optional($._label_name)),

    continue_statement: $ => seq('continue', optional($._label_name)),

    goto_statement: $ => seq('goto', $._label_name),

    return_statement: $ => seq('return', optional($.expression_list)),

    go_statement: $ => seq('go', $._expression),

    // `go { ... }` is a LoGOs group child block (SPEC T-206): an
    // ordinary statement list inside a `go group`.
    group_child_statement: $ => seq('go', $.block),

    // `go group (ctx) { ... }` and `go group (ctx, limit = N) { ... }`
    // are LoGOs structured-concurrency groups (SPEC T-206). Without a
    // trailing block, `go group(args)` stays an ordinary `go` call of a
    // function named `group`.
    go_group_statement: $ => seq(
      'go',
      'group',
      '(',
      commaSep(choice($._expression, $.named_argument)),
      optional(','),
      ')',
      $.block,
    ),

    // `defer f()` plus the LoGOs try form `defer f()?` (SPEC T-104.1).
    defer_statement: $ => seq('defer', $._expression, optional('?')),

    // `errdefer` is a contextual word: it starts a statement only before
    // an expression (SPEC T-104.2), and `errdefer f()?` unwraps the
    // error. `errdefer := 1` stays an assignment because `:=` cannot
    // start an expression here.
    errdefer_statement: $ => seq('errdefer', $._expression, optional('?')),

    // `scope defer f()` runs f at the end of the enclosing block
    // (SPEC T-104).
    scope_defer_statement: $ => seq('scope', 'defer', $._expression),

    // `yield e` yields a value from a generator (SPEC T-104). `yield`
    // stays an ordinary identifier elsewhere (`yield := 1`, `yield()`).
    yield_statement: $ => seq('yield', $.expression_list),

    // `const if cond { } else { }` / `else const if` (SPEC T-205).
    const_if_statement: $ => seq(
      'const',
      'if',
      field('condition', $._expression),
      field('consequence', $.block),
      optional(seq(
        'else',
        field('alternative', choice($.block, $.const_if_statement)),
      )),
    ),

    // `const for i, f := range expr { }` iterates comptime fields
    // (SPEC T-205).
    const_for_statement: $ => seq(
      'const',
      'for',
      field('left', $.expression_list),
      choice('=', ':='),
      'range',
      field('right', $._expression),
      $.block,
    ),

    // `const switch T { case U: ...; case ~V: ... }` switches on a type
    // at compile time (SPEC T-205). There is no default arm.
    const_switch_statement: $ => seq(
      'const',
      'switch',
      field('type', $._type),
      '{',
      repeat($.const_case),
      '}',
    ),

    const_case: $ => seq(
      'case',
      optional('~'),
      field('type', $._type),
      ':',
      optional($.statement_list),
    ),

    // The initializer may be empty: `if ; x {}`, `switch ; {}`.
    if_statement: $ => seq(
      'if',
      optional(seq(
        field('initializer', optional($._simple_statement)),
        ';',
      )),
      field('condition', $._expression),
      field('consequence', $.block),
      optional(seq(
        'else',
        field('alternative', choice($.block, $.if_statement)),
      )),
    ),

    for_statement: $ => seq(
      'for',
      optional(choice($._expression, $.for_clause, $.range_clause)),
      field('body', $.block),
    ),

    for_clause: $ => seq(
      field('initializer', optional($._simple_statement)),
      ';',
      field('condition', optional($._expression)),
      ';',
      field('update', optional($._simple_statement)),
    ),

    range_clause: $ => seq(
      optional(seq(
        field('left', $.expression_list),
        choice('=', ':='),
      )),
      'range',
      field('right', $._expression),
    ),

    // A switch whose body has only `:`-arms or no arms at all is the Go
    // statement form; `switch_expression` (LoGOs, `=>` arms) loses ties so
    // the Go adapter still sees `expression_switch_statement`.
    expression_switch_statement: $ => prec(1, seq(
      'switch',
      optional(seq(
        field('initializer', optional($._simple_statement)),
        ';',
      )),
      field('value', optional($._expression)),
      '{',
      repeat(choice($.expression_case, $.default_case)),
      '}',
    )),

    expression_case: $ => seq(
      'case',
      field('value', $.expression_list),
      ':',
      optional($.statement_list),
    ),

    default_case: $ => seq(
      'default',
      ':',
      optional($.statement_list),
    ),

    type_switch_statement: $ => prec(1, seq(
      'switch',
      $._type_switch_header,
      '{',
      repeat(choice($.type_case, $.default_case)),
      '}',
    )),

    _type_switch_header: $ => prec(1, seq(
      optional(seq(
        field('initializer', optional($._simple_statement)),
        ';',
      )),
      optional(seq(field('alias', $.expression_list), ':=')),
      field('value', $._expression),
      '.',
      '(',
      'type',
      ')',
    )),

    type_case: $ => seq(
      'case',
      field('type', commaSep1($._type)),
      ':',
      optional($.statement_list),
    ),

    select_statement: $ => seq(
      'select',
      '{',
      repeat(choice($.communication_case, $.default_case)),
      '}',
    ),

    communication_case: $ => seq(
      'case',
      field('communication', choice($.send_statement, $.receive_statement)),
      ':',
      optional($.statement_list),
    ),

    _expression: $ => choice(
      $.unary_expression,
      $.binary_expression,
      $.coalesce_expression,
      $.selector_expression,
      $.opt_selector_expression,
      $.index_expression,
      $.slice_expression,
      $.call_expression,
      $.type_assertion_expression,
      $.type_conversion_expression,
      $.type_instantiation_expression,
      $.try_expression,
      $.field_projection,
      $.identifier,
      alias(choice('new', 'make'), $.identifier),
      // `group`, `errdefer`, `yield` and `scope` are contextual words:
      // statements use them as keywords, but they stay ordinary
      // identifiers everywhere else (SPEC T-104.2).
      alias(choice('group', 'errdefer', 'yield', 'scope'), $.identifier),
      $.composite_literal,
      $.func_literal,
      $.lambda_expression,
      $._string_literal,
      $.int_literal,
      $.float_literal,
      $.imaginary_literal,
      $.rune_literal,
      $.nil,
      $.true,
      $.false,
      $.iota,
      $.switch_expression,
      $.parenthesized_expression,
      $.tuple_literal,
    ),

    // `f()?`, `x?`, `a.b?` — the LoGOs try operator (SPEC T-104.1). It is
    // a postfix on any operand; in type position `T?` is option_type.
    try_expression: $ => prec.right(PREC.primary, seq(
      field('operand', $._expression),
      '?',
    )),

    // `x?.name` optional selection (SPEC T-104.3). `?.` lexes as one
    // token only when `?` and `.` are adjacent.
    opt_selector_expression: $ => prec.left(PREC.primary, seq(
      field('operand', $._expression),
      '?.',
      field('field', $._selector_field),
    )),

    // `v.[f]` field projection inside comptime loops (SPEC T-205).
    field_projection: $ => prec(PREC.primary, seq(
      field('operand', $._expression),
      '.',
      '[',
      field('field', $.identifier),
      ']',
    )),

    // `x => e`, `() => e`, `(a, b) => e`, `x => { ... }` lambdas
    // (SPEC T-104.6). The parenthesized parameter list holds plain names
    // and may be empty or carry a trailing comma.
    lambda_expression: $ => seq(
      choice(
        field('parameters', $.identifier),
        seq(
          '(',
          optional(seq(commaSep1($.identifier), optional(','))),
          ')',
        ),
      ),
      '=>',
      field('body', choice($._expression, $.block)),
    ),

    // `(a, b)` tuple literal (SPEC T-204); needs at least two elements
    // so `(a)` stays a parenthesized expression.
    tuple_literal: $ => seq(
      '(',
      $._expression,
      ',',
      commaSep1($._expression),
      optional(','),
      ')',
    ),

    // `a ?? b` nullish coalescing (SPEC T-104.3). It binds less tightly
    // than `||` and groups to the right.
    coalesce_expression: $ => prec.right(-2, seq(
      field('left', $._expression),
      '??',
      field('right', $._expression),
    )),

    // `switch` as an expression with `=>` arms (SPEC T-104.6). Like the
    // statement forms, the header chooses the arm flavor: a plain or
    // absent tag takes value arms (`switch_arm`, as in
    // `expression_case`), while a type-switch guard `x.(type)` takes
    // type arms (`switch_type_arm`, as in `type_case`). The guard and a
    // selector tag diverge on the token after `.`, so no conflict arises.
    switch_expression: $ => choice(
      seq(
        'switch',
        optional(seq(
          field('initializer', optional($._simple_statement)),
          ';',
        )),
        field('tag', optional($._expression)),
        '{',
        // Arms are separated by `;` (newlines are extras).
        repeat(seq(choice($.switch_arm, $.switch_default_arm), optional(';'))),
        '}',
      ),
      seq(
        'switch',
        optional(seq(
          field('initializer', optional($._simple_statement)),
          ';',
        )),
        field('tag', seq(
          $._expression,
          '.',
          '(',
          'type',
          ')',
        )),
        '{',
        // Arms are separated by `;` (newlines are extras).
        repeat(seq(choice($.switch_type_arm, $.switch_default_arm), optional(';'))),
        '}',
      ),
    ),

    switch_arm: $ => seq(
      'case',
      field('value', commaSep1($._expression)),
      optional(','),
      '=>',
      field('body', $._expression),
    ),

    // Type-guarded `switch x.(type)` arms accept types such as `[]byte`,
    // which have no value-expression spelling. Statement-form type cases
    // are untouched.
    switch_type_arm: $ => seq(
      'case',
      field('type', commaSep1($._type)),
      optional(','),
      '=>',
      field('body', $._expression),
    ),

    switch_default_arm: $ => seq(
      'default',
      '=>',
      field('body', $._expression),
    ),

    parenthesized_expression: $ => seq(
      '(',
      $._expression,
      ')',
    ),

    call_expression: $ => prec(PREC.primary, choice(
      seq(
        field('function', alias(choice('new', 'make'), $.identifier)),
        field('arguments', alias($.special_argument_list, $.argument_list)),
      ),
      seq(
        field('function', $._expression),
        field('type_arguments', optional($.type_arguments)),
        field('arguments', $.argument_list),
      ),
    )),

    variadic_argument: $ => prec.right(seq(
      $._expression,
      '...',
    )),

    special_argument_list: $ => seq(
      '(',
      optional(seq(
        // Go 1.26 accepts a value: `new(42)`, `new(int64(-64))`. A type still
        // wins when both readings fit (`new(int)`, `new(a.B)`).
        choice(prec.dynamic(2, $._type), $._expression),
        repeat(seq(',', $._expression)),
        optional(','),
      )),
      ')',
    ),

    // `..expr` spread in argument lists mirrors composite literals:
    // `f(..args)` and `f(a, ..rest)` are calls with spread arguments.
    argument_list: $ => seq(
      '(',
      optional(seq(
        commaSep1(choice(
          $._expression,
          $.variadic_argument,
          $.named_argument,
          $.spread_element
        )),
        optional(','),
      )),
      ')',
    ),

    // `name = value` inside a call argument list (LoGOs named
    // arguments, e.g. `limit = 2` in `go group`).
    named_argument: $ => seq(
      field('name', $.identifier),
      '=',
      field('value', $._expression),
    ),

    literal_element: $ => choice($._expression, $.literal_value, $.spread_element),

    // `..expr` spread element in a composite literal (SPEC T-104).
    spread_element: $ => seq('..', $._expression),

    selector_expression: $ => prec(PREC.primary, seq(
      field('operand', choice(
        $._expression,
        // Method expressions on type literals: `interface{ M() }.M`,
        // `(*struct{ I }).M`. Named types stay expressions: `(*T).M`.
        $.struct_type,
        $.interface_type,
        alias($._parenthesized_type_literal, $.parenthesized_type),
      )),
      '.',
      field('field', $._selector_field),
    )),

    // Only types that cannot also be expressions, so `(*T).M` keeps parsing as
    // a parenthesized expression.
    _parenthesized_type_literal: $ => seq('(', $._type_literal, ')'),

    _type_literal: $ => choice(
      $.struct_type,
      $.interface_type,
      alias($._pointer_type_literal, $.pointer_type),
      alias($._parenthesized_type_literal, $.parenthesized_type),
    ),

    _pointer_type_literal: $ => seq('*', $._type_literal),

    index_expression: $ => prec(PREC.primary, prec.dynamic(1, seq(
      field('operand', $._expression),
      '[',
      field('index', $._expression),
      ']',
    ))),

    slice_expression: $ => prec(PREC.primary, seq(
      field('operand', $._expression),
      '[',
      choice(
        seq(
          field('start', optional($._expression)),
          ':',
          field('end', optional($._expression)),
        ),
        seq(
          field('start', optional($._expression)),
          ':',
          field('end', $._expression),
          ':',
          field('capacity', $._expression),
        ),
      ),
      ']',
    )),

    type_assertion_expression: $ => prec(PREC.primary, seq(
      field('operand', $._expression),
      '.',
      '(',
      field('type', $._type),
      ')',
    )),

    type_conversion_expression: $ => prec.dynamic(-1, seq(
      field('type', $._type),
      '(',
      field('operand', $._expression),
      optional(','),
      ')',
    )),

    type_instantiation_expression: $ => prec.dynamic(-1, seq(
      field('type', $._type),
      '[',
      commaSep1($._type),
      optional(','),
      ']',
    )),

    composite_literal: $ => prec(PREC.composite_literal, seq(
      field('type', choice(
        $.map_type,
        $.slice_type,
        $.array_type,
        $.implicit_length_array_type,
        $.struct_type,
        $.const_view_type,
        $._type_identifier,
        $.generic_type,
        $.qualified_type,
      )),
      field('body', $.literal_value),
    )),

    literal_value: $ => seq(
      '{',
      optional(
        seq(
          commaSep(choice($.literal_element, $.keyed_element)),
          optional(','))),
      '}',
    ),


    // In T{k: v}, the key k may be:
    // - any expression (when T is a map, slice or array),
    // - a field identifier (when T is a struct), or
    // - a literal_element (when T is an array).
    // The first two cases cannot be distinguished without type information.
    keyed_element: $ => seq(
      field('key', $.literal_element),
      ':',
      field('value', $.literal_element),
    ),

    func_literal: $ => seq(
      'func',
      field('parameters', $.parameter_list),
      field('result', optional(choice($.parameter_list, $._simple_type))),
      field('body', $.block),
    ),

    unary_expression: $ => prec(PREC.unary, seq(
      field('operator', choice('+', '-', '!', '^', '*', '&', '<-')),
      field('operand', $._expression),
    )),

    binary_expression: $ => {
      const table = [
        [PREC.multiplicative, choice(...multiplicativeOperators)],
        [PREC.additive, choice(...additiveOperators)],
        [PREC.comparative, choice(...comparativeOperators)],
        [PREC.and, '&&'],
        [PREC.or, '||'],
      ];

      return choice(...table.map(([precedence, operator]) =>
        // @ts-ignore
        prec.left(precedence, seq(
          field('left', $._expression),
          // @ts-ignore
          field('operator', operator),
          field('right', $._expression),
        )),
      ));
    },

    qualified_type: $ => seq(
      field('package', $._package_identifier),
      '.',
      field('name', $._type_identifier),
    ),

    identifier: _ => /[_\p{XID_Start}][_\p{XID_Continue}]*/,

    _type_identifier: $ => alias($.identifier, $.type_identifier),
    _field_identifier: $ => alias($.identifier, $.field_identifier),
    // LoGOs admits the keyword `map` as the name after `.` (iterator
    // chains, `xs.map(f)`) and nowhere else; other keywords error there.
    _selector_field: $ => choice(
      alias($.identifier, $.field_identifier),
      alias('map', $.field_identifier),
    ),
    _package_identifier: $ => alias($.identifier, $.package_identifier),
    _label_name: $ => alias(
      choice($.identifier, 'errdefer', 'yield', 'scope', 'group'),
      $.label_name,
    ),
    _string_literal: $ => choice(
      $.raw_string_literal,
      $.interpreted_string_literal,
      $.interp_string_literal,
    ),
    raw_string_literal: $ => seq(
      '`',
      alias(token(prec(1, /[^`]*/)), $.raw_string_literal_content),
      '`',
    ),

    interpreted_string_literal: $ => seq(
      '"',
      repeat(choice(
        alias(token.immediate(prec(1, /[^"\n\\]+/)), $.interpreted_string_literal_content),
        $.escape_sequence,
      )),
      token.immediate('"'),
    ),

    escape_sequence: _ => token.immediate(seq(
      '\\',
      choice(
        /[^xuU]/,
        /\d{2,3}/,
        /x[0-9a-fA-F]{2,}/,
        /u[0-9a-fA-F]{4}/,
        /U[0-9a-fA-F]{8}/,
      ),
    )),

    int_literal: _ => token(intLiteral),

    float_literal: _ => token(floatLiteral),

    imaginary_literal: _ => token(imaginaryLiteral),

    rune_literal: _ => token(seq(
      '\'',
      choice(
        /[^'\\]/,
        seq(
          '\\',
          choice(
            seq('x', hexDigit, hexDigit),
            seq(octalDigit, octalDigit, octalDigit),
            seq('u', hexDigit, hexDigit, hexDigit, hexDigit),
            seq('U', hexDigit, hexDigit, hexDigit, hexDigit, hexDigit, hexDigit, hexDigit, hexDigit),
            seq(choice('a', 'b', 'f', 'n', 'r', 't', 'v', '\\', '\'', '"')),
          ),
        ),
      ),
      '\'',
    )),

    nil: _ => 'nil',
    true: _ => 'true',
    false: _ => 'false',
    iota: _ => 'iota',

    // http://stackoverflow.com/questions/13014947/regex-to-match-a-c-style-multiline-comment/36328890#36328890
    comment: _ => token(choice(
      seq('//', /.*/),
      seq(
        '/*',
        /[^*]*\*+([^/*][^*]*\*+)*/,
        '/',
      ),
    )),
  },
});

/**
 * Creates a rule to match one or more occurrences of `rule` separated by `sep`
 *
 * @param {RuleOrLiteral} rule
 *
 * @param {RuleOrLiteral} separator
 *
 * @returns {SeqRule}
 */
function sep1(rule, separator) {
  return seq(rule, repeat(seq(separator, rule)));
}

/**
 * Creates a rule to match zero or more of the rules separated by {sep},
 * with an optional one at the end.
 *
 * @param {RuleOrLiteral} rule
 *
 * @param {RuleOrLiteral} separator
 */
function optionalTrailingSep(rule, separator) {
  return optional(seq(rule, repeat(seq(separator, rule)), optional(separator)));
}

/**
 * Creates a rule to match one or more of the rules separated by a comma
 *
 * @param {Rule} rule
 *
 * @returns {SeqRule}
 */
function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}

/**
 * Creates a rule to optionally match one or more of the rules separated by a comma
 *
 * @param {Rule} rule
 *
 * @returns {ChoiceRule}
 */
function commaSep(rule) {
  return optional(commaSep1(rule));
}
