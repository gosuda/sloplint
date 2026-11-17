/**
 * @file C grammar for tree-sitter
 * @author Max Brunsfeld <maxbrunsfeld@gmail.com>
 * @author Amaan Qureshi <amaanq12@gmail.com>
 * @license MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const PREC = {
  PAREN_DECLARATOR: -10,
  ASSIGNMENT: -2,
  CONDITIONAL: -1,
  DEFAULT: 0,
  LOGICAL_OR: 1,
  LOGICAL_AND: 2,
  INCLUSIVE_OR: 3,
  EXCLUSIVE_OR: 4,
  BITWISE_AND: 5,
  EQUAL: 6,
  RELATIONAL: 7,
  OFFSETOF: 8,
  SHIFT: 9,
  ADD: 10,
  MULTIPLY: 11,
  CAST: 12,
  SIZEOF: 13,
  UNARY: 14,
  CALL: 15,
  FIELD: 16,
  SUBSCRIPT: 17,
};

module.exports = grammar({
  name: 'c',

  conflicts: $ => [
    [$._macro_qualifier_bare, $._declarator, $.type_specifier],
    [$._macro_qualifier_bare, $.type_specifier, $.sized_type_specifier],
    [$._macro_qualifier_bare, $.sized_type_specifier],
    [$.macro_qualifier, $._macro_qualifier_bare, $.type_specifier],
    [$.type_specifier, $._declarator],
    [$.type_specifier, $.expression],
    [$.type_specifier, $.expression, $.macro_type_specifier],
    [$.type_specifier, $.macro_type_specifier],
    [$.type_specifier, $.sized_type_specifier],
    [$.sized_type_specifier],
    [$.attributed_statement],
    [$._declaration_modifiers, $.attributed_statement],
    [$.enum_specifier],
    [$.type_specifier, $._old_style_parameter_list],
    [$._macro_qualifier_bare, $.type_specifier],
    [$.parameter_list, $._old_style_parameter_list],
    [$.function_declarator, $._function_declaration_declarator],
    [$._block_item, $.statement],
    [$._top_level_item, $._top_level_statement],
    [$.type_qualifier, $.extension_expression],
    [$._declarator, $._declaration_declarator],
    [$._linkage_guard, $.preproc_if],
    [$._linkage_guard, $.preproc_if_in_statement],
    [$._linkage_guard, $.preproc_ifdef_in_statement],
    [$._declaration_modifiers, $.type_specifier],
    [$._linkage_guard, $.preproc_ifdef],
    [$._typedef_macro_qualifier, $.type_specifier, $.sized_type_specifier],
    [$._typedef_macro_qualifier, $.type_specifier],
    [$._field_declaration_list_item, $.field_declaration],
    [$._midl_attribute, $.expression],
    [$.macro_qualifier, $._macro_declaration_stmt],
    [$._macro_declaration_stmt, $.expression],
    [$.macro_qualifier, $._macro_declaration_stmt, $.type_specifier, $.expression, $.macro_type_specifier],
    [$._macro_declaration_top],
    [$.macro_qualifier, $._macro_declaration_top],
    [$._macro_declaration_top, $._top_level_expression_statement],
    [$._macro_declaration_top, $.type_specifier, $._top_level_expression_statement],
    [$.macro_qualifier, $._macro_declaration_top, $.type_specifier, $.expression, $.macro_type_specifier],
    [$.macro_argument_list, $.type_specifier, $.macro_type_specifier],
    [$.preproc_else_in_enumerator_list, $.preproc_else_in_enumerator_list_no_comma, $.expression, $.concatenated_string],
    [$.preproc_else_in_enumerator_list, $.preproc_else_in_enumerator_list_no_comma, $.concatenated_string],
    [$.preproc_ifdef_in_enumerator_list, $.preproc_ifdef_in_enumerator_list_no_comma, $.expression, $.concatenated_string],
    [$.preproc_ifdef_in_enumerator_list, $.preproc_ifdef_in_enumerator_list_no_comma, $.concatenated_string],
    [$.concatenated_string],
    [$.enumerator_list, $.expression, $.concatenated_string],
    [$.enumerator_list, $.concatenated_string],
    [$.enumerator, $.concatenated_string],
    [$.macro_qualifier, $._declaration_declarator],
    [$.macro_qualifier, $._declarator, $._declaration_declarator],
    [$.extension_expression],
    [$.preproc_else_in_enumerator_list, $.preproc_else_in_enumerator_list_no_comma, $.extension_expression],
    [$.preproc_else_in_enumerator_list, $.preproc_else_in_enumerator_list_no_comma],
    [$.enumerator, $.assignment_expression, $.binary_expression],
    [$.preproc_ifdef_in_enumerator_list, $.preproc_ifdef_in_enumerator_list_no_comma, $.extension_expression],
    [$.preproc_ifdef_in_enumerator_list, $.preproc_ifdef_in_enumerator_list_no_comma],
    [$.preproc_else, $.type_qualifier],
    [$.preproc_else, $.type_definition],
    [$.preproc_else, $.extension_expression],
    [$.preproc_else, $.type_qualifier, $.extension_expression],
    [$.preproc_else_in_statement, $.type_qualifier],
    [$.preproc_else_in_statement, $.type_definition],
    [$.preproc_else_in_statement, $.extension_expression],
    [$.preproc_else_in_statement, $.type_qualifier, $.extension_expression],
    [$.enumerator_list, $.expression],
    [$.enumerator, $.expression],
    [$.enumerator_list, $.extension_expression],
    [$.preproc_ifdef, $.type_qualifier],
    [$.preproc_ifdef, $.type_definition],
    [$.preproc_ifdef, $.extension_expression],
    [$.preproc_ifdef, $.type_qualifier, $.extension_expression],
    [$.preproc_ifdef_in_statement, $.type_qualifier],
    [$.preproc_ifdef_in_statement, $.type_definition],
    [$.preproc_ifdef_in_statement, $.extension_expression],
    [$.preproc_ifdef_in_statement, $.type_qualifier, $.extension_expression],
    [$._string, $.concatenated_string],
    [$.preproc_else_in_initializer_list, $.field_expression],
    [$.preproc_ifdef_in_initializer_list, $.field_expression],
    [$.preproc_else_in_argument_list, $.field_expression],
    [$.comma_expression, $.binary_expression],
    [$.preproc_ifdef_in_argument_list, $.field_expression],
    [$.parenthesized_expression],
    [$.binary_expression, $.parenthesized_expression],
    [$.labeled_statement, $.dangling_label],
    [$.expression, $.field_designator],
    [$.binary_expression, $.initializer_pair],
    [$.binary_expression, $._initializer_list_real],
    [$.binary_expression],
    [$.binary_expression, $.extension_expression],
    [$._parameter_list_real, $._old_style_parameter_list],
    [$._parameter_list_item],
    [$.expression, $.concatenated_string],
    [$.attributed_declarator, $.parameter_declaration],
    [$.parameter_declaration],
    [$._declaration_modifiers, $.parenthesized_declarator, $.abstract_parenthesized_declarator, $.pointer_declarator, $.abstract_pointer_declarator, $.type_specifier],
    [$.parenthesized_declarator, $.abstract_parenthesized_declarator, $.pointer_declarator, $.abstract_pointer_declarator, $.type_specifier],
    [$.parenthesized_declarator, $.abstract_parenthesized_declarator, $.type_specifier],
    [$._declaration_modifiers, $.abstract_parenthesized_declarator, $.abstract_pointer_declarator, $.type_specifier],
    [$.abstract_parenthesized_declarator, $.abstract_pointer_declarator, $.type_specifier],
    [$.abstract_parenthesized_declarator, $.type_specifier],
    [$.parenthesized_declarator, $.abstract_parenthesized_declarator, $.pointer_declarator, $.abstract_pointer_declarator],
    [$.parenthesized_type_declarator, $.pointer_type_declarator],
    [$.abstract_parenthesized_declarator, $.abstract_pointer_declarator],
    [$.function_definition, $.declaration, $.pointer_declarator],
    [$.pointer_declarator, $.abstract_pointer_declarator],
    [$.macro_qualifier, $._field_declarator],
    [$.pointer_type_declarator],
    [$.abstract_pointer_declarator],
    [$.pointer_declarator],
    [$.macro_qualifier, $._type_declarator],
    [$.macro_qualifier, $._declarator],
    [$.macro_qualifier, $._declarator, $.type_specifier, $.macro_type_specifier],
    [$.macro_argument_list, $.sized_type_specifier],
    [$.macro_argument_list, $.type_specifier],
    [$.macro_argument_list, $.type_qualifier],
    [$.macro_qualifier, $.type_specifier, $.expression, $.macro_type_specifier],
    [$.macro_qualifier, $.macro_declaration, $.type_specifier, $.macro_type_specifier],
    [$.macro_qualifier, $.type_specifier, $.macro_type_specifier],
    [$.macro_qualifier, $.union_specifier],
    [$.macro_qualifier, $.struct_specifier],
    [$.macro_qualifier, $.enum_specifier],
    // Generic macro fallbacks (macro_declaration / macro_qualifier / macro_argument_list)
    // start with a bare identifier like every other construct; each entry is a
    // verbatim unresolved-conflict set reported by tree-sitter generate.
    [$.macro_declaration, $.macro_qualifier, $.type_specifier],
    [$.macro_declaration, $.type_specifier],
    [$.macro_qualifier, $.macro_declaration],
    [$.macro_qualifier, $.type_specifier],
    [$.macro_declaration, $.macro_qualifier],
  ],

  extras: $ => [
    /\s|\\\r?\n/,
    $.comment,
  ],

  inline: $ => [
    $._type_identifier,
    $._field_identifier,
    $._statement_identifier,
    $._non_case_statement,
    $._assignment_left_expression,
    $._expression_not_binary,
  ],

  supertypes: $ => [
    $.expression,
    $.statement,
    $.type_specifier,
    $._declarator,
    $._field_declarator,
    $._type_declarator,
    $._abstract_declarator,
  ],

  word: $ => $.identifier,

  rules: {
    translation_unit: $ => repeat($._top_level_item),

    // Top level items are block items with the exception of the expression statement
    _top_level_item: $ => choice(
      $.function_definition,
      alias($._old_style_function_definition, $.function_definition),
      $.linkage_specification,
      $.guarded_linkage,
      $.declaration,
      $._top_level_statement,
      $.attributed_statement,
      $.type_definition,
      $._empty_declaration,
      $.preproc_if,
      $.preproc_ifdef,
      $.preproc_include,
      $.preproc_def,
      $.preproc_function_def,
      $.preproc_call,
      $.preproc_null,
      // `NAME(args)` / `NAME;` / `NAME(args);` only — see _macro_declaration_top.
      // A bare `NAME` alternative lets `NAME(args)` fragment into
      // macro_declaration(NAME) + parenthesized_expression(args) + MISSING ';'.
      alias(prec.dynamic(-2, prec.right($._macro_declaration_top)), $.macro_declaration),
    ),

    _block_item: $ => choice(
      $.function_definition,
      alias($._old_style_function_definition, $.function_definition),
      $.linkage_specification,
      $.guarded_linkage,
      $.declaration,
      $.statement,
      $.attributed_statement,
      $.type_definition,
      $._empty_declaration,
      alias($.preproc_if_in_statement, $.preproc_if),
      alias($.preproc_ifdef_in_statement, $.preproc_ifdef),
      $.preproc_include,
      $.preproc_def,
      $.preproc_function_def,
      $.preproc_call,
      $.preproc_null,
    ),

    // Preprocesser

    preproc_include: $ => seq(
      preprocessor('include'),
      field('path', choice(
        $.string_literal,
        $.system_lib_string,
        $.identifier,
        alias($.preproc_call_expression, $.call_expression),
      )),
      token.immediate(/\r?\n/),
    ),

    preproc_def: $ => seq(
      preprocessor('define'),
      field('name', $.identifier),
      field('value', repeat(choice($.preproc_arg, $.comment))),
      token.immediate(/\r?\n/),
    ),

    preproc_function_def: $ => seq(
      preprocessor('define'),
      field('name', $.identifier),
      field('parameters', $.preproc_params),
      // A block comment inside the \-continued body splits preproc_arg
      // into pieces; let each piece be a value so the directive resumes
      // after the comment instead of ending it.
      field('value', repeat(choice($.preproc_arg, $.comment))),
      token.immediate(/\r?\n/),
    ),

    preproc_params: $ => seq(
      token.immediate('('),
      commaSep(choice($.identifier, seq($.identifier, '...'), '...')),
      ')',
    ),

    preproc_call: $ => seq(
      field('directive', $.preproc_directive),
      field('argument', repeat(choice($.preproc_arg, $.comment))),
      token.immediate(/\r?\n/),
    ),

    // Top-level preproc bodies hold top-level items (a guarded header can
    // contain declaration macros); nested bodies still accept every
    // statement shape through _top_level_statement -> statement.
    ...preprocIf('', $ => choice($._top_level_item, $.dangling_else, $.dangling_label, '__extension__')),
    // Statement-context variant: a preproc group inside a function body
    // holds statements/declarations, NOT top-level items — in particular
    // `NAME(args);` there is a call_expression, never a macro_declaration
    // (the file-scope fallback arm in _top_level_item). Used via
    // _block_item and _statement_or_preproc.
    ...preprocIf('_in_statement', $ => choice($._block_item, $.dangling_else, $.dangling_label, '__extension__')),
    ...preprocIf('_in_field_declaration_list', $ => $._field_declaration_list_item),
    ...preprocIf('_in_enumerator_list', $ => choice(seq($.enumerator, ','), $.enumerator, $.call_expression, '__extension__', $.preproc_def)),
    ...preprocIf('_in_enumerator_list_no_comma', $ => choice($.enumerator, $.call_expression, '__extension__', $.preproc_def), -1),
    // Inside a conditional-compilation block the comma is a loose atom:
    // `f(a #ifdef X , b #endif c)` splits the separator into the block, so
    // a `,` can appear at any position without an item following it.
    ...preprocIf('_in_parameter_list', $ => choice(
      $.parameter_declaration,
      $.variadic_parameter,
      ',',
    )),
    ...preprocIf('_in_initializer_list', $ => choice(
      $.initializer_pair,
      $.expression,
      $.initializer_list,
      ',',
      // `#define`/`#undef`/unknown directives inside a braced list:
      // generated lookup tables redefine an X-macro between items.
      $.preproc_def,
      $.preproc_call,
      $.preproc_null,
      seq(
        choice(
        '||', '&&', '|', '^', '&', '==', '!=', '<', '<=', '>', '>=',
        '<<', '>>', '+', '-', '*', '/', '%', '.', '->',
        ),
        $.expression,
      ),
      seq(
        $.expression,
        choice(
'||', '&&', '|', '^', '&', '==', '!=', '<', '<=', '>', '>=',
        '<<', '>>', '+', '-', '*', '/', '%', '.', '->',
        ),
      ),
    )),
    // A block may hold `,arg` (split separator) or `op expr` (split
    // infix continuation, `a | b #ifdef X | c #endif`) — both sit in the
    // same `preproc_if_in_argument_list` rules, which the infix tail of
    // a binary_expression reuses.
    ...preprocIf('_in_argument_list', $ => choice(
      $.expression,
      $.compound_statement,
      ',',
      $.preproc_def,
      $.preproc_call,
      $.preproc_null,
      // A bare operator passed as a macro argument:
      // `assert_int(1, ==, call(...))`, `check(x, <=, y)`.
      choice(
        '||', '&&', '|', '^', '&', '==', '!=', '<', '<=', '>', '>=',
        '<<', '>>', '+', '-', '*', '/', '%', '.', '->',
      ),
      seq(
        choice(
        '||', '&&', '|', '^', '&', '==', '!=', '<', '<=', '>', '>=',
        '<<', '>>', '+', '-', '*', '/', '%', '.', '->',
        ),
        $.expression,
      ),
      seq(
        $.expression,
        choice(
'||', '&&', '|', '^', '&', '==', '!=', '<', '<=', '>', '>=',
        '<<', '>>', '+', '-', '*', '/', '%', '.', '->',
        ),
      ),
    )),


    preproc_arg: _ => token(prec(-1, /\S([^/\n]|\/[^*]|\\\r?\n)*/)),
    preproc_directive: _ => /#[ \t]*[a-zA-Z0-9]\w*/,

    // A `#` line that carries no directive name: the null directive (`#`
    // alone or `#` + trailing space) and the pre-ANSI `# comment` banner
    // idiom (`# /*`, `# *`, `# */`, `#---`). One token eats the whole line —
    // a `preproc_arg` run here would let a trailing `*/` glue the *next*
    // directive line into the argument stream (`\/[^*]` matches `/\n`).
    // The text after `#` must start with a non-word, non-`#`, non-space
    // char, so `#foo`/`# foo`/`#123` keep their existing reads.
    // A `#` line that carries no directive name: the null directive (`#`
    // alone or `#` + trailing blanks) and the pre-ANSI `# comment` banner
    // idiom (`# /*`, `# *`, `# */`, `#---`). The banner arm is a single
    // token eating the whole line - a `preproc_arg` run here would let a
    // trailing `*/` glue the *next* directive line into the argument stream
    // (`\/[^*]` matches `/\n`). The text after `#` must start with a
    // non-word, non-`#`, non-space char, so `#foo`/`# foo`/`#123` keep their
    // existing reads; a bare `#` goes through the `'#'` + newline arm so it
    // never competes with `#`-starting tokens at lex time.
    preproc_null: $ => choice(
      token(/#[ \t]*[^a-zA-Z0-9#\s][^\n]*/),
      seq('#', token.immediate(/\r?\n/)),
      seq(token(/#[ \t]+/), token.immediate(/\r?\n/)),
    ),

    _preproc_expression: $ => choice(
      $.identifier,
      alias($.preproc_call_expression, $.call_expression),
      $.number_literal,
      $.char_literal,
      $.preproc_defined,
      alias($.preproc_unary_expression, $.unary_expression),
      alias($.preproc_binary_expression, $.binary_expression),
      alias($.preproc_parenthesized_expression, $.parenthesized_expression),
    ),

    preproc_parenthesized_expression: $ => seq(
      '(',
      $._preproc_expression,
      ')',
    ),

    preproc_defined: $ => choice(
      prec(PREC.CALL, seq('defined', '(', $.identifier, ')')),
      seq('defined', $.identifier),
    ),

    preproc_unary_expression: $ => prec.left(PREC.UNARY, seq(
      field('operator', choice('!', '~', '-', '+')),
      field('argument', $._preproc_expression),
    )),

    preproc_call_expression: $ => prec(PREC.CALL, seq(
      field('function', $.identifier),
      field('arguments', alias($.preproc_argument_list, $.argument_list)),
    )),

    preproc_argument_list: $ => seq(
      '(',
      commaSep(choice(
        $._preproc_expression,
        $.string_literal,
        $.system_lib_string,
        // A leading comparison/operator token with an optional operand,
        // for assertion-style and workaround macros that pass the
        // comparison itself as an argument:
        // `BOOST_WORKAROUND(BOOST_MSVC, < 1300)`, `CHECK(x, ==, y)`.
        // Commas delimit items, so `a > b` still reads as one argument.
        seq(
          choice('==', '!=', '<=', '>=', '<', '>'),
          optional($._preproc_expression),
        ),
      )),
      ')',
    ),

    preproc_binary_expression: $ => {
      const table = [
        ['+', PREC.ADD],
        ['-', PREC.ADD],
        ['*', PREC.MULTIPLY],
        ['/', PREC.MULTIPLY],
        ['%', PREC.MULTIPLY],
        ['||', PREC.LOGICAL_OR],
        ['&&', PREC.LOGICAL_AND],
        ['|', PREC.INCLUSIVE_OR],
        ['^', PREC.EXCLUSIVE_OR],
        ['&', PREC.BITWISE_AND],
        ['==', PREC.EQUAL],
        ['!=', PREC.EQUAL],
        ['>', PREC.RELATIONAL],
        ['>=', PREC.RELATIONAL],
        ['<=', PREC.RELATIONAL],
        ['<', PREC.RELATIONAL],
        ['<<', PREC.SHIFT],
        ['>>', PREC.SHIFT],
      ];

      return choice(...table.map(([operator, precedence]) => {
        return prec.left(precedence, seq(
          field('left', $._preproc_expression),
          // @ts-ignore
          field('operator', operator),
          field('right', $._preproc_expression),
        ));
      }));
    },

    // Main Grammar

    function_definition: $ => seq(
      optional($.ms_call_modifier),
      $._declaration_specifiers,
      // WINAPI/NTAPI/APIENTRY/CDECL/FAR and friends: macro_qualifier covers
      // calling-convention spellings outside ms_call_modifier's literal list.
      repeat(choice($.ms_call_modifier, $.macro_attribute)),
      field('declarator', $._declarator),
      // Trailing attribute/override macros between declarator and body:
      // `f() FLATBUFFERS_OVERRIDE {`, `f() MYLIB_NOEXCEPT {`.
      repeat(choice($.ms_call_modifier, $.macro_attribute, $.macro_qualifier)),
      field('body', $.compound_statement),
    ),

    _old_style_function_definition: $ => seq(
      optional($.ms_call_modifier),
      $._declaration_specifiers,
      field('declarator', alias($._old_style_function_declarator, $.function_declarator)),
      repeat1($.declaration),
      field('body', $.compound_statement),
    ),

    declaration: $ => seq(
      $._declaration_specifiers,
      commaSep1(field('declarator', choice(
        seq(
          repeat(choice($.ms_call_modifier, $.macro_attribute)),
          $._declaration_declarator,
          optional($.gnu_asm_expression),
          repeat(choice($.ms_call_modifier, $.macro_attribute, $.macro_qualifier)),
        ),
        $.init_declarator,
      ))),
      ';',
    ),

    type_definition: $ => choice(
      seq(
        optional('__extension__'),
        'typedef',
        $._type_definition_type,
        $._type_definition_declarators,
        repeat($.attribute_specifier),
        ';',
      ),
      // Declaratorless typedef (`typedef enum X {...};`) — compilers
      // accept it as an extension; Windows SDK headers use it inside
      // #if blocks. Optional() would insert a MISSING declarator, which
      // still counts as a parse error, so the empty branch is a full
      // alternate instead.
      seq(
        optional('__extension__'),
        'typedef',
        $._type_definition_type,
        repeat($.attribute_specifier),
        ';',
      ),
    ),
    // Pre-type qualifiers also include a bare qualifier macro (`typedef
    // CONST char *PCSZ`) — without an argument list, because `VOID (*P)`
    // would otherwise swallow `(*P)` as its macro arguments and kill
    // `typedef VOID (*F)(params)` function-pointer typedefs. Post-type
    // stays keyword-only so a bare declarator name is never a macro.
    _type_definition_type: $ => seq(repeat(choice($.type_qualifier, alias($._typedef_macro_qualifier, $.macro_qualifier))), field('type', $.type_specifier), repeat($.type_qualifier)),
    _typedef_macro_qualifier: $ => $.identifier,
    _type_definition_declarators: $ => commaSep1(field('declarator', $._type_declarator)),

    _declaration_modifiers: $ => choice(
      $.storage_class_specifier,
      $.type_qualifier,
      $.attribute_specifier,
      $.attribute_declaration,
      $.ms_declspec_modifier,
      $.macro_attribute,
    ),

    // Vendor headers pre-decorate declarations with attribute-like macros:
    // `__IO uint32_t ISER[1];`, `VKAPI_ATTR VkBool32 VKAPI_CALL f(void);`,
    // `int WINAPI wWinMain(...)`. The `word` token intercepts every
    // identifier-shaped input, so only literals can be recognized here;
    // this enumerates the widespread calling-convention, storage and
    // compiler-attribute macros, which are never real types or declarators.
    macro_attribute: $ => choice(
      // Windows / JNI calling conventions
      'WINAPI', 'WINAPIV', 'CALLBACK', 'APIENTRY', 'NTAPI', 'PASCAL',
      'STDCALL', 'CDECL', 'FASTCALL', 'JNICALL', 'WINBASEAPI', 'WINADVAPI',
      'STDAPICALLTYPE', 'STDAPI', 'RPC_ENTRY', 'APIPRIVATE', 'APIPUBLIC',
      '__RPC_API', '__RPC_STUB', '__RPC_USER',
      // CMSIS / ARM
      '__IO', '__I', '__O', '__IM', '__OM', '__IOM', '__ASM', '__INLINE',
      '__STATIC_INLINE', '__STATIC_FORCEINLINE', '__WEAK', '__IRQ',
      '__PACKED', '__NO_RETURN', '__ALIGNED', '__COMPILER_BARRIER',
      '__PROGRAM_START', '__VECTOR_TABLE',
      // Linux sparse / GCC attributes
      '__user', '__kernel', '__iomem', '__maybe_unused', '__always_inline',
      '__must_check', '__init', '__exit', '__weak', '__section', '__rcu',
      '__percpu', '__bitwise', '__force', '__nocast', '__acquires',
      '__releases', '__initdata', '__ref', '__refdata', '__read_mostly',
      '__ro_after_init', '__latent_entropy', '__cold', '__noreturn',
      '__unused', '__used', '__alias', '__deprecated', '__nonstring',
      '__counted_by', '__printf', '__scanf', '__visible', '__initconst',
      '__exitconst', '__init_or_module', '__randomize_layout',
      '__no_randomize_layout',
      // glibc
      '__THROW', '__THROWNL', '__LEAF', '__LEAF_ATTR', '__wur',
      '__attribute_pure__', '__attribute_const__', '__attribute_malloc__',
      '__attribute_alloc_size__', '__attribute_deprecated__',
      '__attribute_warn_unused_result__', '__nonnull',
      '__attribute_format_arg__', '__attribute_format_strfmon__',
      '__attribute_artificial__', '__glibc_unlikely', '__glibc_likely',
      // vendor API/export macros seen in the wild
      'VKAPI_ATTR', 'VKAPI_CALL', 'SDLCALL', 'NAPI_CDECL', 'NAPI_EXTERN',
      'STRINGZILLA_API', 'STRINGZILLA_INLINE', 'STRINGZILLA_INLINE_',
      'STRINGZILLA_CONSTEXPR', 'STRINGZILLA_OUTLINED_',
      'STRINGZILLA_DEVICE_NOINLINE',
      'DOCKS_EXPORT', 'DOCKS_EXPORT_FOR_UNIT_TESTS', 'SQLITECPP_API',
      'PCAP_API', 'WUFFS_MIMIC',
      // Corpus-mined export/attribute spellings (frequent in repos_big):
      // sqlite, Nuklear, nghttp2, GLFW, zlib, glad, doctest, stb,
      // Foundation, flatbuffers, utf8proc, 7zip, Flutter, seccomp, node-api.
      'SQLITE_API', 'NK_API', 'NK_LIB', 'NK_INTERN', 'NK_GLOBAL',
      'NGHTTP2_EXTERN', 'GLFWAPI', 'ZEXTERN', 'IJK_API', 'GLAD_API_CALL',
      'DOCTEST_INTERFACE', 'DOCTEST_INTERFACE_DECL', 'DOCTEST_INTERFACE_DEF',
      'DOCTEST_NOINLINE', 'DOCTEST_THREAD_LOCAL', 'DOCTEST_NORETURN',
      'EXTERN_C', 'SCMP_WEAK', 'STBIWDEF', 'FOUNDATION_EXPORT',
      'NO_INLINE', 'NOINLINE', 'H_INLINE', 'Z7_NO_INLINE',
      'Z7_FORCE_INLINE', 'FLATBUFFERS_CONSTEXPR',
      'FLATBUFFERS_CONSTEXPR_CPP11', 'IMPORT_FN', 'UTF8PROC_DLLEXPORT',
      'FLUTTER_PLUGIN_EXPORT',
    ),

    // Generic identifier-as-modifier for API/export/qualifier macros the
    // literal lists cannot enumerate: `ASIO_DECL`, `FMT_API`, `FOO_EXPORT`,
    // `MYLIB_CALL(args)`. Only usable in the pre-type position of
    // _declaration_specifiers and before a struct/union/enum/class name:
    // post-type an identifier is a declarator. Dynamic -1: any real type or
    // declarator read wins, so `uint`/`ULong`-shaped typedefs still resolve
    // as types.
    macro_qualifier: $ => prec.dynamic(-1, prec.right(seq(
      $.identifier,
      optional($.macro_argument_list),
    ))),

    // Generic macro-expands-to-declarations fallback for names the literal
    // lists cannot enumerate (test/assert/serialize macros, Qt/UE-adjacent
    // markers). Dynamic -1: real statements and declarations win; this only
    // catches bare `NAME` / `NAME(...)` items no other rule accepts, chiefly
    // inside struct bodies where expression statements cannot appear.
    macro_declaration: $ => prec.dynamic(-2, prec.right(seq(
      $.identifier,
      optional($.macro_argument_list),
      optional(';'),
    ))),

    // Generic `NAME(args)` fallback usable only where a bare `NAME` would be
    // wrong: top level. `identifier + argument_list` collides token-for-token
    // with call_expression, so the args here go through the permissive macro
    // list, which never shares states with call arguments.
    _macro_declaration_top: $ => seq(
      $.identifier,
      choice(
        seq($.macro_argument_list, optional(';')),
        ';',
      ),
    ),

    // `NAME(args)` and nothing else: no bare `NAME` (lets `NAME(args)`
    // fragment into NAME + parenthesized args) and no `;` (a `;`-tolerant
    // variant re-collides with call_expression + `;`, which must win in
    // statement position). Used via _non_case_statement.
    _macro_declaration_stmt: $ => seq(
      $.identifier,
      $.macro_argument_list,
    ),

    // Macro arguments that are not expressions (types, member names, DSL
    // keywords, operators) accepted as a permissive token list.
    macro_argument_list: $ => seq(
      '(',
      repeat(choice(
        $.identifier,
        $.primitive_type,
        $.sized_type_specifier,
        $.number_literal,
        $.pointer_expression,
        $.unary_expression,
        $.sizeof_expression,
        $.string_literal,
        $.char_literal,
        ',',
        '<',
        '>',
        '*',
        '&',
        '&&',
        '|',
        '=',
        '!',
        '?',
        ':',
        '.',
        '->',
        '~',
        '+',
        '-',
        '/',
        '%',
        '==',
        '!=',
        '<<',
        '>>',
        '<=',
        '>=',
        '||',
        '^',
        '<<=',
        '>>=',
        '%=',
        '&=',
        '|=',
        '^=',
        '++',
        '--',
        '[',
        ']',
        'const',
        // Brace-init inside macro args: `M(name, T{})` / `M({a, b})`.
        '{',
        '}',
        $.macro_argument_list,
      )),
      ')',
    ),

    _declaration_specifiers: $ => prec.right(seq(
      // macro_qualifier is allowed only before the type: a bare identifier
      // after the type is always a declarator name, never a macro.
      repeat(choice($._declaration_modifiers, $.macro_qualifier)),
      field('type', $.type_specifier),
      repeat($._declaration_modifiers),
    )),

    // Parameter specifiers differ from _declaration_specifiers in one way:
    // the pre-type qualifier is a bare identifier — no argument list. In a
    // parameter list `T (&arr)[N]` is a parenthesized declarator
    // (reference-to-array), never a macro call; taking args for the
    // qualifier would swallow the declarator. `MACRO(args) T p` is the
    // accepted loss.
    _parameter_specifiers: $ => prec.right(seq(
      repeat(choice(
        $._declaration_modifiers,
        alias($._macro_qualifier_bare, $.macro_qualifier),
      )),
      field('type', $.type_specifier),
      repeat($._declaration_modifiers),
    )),

    _macro_qualifier_bare: $ => prec.dynamic(-1, seq($.identifier)),

    linkage_specification: $ => seq(
      'extern',
      field('value', $.string_literal),
      field('body', choice(
        $.function_definition,
        $.declaration,
        $.declaration_list,
      )),
    ),

    // `extern "C"` inside preproc guards: the opening brace lives inside one
    // conditional block and the closing brace inside a later one, so neither
    // block parses on its own. Shared C/C++ headers are written this way.
    guarded_linkage: $ => prec.dynamic(10, seq(
      $._linkage_guard,
      'extern',
      field('value', $.string_literal),
      '{',
      alias(preprocessor('endif'), '#endif'),
      repeat($._top_level_item),
      $._linkage_guard,
      '}',
      alias(preprocessor('endif'), '#endif'),
    )),

    _linkage_guard: $ => choice(
      seq(preprocessor('ifdef'), field('name', $.identifier)),
      seq(preprocessor('ifndef'), field('name', $.identifier)),
      seq(preprocessor('if'), field('condition', $._preproc_expression), '\n'),
    ),

    attribute_specifier: $ => seq(
      choice('__attribute__', '__attribute'),
      '(',
      $.argument_list,
      ')',
    ),

    attribute: $ => seq(
      optional(seq(field('prefix', $.identifier), '::')),
      field('name', $.identifier),
      optional($.argument_list),
    ),

    attribute_declaration: $ => seq(
      '[[',
      commaSep1($.attribute),
      ']]',
    ),

    ms_declspec_modifier: $ => seq(
      '__declspec',
      '(',
      $.identifier,
      ')',
    ),

    ms_based_modifier: $ => seq(
      '__based',
      $.argument_list,
    ),

    ms_call_modifier: _ => choice(
      '__cdecl',
      '__clrcall',
      '__stdcall',
      '__fastcall',
      '__thiscall',
      '__vectorcall',
    ),

    // Pointer-model macros (`fd_set FAR*`, `char NEAR*`). Calling-convention
    // spellings like WINAPI ride macro_attribute/macro_qualifier, which
    // already cover them.
    far_ptr_modifier: _ => choice(
      'FAR', 'NEAR', '_far', '_near', '__far', '__near', '__huge', '_huge',
    ),

    ms_restrict_modifier: _ => '__restrict',

    ms_unsigned_ptr_modifier: _ => '__uptr',

    ms_signed_ptr_modifier: _ => '__sptr',

    ms_unaligned_ptr_modifier: _ => choice('_unaligned', '__unaligned'),

    ms_pointer_modifier: $ => choice(
      $.ms_unaligned_ptr_modifier,
      $.ms_restrict_modifier,
      $.ms_unsigned_ptr_modifier,
      $.ms_signed_ptr_modifier,
    ),

    declaration_list: $ => seq(
      '{',
      repeat($._block_item),
      '}',
    ),

    _declarator: $ => choice(
      $.attributed_declarator,
      $.pointer_declarator,
      $.function_declarator,
      $.array_declarator,
      $.parenthesized_declarator,
      $.identifier,
    ),

    _declaration_declarator: $ => choice(
      $.attributed_declarator,
      $.pointer_declarator,
      alias($._function_declaration_declarator, $.function_declarator),
      $.array_declarator,
      $.parenthesized_declarator,
      $.identifier,
    ),

    _field_declarator: $ => choice(
      alias($.attributed_field_declarator, $.attributed_declarator),
      alias($.pointer_field_declarator, $.pointer_declarator),
      alias($.function_field_declarator, $.function_declarator),
      alias($.array_field_declarator, $.array_declarator),
      alias($.parenthesized_field_declarator, $.parenthesized_declarator),
      $._field_identifier,
    ),

    _type_declarator: $ => choice(
      alias($.attributed_type_declarator, $.attributed_declarator),
      alias($.pointer_type_declarator, $.pointer_declarator),
      alias($.function_type_declarator, $.function_declarator),
      alias($.array_type_declarator, $.array_declarator),
      alias($.parenthesized_type_declarator, $.parenthesized_declarator),
      $._type_identifier,
      alias(choice('signed', '__signed__', '__signed', 'unsigned', 'long', 'short'), $.primitive_type),
      $.primitive_type,
    ),

    _abstract_declarator: $ => choice(
      $.abstract_pointer_declarator,
      $.abstract_function_declarator,
      $.abstract_array_declarator,
      $.abstract_parenthesized_declarator,
    ),

    parenthesized_declarator: $ => prec.dynamic(PREC.PAREN_DECLARATOR, seq(
      '(',
      repeat(choice($.ms_call_modifier, $.macro_attribute)),
      $._declarator,
      ')',
    )),
    parenthesized_field_declarator: $ => prec.dynamic(PREC.PAREN_DECLARATOR, seq(
      '(',
      repeat(choice($.ms_call_modifier, $.macro_attribute)),
      $._field_declarator,
      ')',
    )),
    parenthesized_type_declarator: $ => prec.dynamic(PREC.PAREN_DECLARATOR, seq(
      '(',
      repeat(choice($.ms_call_modifier, $.macro_attribute)),
      $._type_declarator,
      ')',
    )),
    abstract_parenthesized_declarator: $ => prec(1, seq(
      '(',
      repeat(choice($.ms_call_modifier, $.macro_attribute)),
      $._abstract_declarator,
      ')',
    )),


    attributed_declarator: $ => prec.right(seq(
      $._declarator,
      repeat1($.attribute_declaration),
    )),
    attributed_field_declarator: $ => prec.right(seq(
      $._field_declarator,
      repeat1($.attribute_declaration),
    )),
    attributed_type_declarator: $ => prec.right(seq(
      $._type_declarator,
      repeat1($.attribute_declaration),
    )),

    pointer_declarator: $ => prec.dynamic(1, prec.right(seq(
      optional($.ms_based_modifier),
      repeat(choice($.ms_call_modifier, $.far_ptr_modifier, $.macro_attribute, $.macro_qualifier)),
      '*',
      repeat(choice(
        $.ms_pointer_modifier,
        $.type_qualifier,
        $.ms_call_modifier,
        $.far_ptr_modifier,
        $.macro_attribute,
        $.macro_qualifier,
      )),
      field('declarator', $._declarator),
    ))),
    pointer_field_declarator: $ => prec.dynamic(1, prec.right(seq(
      optional($.ms_based_modifier),
      repeat(choice($.ms_call_modifier, $.far_ptr_modifier, $.macro_attribute, $.macro_qualifier)),
      '*',
      repeat(choice(
        $.ms_pointer_modifier,
        $.type_qualifier,
        $.ms_call_modifier,
        $.far_ptr_modifier,
        $.macro_attribute,
        $.macro_qualifier,
      )),
      field('declarator', $._field_declarator),
    ))),
    pointer_type_declarator: $ => prec.dynamic(1, prec.right(seq(
      optional($.ms_based_modifier),
      repeat(choice($.ms_call_modifier, $.far_ptr_modifier, $.macro_attribute, $.macro_qualifier)),
      '*',
      repeat(choice(
        $.ms_pointer_modifier,
        $.type_qualifier,
        $.ms_call_modifier,
        $.far_ptr_modifier,
        $.macro_attribute,
        $.macro_qualifier,
      )),
      field('declarator', $._type_declarator),
    ))),
    abstract_pointer_declarator: $ => prec.dynamic(1, prec.right(seq(
      repeat(choice($.ms_call_modifier, $.far_ptr_modifier, $.macro_attribute, $.macro_qualifier)),
      '*',
      repeat(choice(
        $.ms_pointer_modifier,
        $.type_qualifier,
        $.ms_call_modifier,
        $.far_ptr_modifier,
        $.macro_attribute,
      )),
      field('declarator', optional($._abstract_declarator)),
    ))),

    function_declarator: $ => prec.right(1,
      seq(
        field('declarator', $._declarator),
        field('parameters', $.parameter_list),
        optional($.gnu_asm_expression),
        repeat(choice(
          $.attribute_specifier,
          $.identifier,
          alias($.preproc_call_expression, $.call_expression),
        )),
      ),
    ),

    _function_declaration_declarator: $ => prec.right(1,
      seq(
        field('declarator', $._declarator),
        field('parameters', $.parameter_list),
        optional($.gnu_asm_expression),
        repeat($.attribute_specifier),
      )),

    function_field_declarator: $ => prec(1, seq(
      field('declarator', $._field_declarator),
      field('parameters', $.parameter_list),
    )),
    function_type_declarator: $ => prec(1, seq(
      field('declarator', $._type_declarator),
      field('parameters', $.parameter_list),
    )),
    abstract_function_declarator: $ => prec(1, seq(
      field('declarator', optional($._abstract_declarator)),
      field('parameters', $.parameter_list),
    )),

    _old_style_function_declarator: $ => seq(
      field('declarator', $._declarator),
      field('parameters', alias($._old_style_parameter_list, $.parameter_list)),
    ),

    array_declarator: $ => prec(1, seq(
      field('declarator', $._declarator),
      '[',
      repeat(choice($.type_qualifier, 'static')),
      field('size', optional(choice($.expression, '*'))),
      ']',
    )),
    array_field_declarator: $ => prec(1, seq(
      field('declarator', $._field_declarator),
      '[',
      repeat(choice($.type_qualifier, 'static')),
      field('size', optional(choice($.expression, '*'))),
      ']',
    )),
    array_type_declarator: $ => prec(1, seq(
      field('declarator', $._type_declarator),
      '[',
      repeat(choice($.type_qualifier, 'static')),
      field('size', optional(choice($.expression, '*'))),
      ']',
    )),
    abstract_array_declarator: $ => prec(1, seq(
      field('declarator', optional($._abstract_declarator)),
      '[',
      repeat(choice($.type_qualifier, 'static')),
      field('size', optional(choice($.expression, '*'))),
      ']',
    )),

    init_declarator: $ => seq(
      field('declarator', $._declarator),
      repeat(choice($.ms_call_modifier, $.macro_attribute, $.macro_qualifier)),
      optional($.gnu_asm_expression),
      '=',
      field('value', choice($.initializer_list, $.expression)),
    ),

    compound_statement: $ => seq(
      '{',
      repeat($._block_item),
      '}',
    ),

    storage_class_specifier: _ => choice(
      'extern',
      'static',
      'auto',
      'register',
      'inline',
      '__inline',
      '__inline__',
      '__forceinline',
      'thread_local',
      '_Thread_local',
      '__thread',
    ),

    type_qualifier: $ => choice(
      'const',
      'constexpr',
      'volatile',
      'restrict',
      '__restrict__',
      '__extension__',
      '_Atomic',
      '_Complex',
      'complex',
      '_Noreturn',
      'noreturn',
      '_Nonnull',
      $.alignas_qualifier,
    ),

    alignas_qualifier: $ => seq(
      choice('alignas', '_Alignas'),
      '(',
      choice($.expression, $.type_descriptor),
      ')',
    ),

    type_specifier: $ => choice(
      $.struct_specifier,
      $.union_specifier,
      $.enum_specifier,
      $.macro_type_specifier,
      $.sized_type_specifier,
      $.primitive_type,
      $.macro_attribute,
      $._type_identifier,
    ),

    sized_type_specifier: $ => choice(
      seq(
        repeat(choice(
          'signed',
          '__signed__',
          '__signed',
          'unsigned',
          'long',
          'short',
        )),
        field('type', optional(choice(
          prec.dynamic(-1, $._type_identifier),
          $.primitive_type,
        ))),
        repeat1(choice(
          'signed',
          '__signed__',
          '__signed',
          'unsigned',
          'long',
          'short',
        )),
      ),
      seq(
        repeat1(choice(
          'signed',
          '__signed__',
          '__signed',
          'unsigned',
          'long',
          'short',
        )),
        repeat($.type_qualifier),
        field('type', optional(choice(
          prec.dynamic(-1, $._type_identifier),
          $.primitive_type,
        ))),
        repeat(choice(
          'signed',
          '__signed__',
          '__signed',
          'unsigned',
          'long',
          'short',
        )),
      ),
    ),

    primitive_type: _ => token(choice(
      'bool',
      'char',
      'int',
      'float',
      'double',
      'void',
      'size_t',
      'ssize_t',
      'ptrdiff_t',
      'intptr_t',
      'uintptr_t',
      'charptr_t',
      'nullptr_t',
      'max_align_t',
      // Reserved/builtin spellings (underscore- or _Capital-prefixed) that
      // cannot be user typedefs: MSVC ints, GCC/Clang vectors, ISO _FloatN.
      '_Bool',
      '__fp16',
      '__bf16',
      '__vector',
      '__pixel',
      '__int128_t',
      ...[8, 16, 32, 64, 128].map(n => `__int${n}`),
      ...[16, 32, 64, 128].map(n => `_Float${n}`),
      ...[32, 64, 128].map(n => `_Float${n}x`),
      ...[32, 64, 128].map(n => `_Decimal${n}`),
      ...[8, 16, 32, 64].map(n => `int${n}_t`),
      ...[8, 16, 32, 64].map(n => `uint${n}_t`),
      ...[8, 16, 32, 64].map(n => `char${n}_t`),
    )),

    enum_specifier: $ => seq(
      'enum',
      repeat($.macro_qualifier),
      choice(
        seq(
          field('name', $._type_identifier),
          optional($._enum_underlying_type),
          field('body', optional($.enumerator_list)),
        ),
        seq(
          optional($._enum_underlying_type),
          field('body', $.enumerator_list),
        ),
      ),
      optional($.attribute_specifier),
    ),

    _enum_underlying_type: $ => prec.right(seq(
      ':',
      field('underlying_type', choice($.primitive_type, $.sized_type_specifier)),
    )),

    enumerator_list: $ => seq(
      '{',
      repeat(choice(
        $.enumerator,
        $.call_expression,
        '__extension__',
        alias($.preproc_if_in_enumerator_list, $.preproc_if),
        alias($.preproc_ifdef_in_enumerator_list, $.preproc_ifdef),
        alias($.preproc_if_in_enumerator_list_no_comma, $.preproc_if),
        alias($.preproc_ifdef_in_enumerator_list_no_comma, $.preproc_ifdef),
        $.preproc_def,
        $.preproc_call,
        $.preproc_null,
        ',',
      )),
      '}',
    ),

    struct_specifier: $ => prec.right(seq(
      'struct',
      optional($.attribute_specifier),
      optional($.ms_declspec_modifier),
      repeat($.macro_qualifier),
      choice(
        seq(
          field('name', $._type_identifier),
          // `struct X : public Y` inside `#ifdef __cplusplus` in shared
          // headers — never reachable in C-only code, so no ambiguity.
          optional(seq(':', commaSep1(seq(optional($.identifier), $._type_identifier)))),
          field('body', optional($.field_declaration_list)),
        ),
        field('body', $.field_declaration_list),
      ),
      optional($.attribute_specifier),
    )),

    union_specifier: $ => prec.right(seq(
      'union',
      optional($.ms_declspec_modifier),
      repeat($.macro_qualifier),
      choice(
        seq(
          field('name', $._type_identifier),
          field('body', optional($.field_declaration_list)),
        ),
        field('body', $.field_declaration_list),
      ),
      optional($.attribute_specifier),
    )),

    field_declaration_list: $ => seq(
      '{',
      repeat($._field_declaration_list_item),
      '}',
    ),

    _field_declaration_list_item: $ => choice(
      $.field_declaration,
      $.preproc_def,
      $.preproc_function_def,
      $.preproc_call,
      $.preproc_null,
      $.macro_declaration,
      // MIDL brackets alone inside `#ifdef MIDL_PASS` while the field they
      // annotate sits after the `#endif` (`[size_is(...)]` guard pattern).
      $.midl_attribute_list,
      alias($.preproc_if_in_field_declaration_list, $.preproc_if),
      alias($.preproc_ifdef_in_field_declaration_list, $.preproc_ifdef),
    ),

    // MIDL/RPC annotation brackets seen on Windows SDK members and params:
    // `[size_is(N)] PUCHAR Buf;`, `[case(A, B)] T X;`, `[default] T X;`,
    // `[in, out]`, `[range(0, N)]`. `case`/`default` are keywords so they
    // need explicit alternatives next to the generic `name(args)` entry.
    midl_attribute_list: $ => seq(
      '[',
      commaSep1($._midl_attribute),
      ']',
    ),
    _midl_attribute: $ => choice(
      'default',
      seq(choice('case', $.identifier), optional($.macro_argument_list)),
    ),

    field_declaration: $ => seq(
      repeat($.midl_attribute_list),
      $._declaration_specifiers,
      optional($._field_declaration_declarator),
      optional($.attribute_specifier),
      ';',
    ),
    _field_declaration_declarator: $ => commaSep1(choice(
      seq(
        field('declarator', $._field_declarator),
        optional($.bitfield_clause),
      ),
      $.bitfield_clause,
    )),

    bitfield_clause: $ => seq(':', $.expression),

    enumerator: $ => seq(
      field('name', $.identifier),
      optional(seq('=', field('value', $.expression))),
    ),

    variadic_parameter: _ => '...',

    // `f(int a #ifdef X , int b #endif , int c)` and
    // `f(int a, #ifdef X int b, #endif int c)`: the separator comma may
    // hide inside an adjacent preproc block, so a real item may follow a
    // block comma-free. Between real items commas stay strict.
    _parameter_list_real: $ => choice(
      $.parameter_declaration,
      $.variadic_parameter,
    ),

    _parameter_list_item: $ => choice(
      $._parameter_list_real,
      seq(
        $._parameter_list_real,
        repeat1($._parameter_list_preproc),
        optional(seq(optional(','), $._parameter_list_real)),
      ),
      seq(
        repeat1($._parameter_list_preproc),
        optional(seq(optional(','), $._parameter_list_real)),
      ),
    ),

    _parameter_list_preproc: $ => choice(
      alias($.preproc_if_in_parameter_list, $.preproc_if),
      alias($.preproc_ifdef_in_parameter_list, $.preproc_ifdef),
    ),

    parameter_list: $ => seq(
      '(',
      choice(
        optional(commaSep1($._parameter_list_item)),
        $.compound_statement,
      ),
      ')',
    ),
    _old_style_parameter_list: $ => seq(
      '(',
      commaSep(choice($.identifier, $.variadic_parameter)),
      ')',
    ),

    parameter_declaration: $ => seq(
      repeat($.midl_attribute_list),
      $._parameter_specifiers,
      optional(field('declarator', choice(
        $._declarator,
        $._abstract_declarator,
      ))),
      repeat($.attribute_specifier),
    ),

    // Statements

    attributed_statement: $ => seq(
      repeat1($.attribute_declaration),
      $.statement,
    ),

    statement: $ => choice(
      $.case_statement,
      $._non_case_statement,
    ),

    _non_case_statement: $ => choice(
      $.attributed_statement,
      $.labeled_statement,
      $.compound_statement,
      $.expression_statement,
      $.if_statement,
      $.switch_statement,
      $.do_statement,
      $.while_statement,
      $.for_statement,
      $.return_statement,
      $.break_statement,
      $.continue_statement,
      $.goto_statement,
      $.seh_try_statement,
      $.seh_leave_statement,
      $.ms_asm_statement,
      // `NAME(args)` with no `;` in statement position — unreachable as a
      // call (expression_statement requires `;`), so it can only be a
      // table-generator/statement macro, e.g. `FLATBUFFERS_GEN_TYPES(TD)`
      // inside a switch. With `;` the call read wins at dynamic 0 over
      // the macro + empty-`;`-statement split at -2. See
      // _macro_declaration_stmt.
      alias(prec.dynamic(-2, prec.right($._macro_declaration_stmt)), $.macro_declaration),
    ),

    _top_level_statement: $ => choice(
      $.case_statement,
      $.attributed_statement,
      $.labeled_statement,
      $.compound_statement,
      alias($._top_level_expression_statement, $.expression_statement),
      $.if_statement,
      $.switch_statement,
      $.do_statement,
      $.while_statement,
      $.for_statement,
      $.return_statement,
      $.break_statement,
      $.continue_statement,
      $.goto_statement,
    ),

    labeled_statement: $ => seq(
      field('label', $._statement_identifier),
      ':',
      choice($.declaration, $._statement_or_preproc),
    ),

    // This is missing binary expressions, others were kept so that macro code can be parsed better and code examples
    _top_level_expression_statement: $ => seq(
      optional($._expression_not_binary),
      ';',
    ),

    expression_statement: $ => seq(
      optional(choice(
        $.expression,
        $.comma_expression,
      )),
      ';',
    ),

    // A statement slot that also accepts a whole `#if`/`#ifdef` group:
    // `if (x)\n#ifdef MAC\n a();\n#else\n b();\n#endif` — the conditional
    // group IS the (conditionally compiled) body. The `_in_statement`
    // variant keeps `NAME(args);` inside the group a call_expression.
    _statement_or_preproc: $ => choice(
      $.statement,
      alias($.preproc_if_in_statement, $.preproc_if),
      alias($.preproc_ifdef_in_statement, $.preproc_ifdef),
    ),

    if_statement: $ => prec.right(seq(
      'if',
      field('condition', $.parenthesized_expression),
      field('consequence', $._statement_or_preproc),
      optional(field('alternative', $.else_clause)),
    )),

    else_clause: $ => seq('else', $._statement_or_preproc),
    // `} else #endif { }`: a preproc block may end between `else` and
    // its body, leaving an if without an else-consequence inside the
    // block (the `{ ... }` after `#endif` then parses as a bare
    // compound statement). Chained `else if` tails recurse.
    dangling_else: $ => prec.right(seq(
      'if',
      field('condition', $.parenthesized_expression),
      field('consequence', $._statement_or_preproc),
      'else',
      optional($.dangling_else),
    )),

    // `#ifdef X fin: #endif stmt`: a label may sit alone inside a
    // preproc block while its statement follows the `#endif`.
    dangling_label: $ => seq(field('label', $._statement_identifier), ':'),


    switch_statement: $ => seq(
      'switch',
      field('condition', $.parenthesized_expression),
      field('body', $.compound_statement),
    ),

    case_statement: $ => prec.right(seq(
      choice(
        seq('case', field('value', $.expression)),
        'default',
      ),
      ':',
      repeat(choice(
        $._non_case_statement,
        $.declaration,
        $.type_definition,
      )),
    )),

    while_statement: $ => seq(
      'while',
      field('condition', $.parenthesized_expression),
      field('body', $._statement_or_preproc),
    ),

    do_statement: $ => seq(
      'do',
      field('body', $._statement_or_preproc),
      'while',
      field('condition', $.parenthesized_expression),
      ';',
    ),

    for_statement: $ => seq(
      'for',
      '(',
      $._for_statement_body,
      ')',
      field('body', $._statement_or_preproc),
    ),
    _for_statement_body: $ => seq(
      choice(
        field('initializer', $.declaration),
        seq(field('initializer', optional(choice($.expression, $.comma_expression))), ';'),
      ),
      field('condition', optional(choice($.expression, $.comma_expression))),
      ';',
      field('update', optional(choice($.expression, $.comma_expression))),
    ),

    return_statement: $ => seq(
      'return',
      optional(choice($.expression, $.comma_expression)),
      ';',
    ),

    break_statement: _ => seq(
      'break', ';',
    ),

    continue_statement: _ => seq(
      'continue', ';',
    ),

    goto_statement: $ => seq(
      'goto',
      field('label', $._statement_identifier),
      ';',
    ),

    seh_try_statement: $ => seq(
      '__try',
      field('body', $.compound_statement),
      choice($.seh_except_clause, $.seh_finally_clause),
    ),

    seh_except_clause: $ => seq(
      '__except',
      field('filter', $.parenthesized_expression),
      field('body', $.compound_statement),
    ),

    seh_finally_clause: $ => seq(
      '__finally',
      field('body', $.compound_statement),
    ),

    seh_leave_statement: _ => seq(
      '__leave', ';',
    ),

    // Expressions

    expression: $ => choice(
      $._expression_not_binary,
      $.binary_expression,
    ),

    _expression_not_binary: $ => choice(
      $.conditional_expression,
      $.assignment_expression,
      $.unary_expression,
      $.update_expression,
      $.cast_expression,
      $.pointer_expression,
      $.sizeof_expression,
      $.alignof_expression,
      $.offsetof_expression,
      $.generic_expression,
      $.subscript_expression,
      $.call_expression,
      $.field_expression,
      $.compound_literal_expression,
      $.identifier,
      $.number_literal,
      $._string,
      $.true,
      $.false,
      $.null,
      $.char_literal,
      $.parenthesized_expression,
      $.gnu_asm_expression,
      $.extension_expression,
    ),

    _string: $ => prec.left(choice(
      $.string_literal,
      $.concatenated_string,
    )),

    comma_expression: $ => seq(
      field('left', $.expression),
      ',',
      field('right', choice($.expression, $.comma_expression)),
    ),

    conditional_expression: $ => prec.right(PREC.CONDITIONAL, seq(
      field('condition', $.expression),
      '?',
      optional(field('consequence', choice($.expression, $.comma_expression))),
      ':',
      field('alternative', $.expression),
    )),

    _assignment_left_expression: $ => choice(
      $.identifier,
      $.call_expression,
      $.field_expression,
      $.pointer_expression,
      $.subscript_expression,
      $.parenthesized_expression,
    ),

    assignment_expression: $ => prec.right(PREC.ASSIGNMENT, choice(
      seq(
        field('left', $._assignment_left_expression),
        field('operator', choice(
          '=',
          '*=',
          '/=',
          '%=',
          '+=',
          '-=',
          '<<=',
          '>>=',
          '&=',
          '^=',
          '|=',
        )),
        field('right', $.expression),
      ),
      // `x = #ifndef A v1 #else v2 #endif`: the right operand may live
      // inside a preproc block, or the block may sit before it.
      seq(
        field('left', $._assignment_left_expression),
        field('operator', choice(
          '=',
          '*=',
          '/=',
          '%=',
          '+=',
          '-=',
          '<<=',
          '>>=',
          '&=',
          '^=',
          '|=',
        )),
        repeat1($._infix_continuation_preproc),
        optional(field('right', $.expression)),
      ),
    )),

    pointer_expression: $ => prec.left(PREC.CAST, seq(
      field('operator', choice('*', '&')),
      field('argument', $.expression),
    )),

    unary_expression: $ => prec.left(PREC.UNARY, seq(
      field('operator', choice('!', '~', '-', '+')),
      field('argument', $.expression),
    )),

    binary_expression: $ => {
      const table = [
        ['+', PREC.ADD],
        ['-', PREC.ADD],
        ['*', PREC.MULTIPLY],
        ['/', PREC.MULTIPLY],
        ['%', PREC.MULTIPLY],
        ['||', PREC.LOGICAL_OR],
        ['&&', PREC.LOGICAL_AND],
        ['|', PREC.INCLUSIVE_OR],
        ['^', PREC.EXCLUSIVE_OR],
        ['&', PREC.BITWISE_AND],
        ['==', PREC.EQUAL],
        ['!=', PREC.EQUAL],
        ['>', PREC.RELATIONAL],
        ['>=', PREC.RELATIONAL],
        ['<=', PREC.RELATIONAL],
        ['<', PREC.RELATIONAL],
        ['<<', PREC.SHIFT],
        ['>>', PREC.SHIFT],
      ];

      return choice(
        ...table.map(([operator, precedence]) => {
          return prec.left(precedence, seq(
            field('left', $.expression),
            // @ts-ignore
            field('operator', operator),
            field('right', $.expression),
          ));
        }),
        // `a | b #ifdef X | c #endif`: the tail of a binary chain can be
        // split into conditional blocks carrying `op operand` pairs.
        seq(
          field('left', $.expression),
          repeat1($._infix_continuation_preproc),
        ),
        // `a + #ifdef X b + #endif c` / `a && #ifdef X b #endif`: a block
        // may sit between the operator and the right operand — or hold
        // the right operand entirely.
        ...table.map(([operator, precedence]) => {
          return prec.left(precedence, seq(
            field('left', $.expression),
            // @ts-ignore
            field('operator', operator),
            repeat1($._infix_continuation_preproc),
            field('right', $.expression),
          ));
        }),
        ...table.map(([operator, precedence]) => {
          return prec.left(precedence, seq(
            field('left', $.expression),
            // @ts-ignore
            field('operator', operator),
            repeat1($._infix_continuation_preproc),
          ));
        }),
      );
    },

    _infix_continuation_preproc: $ => choice(
      alias($.preproc_if_in_argument_list, $.preproc_if),
      alias($.preproc_ifdef_in_argument_list, $.preproc_ifdef),
    ),

    update_expression: $ => {
      const argument = field('argument', $.expression);
      const operator = field('operator', choice('--', '++'));
      return prec.right(PREC.UNARY, choice(
        seq(operator, argument),
        seq(argument, operator),
      ));
    },

    cast_expression: $ => prec(PREC.CAST, seq(
      '(',
      field('type', $.type_descriptor),
      ')',
      field('value', $.expression),
    )),

    type_descriptor: $ => seq(
      repeat($.type_qualifier),
      field('type', $.type_specifier),
      repeat($.type_qualifier),
      field('declarator', optional($._abstract_declarator)),
    ),

    sizeof_expression: $ => prec(PREC.SIZEOF, seq(
      'sizeof',
      choice(
        field('value', $.expression),
        seq('(', field('type', $.type_descriptor), ')'),
      ),
    )),

    alignof_expression: $ => prec(PREC.SIZEOF, seq(
      choice('__alignof__', '__alignof', '_alignof', 'alignof', '_Alignof'),
      seq('(', field('type', $.type_descriptor), ')'),
    )),

    offsetof_expression: $ => prec(PREC.OFFSETOF, seq(
      'offsetof',
      seq('(', field('type', $.type_descriptor), ',', field('member', $._field_identifier), ')'),
    )),

    generic_expression: $ => prec(PREC.CALL, seq(
      '_Generic',
      '(',
      $.expression,
      ',',
      commaSep1(seq($.type_descriptor, ':', $.expression)),
      ')',
    )),

    subscript_expression: $ => prec(PREC.SUBSCRIPT, seq(
      field('argument', $.expression),
      '[',
      field('index', $.expression),
      ']',
    )),

    call_expression: $ => prec(PREC.CALL, seq(
      field('function', $.expression),
      field('arguments', $.argument_list),
    )),

    gnu_asm_expression: $ => prec(PREC.CALL, seq(
      choice('asm', '__asm__', '__asm'),
      repeat($.gnu_asm_qualifier),
      '(',
      field('assembly_code', $._string),
      optional(seq(
        field('output_operands', $.gnu_asm_output_operand_list),
        optional(seq(
          field('input_operands', $.gnu_asm_input_operand_list),
          optional(seq(
            field('clobbers', $.gnu_asm_clobber_list),
            optional(field('goto_labels', $.gnu_asm_goto_list)),
          )),
        )),
      )),
      ')',
    )),

    // MSVC `__asm { ... }` / `_asm { ... }` inline-assembly block: a
    // statement, terminated by the closing brace with no semicolon.
    ms_asm_statement: $ => seq(
      choice('__asm', '_asm'),
      field('assembly_block', $.ms_asm_block),
    ),

    // MSVC `__asm { push ecx \ mov eax, [ebx+4] }` inline-assembly block.
    // Instruction bodies are not C: accept a broad token soup between the
    // braces rather than grammar real mnemonics (they are not part of the
    // C/C++ token set and names like `cpuid`/`xmm0` are just identifiers).
    ms_asm_block: $ => seq(
      '{',
      repeat(choice(
        $.identifier,
        $.primitive_type,
        $.number_literal,
        $.string_literal,
        $.char_literal,
        $.comment,
        ',', ';', ':', '[', ']', '(', ')',
        '.', '->', '::',
        '+', '-', '*', '/', '%',
        '&', '&&', '|', '||', '^', '~', '!',
        '=', '==', '!=', '<', '>', '<<', '>>', '<=', '>=',
        '?', '...',
      )),
      '}',
    ),

    gnu_asm_qualifier: _ => choice(
      'volatile',
      '__volatile__',
      'inline',
      'goto',
    ),

    gnu_asm_output_operand_list: $ => seq(
      ':',
      commaSep(field('operand', $.gnu_asm_output_operand)),
    ),

    gnu_asm_output_operand: $ => seq(
      optional(seq(
        '[',
        field('symbol', $.identifier),
        ']',
      )),
      field('constraint', $.string_literal),
      '(',
      field('value', $.expression),
      ')',
    ),

    gnu_asm_input_operand_list: $ => seq(
      ':',
      commaSep(field('operand', $.gnu_asm_input_operand)),
    ),

    gnu_asm_input_operand: $ => seq(
      optional(seq(
        '[',
        field('symbol', $.identifier),
        ']',
      )),
      field('constraint', $.string_literal),
      '(',
      field('value', $.expression),
      ')',
    ),

    gnu_asm_clobber_list: $ => seq(
      ':',
      commaSep(field('register', $._string)),
    ),

    gnu_asm_goto_list: $ => seq(
      ':',
      commaSep(field('label', $.identifier)),
    ),

    extension_expression: $ => seq('__extension__', $.expression),

    // The compound_statement is added to parse macros taking statements as arguments, e.g. MYFORLOOP(1, 10, i, { foo(i); bar(i); })
    // A real argument may follow a preproc block comma-free because the
    // separator comma often hides inside the block (`f(a #ifdef X , b
    // #endif c)`); between real arguments commas stay strict.
    _argument_list_real: $ => choice($.expression, $.compound_statement, $._argument_list_type, $._argument_list_operator),

    // Assertion macros (munit, CMocka) take a bare comparison operator:
    // `assert_int(0, ==, rv)`. No expression can start with these tokens,
    // so accepting them in argument position is unambiguous.
    _argument_list_operator: $ => choice('==', '!=', '<=', '>=', '<', '>'),

    // A bare type as a call argument: `SWAP(void *, &a, &b)`,
    // `SWAP(const char *, ...)`, `M(unsigned int, x)`. Restricted to
    // primitive_type and sized_type_specifier so `T *` never competes
    // with `a * b` on the identifier-expression path.
    _argument_list_type: $ => prec.dynamic(-1, seq(
      repeat($.type_qualifier),
      choice(
        $.primitive_type,
        $.sized_type_specifier,
        $.struct_specifier,
        $.union_specifier,
        $.enum_specifier,
      ),
      repeat('*'),
    )),

    // Free-atom stream: real arguments, preproc blocks/directives and
    // separator commas interleave in any order. `#define`/`#undef`
    // between items (generated lookup tables redefining an X-macro) join
    // the preproc stream, so `f(#define X ... GEN(X) #undef X)` needs no
    // comma bookkeeping.
    _argument_list_preproc: $ => choice(
      alias($.preproc_if_in_argument_list, $.preproc_if),
      alias($.preproc_ifdef_in_argument_list, $.preproc_ifdef),
      $.preproc_def,
      $.preproc_call,
      $.preproc_null,
    ),

    // Real args need commas between them; preproc atoms stay a free stream.
    // Comma-free `"a" "b"` must therefore reduce through concatenated_string
    // instead of splitting into two items.
    argument_list: $ => seq(
      '(',
      repeat(choice(
        seq($._argument_list_real, ','),
        $._argument_list_preproc,
        ',',
      )),
      optional($._argument_list_real),
      ')',
    ),

    field_expression: $ => seq(
      prec(PREC.FIELD, seq(
        field('argument', $.expression),
        field('operator', choice('.', '->')),
      )),
      field('field', $._field_identifier),
    ),

    compound_literal_expression: $ => seq(
      '(',
      field('type', $.type_descriptor),
      ')',
      field('value', $.initializer_list),
    ),

    parenthesized_expression: $ => seq(
      '(',
      choice(
        $.expression,
        $.comma_expression,
        $.compound_statement,
        seq(
          repeat1($._infix_continuation_preproc),
          repeat(choice(
            $._infix_continuation_preproc,
            $.expression,
            $.comma_expression,
          )),
        ),
      ),
      ')',
    ),

    // `{a, #ifdef X ,b #endif ,c}` and `{a, #ifdef X b, #endif c}`:
    // the separator comma may hide inside an adjacent preproc block, so a
    // real item may follow a block comma-free.
    _initializer_list_real: $ => choice(
      $.initializer_pair,
      $.expression,
      $.initializer_list,
    ),

    _initializer_list_preproc: $ => choice(
      alias($.preproc_if_in_initializer_list, $.preproc_if),
      alias($.preproc_ifdef_in_initializer_list, $.preproc_ifdef),
      $.preproc_def,
      $.preproc_call,
      $.preproc_null,
    ),

    // Free-atom stream like argument_list: `{ #define TD(A) #A,
    // GEN(TD) #undef TD }` chains preproc and real atoms without commas.
    initializer_list: $ => seq(
      '{',
      repeat(choice(
        $._initializer_list_real,
        $._initializer_list_preproc,
        ',',
      )),
      '}',
    ),

    initializer_pair: $ => choice(
      seq(
        field('designator', repeat1(choice(
          $.subscript_designator,
          $.field_designator,
          $.subscript_range_designator,
        ))),
        '=',
        field('value', choice($.expression, $.initializer_list)),
      ),
      seq(
        field('designator', $._field_identifier),
        ':',
        field('value', choice($.expression, $.initializer_list)),
      ),
      // `.local{expr}`: a braced designated initializer drops the `=`.
      seq(
        field('designator', repeat1(choice(
          $.subscript_designator,
          $.field_designator,
          $.subscript_range_designator,
        ))),
        field('value', $.initializer_list),
      ),
    ),

    subscript_designator: $ => seq('[', $.expression, ']'),

    subscript_range_designator: $ => seq('[', field('start', $.expression), '...', field('end', $.expression), ']'),

    field_designator: $ => seq('.', $._field_identifier),

    number_literal: _ => {
      const separator = '\'';
      const hex = /[0-9a-fA-F]/;
      const decimal = /[0-9]/;
      const hexDigits = seq(repeat1(hex), repeat(seq(separator, repeat1(hex))));
      const decimalDigits = seq(repeat1(decimal), repeat(seq(separator, repeat1(decimal))));
      return token(seq(
        optional(/[-\+]/),
        optional(choice(/0[xX]/, /0[bB]/)),
        choice(
          seq(
            choice(
              decimalDigits,
              seq(/0[bB]/, decimalDigits),
              seq(/0[xX]/, hexDigits),
            ),
            optional(seq('.', optional(hexDigits))),
          ),
          seq('.', decimalDigits),
        ),
        optional(seq(
          /[eEpP]/,
          optional(seq(
            optional(/[-\+]/),
            hexDigits,
          )),
        )),
        /[uUlLwWfFbBdD]*/,
      ));
    },

    char_literal: $ => seq(
      choice('L\'', 'u\'', 'U\'', 'u8\'', '\''),
      repeat1(choice(
        $.escape_sequence,
        alias(token.immediate(/[^\n']/), $.character),
      )),
      '\'',
    ),

    // Must concatenate at least 2 nodes, one of which must be a string_literal.
    // Identifier is added to parse macros that are strings, like PRIu64.
    concatenated_string: $ => prec.dynamic(2, prec.right(seq(
      choice(
        seq($.identifier, $.string_literal),
        seq($.string_literal, $.string_literal),
        seq($.string_literal, $.identifier),
        // Stringification/expansion macros take part in literal
        // concatenation: `XSTR(a) "." XSTR(b)`, `"v" FLATBUFFERS_STRING(x)`.
        seq($.call_expression, $.string_literal),
        seq($.string_literal, $.call_expression),
        seq($.call_expression, $.call_expression),
      ),
      repeat(choice($.string_literal, $.identifier, $.call_expression)),
    ))),

    string_literal: $ => seq(
      choice('L"', 'u"', 'U"', 'u8"', '"'),
      repeat(choice(
        alias(token.immediate(prec(1, /[^\\"\n]+/)), $.string_content),
        $.escape_sequence,
      )),
      '"',
    ),

    escape_sequence: _ => token(prec(1, seq(
      '\\',
      choice(
        /[^xuU]/,
        /\d{2,3}/,
        /x[0-9a-fA-F]{1,4}/,
        /u[0-9a-fA-F]{4}/,
        /U[0-9a-fA-F]{8}/,
      ),
    ))),

    system_lib_string: _ => token(seq(
      '<',
      repeat(choice(/[^>\n]/, '\\>')),
      '>',
    )),

    true: _ => token(choice('TRUE', 'true')),
    false: _ => token(choice('FALSE', 'false')),
    null: _ => choice('NULL', 'nullptr'),

    identifier: _ =>
      /(\p{XID_Start}|\$|_|\\u[0-9A-Fa-f]{4}|\\U[0-9A-Fa-f]{8})(\p{XID_Continue}|\$|\\u[0-9A-Fa-f]{4}|\\U[0-9A-Fa-f]{8})*/,

    _type_identifier: $ => alias(
      $.identifier,
      $.type_identifier,
    ),
    _field_identifier: $ => alias($.identifier, $.field_identifier),
    _statement_identifier: $ => alias($.identifier, $.statement_identifier),

    _empty_declaration: $ => seq(
      $.type_specifier,
      ';',
    ),

    macro_type_specifier: $ => prec.dynamic(-1, seq(
      field('name', choice(
        $.identifier,
        alias('_Atomic', $.identifier),
      )),
      '(',
      field('type', $.type_descriptor),
      ')',
    )),

    // http://stackoverflow.com/questions/13014947/regex-to-match-a-c-style-multiline-comment/36328890#36328890
    comment: _ => token(choice(
      seq('//', /(\\+(.|\r?\n)|[^\\\n])*/),
      seq(
        '/*',
        /[^*]*\*+([^/*][^*]*\*+)*/,
        '/',
      ),
    )),
  },
});

/**
 *
 * @param {string} suffix
 *
 * @param {RuleBuilder<string>} content
 *
 * @param {number} precedence
 *
 * @returns {RuleBuilders<string, string>}
 */
function preprocIf(suffix, content, precedence = 0) {
  /**
   *
   * @param {GrammarSymbols<string>} $
   *
   * @returns {ChoiceRule}
   */
  function alternativeBlock($) {
    return choice(
      suffix ? alias($['preproc_else' + suffix], $.preproc_else) : $.preproc_else,
      suffix ? alias($['preproc_elif' + suffix], $.preproc_elif) : $.preproc_elif,
      suffix ? alias($['preproc_elifdef' + suffix], $.preproc_elifdef) : $.preproc_elifdef,
    );
  }

  return {
    ['preproc_if' + suffix]: $ => prec(precedence, seq(
      preprocessor('if'),
      field('condition', $._preproc_expression),
      '\n',
      repeat(content($)),
      field('alternative', optional(alternativeBlock($))),
      preprocessor('endif'),
    )),

    ['preproc_ifdef' + suffix]: $ => prec(precedence, seq(
      choice(preprocessor('ifdef'), preprocessor('ifndef')),
      field('name', $.identifier),
      repeat(content($)),
      field('alternative', optional(alternativeBlock($))),
      preprocessor('endif'),
    )),

    ['preproc_else' + suffix]: $ => prec(precedence, seq(
      preprocessor('else'),
      repeat(content($)),
    )),

    ['preproc_elif' + suffix]: $ => prec(precedence, seq(
      preprocessor('elif'),
      field('condition', $._preproc_expression),
      '\n',
      repeat(content($)),
      field('alternative', optional(alternativeBlock($))),
    )),

    ['preproc_elifdef' + suffix]: $ => prec(precedence, seq(
      choice(preprocessor('elifdef'), preprocessor('elifndef')),
      field('name', $.identifier),
      repeat(content($)),
      field('alternative', optional(alternativeBlock($))),
    )),
  };
}

/**
 * Creates a preprocessor regex rule
 *
 * @param {RegExp | Rule | string} command
 *
 * @returns {AliasRule}
 */
function preprocessor(command) {
  return alias(new RegExp('#[ \t]*' + command), '#' + command);
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

module.exports.PREC = PREC;
module.exports.preprocIf = preprocIf;
module.exports.preprocessor = preprocessor;
module.exports.commaSep = commaSep;
module.exports.commaSep1 = commaSep1;
