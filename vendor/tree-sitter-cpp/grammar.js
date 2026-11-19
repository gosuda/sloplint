/**
 * @file C++ grammar for tree-sitter
 * @author Max Brunsfeld <maxbrunsfeld@gmail.com>
 * @author Amaan Qureshi <amaanq12@gmail.com>
 * @author John Drouhard <john@drouhard.dev>
 * @license MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const C = require('tree-sitter-c/grammar');

const PREC = Object.assign(C.PREC, {
  LAMBDA: 18,
  NEW: C.PREC.CALL + 1,
  STRUCTURED_BINDING: -1,
  THREE_WAY: C.PREC.RELATIONAL + 1,
});

const FOLD_OPERATORS = [
  '+', '-', '*', '/', '%',
  '^', '&', '|',
  '=', '<', '>',
  '<<', '>>',
  '+=', '-=', '*=', '/=', '%=', '^=', '&=', '|=',
  '>>=', '<<=',
  '==', '!=', '<=', '>=',
  '&&', '||',
  ',',
  '.*', '->*',
  'or', 'and', 'bitor', 'xor', 'bitand', 'not_eq',
];

const ASSIGNMENT_OPERATORS = [
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
  'and_eq',
  'or_eq',
  'xor_eq',
];

module.exports = grammar(C, {
  name: 'cpp',

  externals: $ => [
    $.raw_string_delimiter,
    $.raw_string_content,
    // A `{` that only lexes when a `;` appears inside the brace region,
    // so `Type{arg}` braced temporaries can never open a macro block.
    $._macro_braced_open,
  ],

  conflicts: $ => [
        // `#ifdef`-arm specifiers vs a bare `#ifdef` arm vs an `extern "C"`
    // guarded block — all can open after a macro/identifier prefix.
    [$.preproc_ifdef, $.preproc_ifdef_in_declaration_specifiers, $._linkage_guard, $.preproc_ifdef_in_guarded_signature],
    [$.preproc_if, $.preproc_if_in_declaration_specifiers, $._linkage_guard, $.preproc_if_in_guarded_signature],
    [$.preproc_ifdef, $.preproc_ifdef_in_declaration_specifiers],
    [$.preproc_else, $.preproc_else_in_declaration_specifiers, $.preproc_else_in_guarded_signature, $.preproc_else_in_guarded_class],
    [$.preproc_ifdef_in_declaration_specifiers, $._declaration_specifiers, $._constructor_specifiers],
    [$.preproc_ifdef_in_declaration_specifiers, $._declaration_specifiers],
    [$.preproc_if, $.preproc_if_in_declaration_specifiers],
    [$.preproc_else, $.preproc_else_in_declaration_specifiers],
    [$.preproc_else_in_declaration_specifiers, $._declaration_specifiers, $._constructor_specifiers],
    [$.preproc_else_in_declaration_specifiers, $._declaration_specifiers],
    [$.preproc_elifdef, $.preproc_elifdef_in_declaration_specifiers, $.preproc_elifdef_in_guarded_signature, $.preproc_elifdef_in_guarded_class],
    [$._class_declaration_item, $._guarded_class_head],
    [$.type_qualifier],
    [$.type_definition],
    [$.preproc_else, $.preproc_else_in_declaration_specifiers, $.preproc_else_in_initializer_list, $.preproc_else_in_guarded_signature, $.preproc_else_in_guarded_class],
            [$.preproc_elif, $.preproc_elif_in_declaration_specifiers, $.preproc_elif_in_initializer_list, $.preproc_elif_in_guarded_signature, $.preproc_elif_in_guarded_class],
    [$.preproc_elifdef, $.preproc_elifdef_in_declaration_specifiers, $.preproc_elifdef_in_initializer_list, $.preproc_elifdef_in_guarded_signature, $.preproc_elifdef_in_guarded_class],
    [$.preproc_if, $.preproc_if_in_declaration_specifiers, $.preproc_if_in_initializer_list],
    [$.preproc_ifdef, $.preproc_ifdef_in_declaration_specifiers, $.preproc_ifdef_in_initializer_list],
            [$.preproc_ifdef_in_declaration_specifiers, $.preproc_ifdef_in_field_declaration_list],
    [$.preproc_if_in_declaration_specifiers, $.preproc_if_in_field_declaration_list],
    [$.preproc_else_in_declaration_specifiers, $.preproc_else_in_field_declaration_list],
    [$.preproc_elif_in_declaration_specifiers, $.preproc_elif_in_field_declaration_list],
    [$.preproc_elifdef_in_declaration_specifiers, $.preproc_elifdef_in_field_declaration_list],
    [$.preproc_elif, $.preproc_elif_in_declaration_specifiers, $.preproc_elif_in_guarded_signature, $.preproc_elif_in_guarded_class],
        [$.preproc_if, $.preproc_if_in_declaration_specifiers],
    [$.preproc_ifdef, $.preproc_ifdef_in_declaration_specifiers],
        [$.preproc_elif, $.preproc_elif_in_declaration_specifiers],
    [$.preproc_elifdef, $.preproc_elifdef_in_declaration_specifiers],
    [$.preproc_else, $.preproc_else_in_declaration_specifiers],
    [$.preproc_else_in_enumerator_list, $.preproc_else_in_enumerator_list_no_comma, $.expression],
    [$.preproc_ifdef_in_enumerator_list, $.preproc_ifdef_in_enumerator_list_no_comma, $.expression],
    [$.enumerator_list, $.expression],
    // `identifier (` opens a call, a declarator, a macro-type specifier, or
    // the generic macro_declaration/macro_qualifier fallbacks — all five can
    // coexist where declarations and statements interleave. Dynamic -1 on
    // the macro rules keeps real constructs winning at runtime.
    // `identifier ~` leads a destructor vs a macro fallback.
    [$.macro_declaration, $.macro_qualifier, $.type_specifier],
    // Bare `identifier` at statement start: expression vs macro fallback.
    // `identifier *|&|::|<|…` — multiply/pointer/member reads vs the macro
    // fallbacks; one broad set for every post-identifier continuation.
    // `Name {` in class position: class name vs bare macro marker.
    // `identifier [[` — attribute-attached declarator vs macro fallback.
    // `identifier __cdecl|const|…` — type followed by modifier vs fallback.
    [$.macro_declaration, $.type_specifier],
    // `identifier int|void|…` — modifier-then-primitive vs the two fallbacks.
    [$.macro_qualifier, $.macro_declaration],
    // `{ identifier ;` inside class bodies: expr-statement, field decl, or
    // bare macro marker — three-way for the optional-semicolon generic.
    // `enum Name {` — enum name vs export-macro qualifier.
    [$.macro_qualifier, $._class_name],
    // `identifier ;` at top level before a preproc directive: expression
    // statement vs generic macro item.
    // `template Foo (` — explicit instantiation declarator vs qualifier.
    [$.macro_qualifier, $._declarator, $.type_specifier],
    // `Foo __extension__|modifier` — qualifier macro vs type.
    [$.macro_qualifier, $.type_specifier],
    // `operator T (` / `Foo (` declarator-vs-qualifier after operator casts.
    // `name [` in class bodies: array/bitfield declarator vs macro marker.
    // `if (Foo (` — condition declaration (lock-guard idiom) vs call.
    [$.macro_qualifier, $.type_specifier, $.expression],
    // C
    [$.type_specifier, $._declarator],
    [$.type_specifier, $.expression],
    [$.sized_type_specifier],
    [$.attributed_statement],
    [$._declaration_modifiers, $.attributed_statement],
    [$._top_level_item, $._top_level_statement],
    [$.macro_braced_statement],
    [$.macro_braced_expression],
    [$.macro_braced_declaration],
    [$.macro_braced_statement, $.macro_braced_expression],
    [$.expression, $.macro_braced_expression],
    // `NAME {` inside a class/namespace body: the member body can hold
    // `;`-terminated items that also look like expression statements.
    [$._declarator, $.expression, $.macro_braced_expression],
    [$.macro_qualifier, $._declarator, $.expression, $.macro_braced_expression],
    [$._declarator, $.type_specifier, $.expression, $.macro_braced_expression],
    [$._macro_declaration_stmt, $.expression, $.macro_braced_statement, $._macro_braced_declaration_paren, $.macro_braced_expression],
    [$.macro_qualifier, $._macro_declaration_stmt, $._declarator, $.type_specifier, $.expression, $.macro_braced_statement, $._macro_braced_declaration_paren, $.macro_braced_expression],
    [$.expression, $.concatenated_string, $.macro_braced_expression],
    [$.type_specifier, $.expression, $.macro_braced_expression],
    [$.enumerator, $.expression, $.macro_braced_expression],
    [$.macro_qualifier, $._macro_declaration_stmt, $.type_specifier, $.expression, $.macro_braced_statement, $._macro_braced_declaration_paren, $.macro_braced_expression],
    [$.macro_qualifier, $.type_specifier, $.expression, $.macro_braced_expression],
                    [$.macro_argument_list, $.expression],
    [$.macro_argument_list, $.expression, $.template_type, $.template_function],
                    [$.macro_argument_list, $._string],
    [$.macro_argument_list, $._string, $.concatenated_string],
    [$.macro_argument_list, $._class_name],
    [$.macro_argument_list, $.type_specifier, $.call_expression],
    [$.macro_argument_list, $.compound_literal_expression],
    [$.macro_argument_list, $.type_specifier, $.expression],
    [$.macro_argument_list, $.type_specifier, $.expression, $.macro_braced_expression],
    [$.macro_argument_list, $.concatenated_string],
    [$._midl_attribute, $.expression, $.macro_braced_expression],
    [$._macro_braced_body, $._macro_braced_members],
    [$._block_item, $._macro_braced_members],
    [$.macro_argument_list, $.sized_type_specifier, $._macro_argument_list_nonempty],
    [$.macro_argument_list, $.template_type, $._macro_argument_list_nonempty],
    [$.macro_qualifier, $._declarator, $._field_declarator],
    [$.macro_qualifier, $._declarator, $._field_declarator, $._init_pointer_declarator],
    [$._declarator, $._field_declarator, $._init_reference_declarator],
    [$.macro_argument_list, $.type_specifier, $._macro_argument_list_nonempty],
    [$._init_declarator_preproc, $._declarator, $._field_declarator],
    [$._declarator, $._field_declarator],
    [$.labeled_statement, $.macro_access_label],
    [$._midl_attribute, $.structured_binding_declarator, $._lambda_capture_identifier],
        [
      $.macro_qualifier,
      $.macro_declaration,
      $._declarator,
      $.type_specifier,
      $.macro_type_specifier,
      $.macro_braced_declaration,
    ],
    [$._block_item, $.statement],
    [$.type_qualifier, $.extension_expression],
    // C++
    [$.template_function, $.template_type],
    [$.template_function, $.template_type, $.expression],
    [$.template_function, $.template_type, $.qualified_identifier],
    [$.template_type, $.qualified_type_identifier],
    [$.qualified_type_identifier, $.qualified_identifier],
    [$.expression, $._declarator],
    [$.expression, $.structured_binding_declarator],
    [$.expression, $._declarator, $.type_specifier],
    [$.expression, $.identifier_parameter_pack_expansion],
    [$.expression, $._lambda_capture_identifier],
    [$.expression, $._lambda_capture],
    [$.expression, $.structured_binding_declarator, $._lambda_capture_identifier],
    [$.structured_binding_declarator, $._lambda_capture_identifier],
    [$.parameter_list, $.argument_list],
    [$.type_specifier, $.call_expression],
    [$._declaration_modifiers, $.type_specifier],
    [$._declaration_specifiers, $._constructor_specifiers],
    [$._binary_fold_operator, $._fold_operator],
    [$._function_declarator_seq],
    [$.type_specifier, $.sized_type_specifier],
    [$.initializer_pair, $.comma_expression],
    [$.expression_statement, $._for_statement_body],
    [$.init_statement, $._for_statement_body],
    [$.field_expression, $.template_method, $.template_type],
    [$.qualified_field_identifier, $.template_method, $.template_type],
    [$.storage_class_specifier, $.template_instantiation],
    [$.field_expression, $.template_method],
    [$.qualified_field_identifier, $.template_method],
    [$.type_specifier, $._class_name],
    [$._class_name, $.expression],
    [$.macro_declaration, $._declarator],
    [$.preproc_ifdef, $.preproc_ifdef_in_guarded_signature, $.preproc_ifdef_in_guarded_class],
        [$.preproc_if, $.preproc_if_in_guarded_signature, $.preproc_if_in_guarded_class],
        [$.preproc_ifdef, $.preproc_ifdef_in_initializer_list, $.preproc_ifdef_in_guarded_signature, $.preproc_ifdef_in_guarded_class],
        [$.preproc_if, $.preproc_if_in_initializer_list, $.preproc_if_in_guarded_signature, $.preproc_if_in_guarded_class],
            [$._template_argument, $._template_relational_argument],
    [$.expression, $._template_relational_operand],
    [$.expression, $.template_type, $.template_method, $.template_function, $._template_relational_operand],
    [$._string, $._template_relational_operand],
    [$._string, $.concatenated_string, $._template_relational_operand],
    [$.template_method, $.template_function],
    [$.binary_expression, $._template_tight_binary],
    [$._init_declarator_preproc, $._declarator],
    [$.pointer_declarator, $._init_pointer_declarator],
    [$._declarator, $._init_reference_declarator],
    [$.function_definition, $.pointer_declarator],
    [$.parenthesized_declarator, $.pointer_declarator],
    [$.pointer_declarator],
    [$.abstract_parenthesized_declarator, $.abstract_pointer_declarator],
    [$.abstract_pointer_declarator],
    [$.pointer_type_declarator],
    [$.parenthesized_type_declarator, $.pointer_type_declarator],
    [$.parenthesized_field_declarator, $.pointer_field_declarator],
    [$.pointer_field_declarator],
    [$.pointer_declarator, $.abstract_pointer_declarator],
    [$._declaration_modifiers, $.pointer_declarator, $.type_specifier],
    [$.pointer_declarator, $.type_specifier],
    [$._declaration_modifiers, $.pointer_declarator],
    [$.parenthesized_declarator, $.type_specifier],
    [$.parenthesized_declarator, $.pointer_declarator, $.type_specifier],
    [$.abstract_parenthesized_declarator, $.type_specifier],
    [$.abstract_parenthesized_declarator, $.abstract_pointer_declarator, $.type_specifier],
    [$._declaration_modifiers, $.abstract_parenthesized_declarator, $.abstract_pointer_declarator, $.type_specifier],
    [$.parenthesized_declarator, $.abstract_parenthesized_declarator, $.type_specifier],
    [$.parenthesized_declarator, $.abstract_parenthesized_declarator, $.pointer_declarator, $.abstract_pointer_declarator, $.type_specifier],
    [$._declaration_modifiers, $.parenthesized_declarator, $.abstract_parenthesized_declarator, $.pointer_declarator, $.abstract_pointer_declarator, $.type_specifier],
    [$._declaration_modifiers, $.parenthesized_declarator, $.abstract_parenthesized_declarator, $.type_specifier],
    [$.macro_qualifier, $._declarator, $.expression],
    [$.macro_qualifier, $.expression],
    [$.macro_qualifier, $._declarator],
    [$.macro_qualifier, $._declarator, $._init_pointer_declarator],
    [$.macro_qualifier, $._type_declarator],
    [$.macro_qualifier, $._field_declarator],
    [$._abstract_declarator, $.optional_parameter_declaration],
    [$.ms_pointer_modifier, $.qt_signature_type],
    [$.parameter_declaration, $.variadic_parameter_declaration],
    [$.parameter_declaration, $.optional_parameter_declaration],
    [$.parameter_declaration],
    [$.variadic_declarator],
    [$.attributed_declarator, $.parameter_declaration],
    [$._function_attributes_start, $._function_attributes_end],
    [$.template_type, $.template_method],
    [$.expression, $.concatenated_string],
    [$._string, $.concatenated_string],
    [$.preproc_ifdef, $.preproc_ifdef_in_initializer_list],
    [$.preproc_if, $.preproc_if_in_initializer_list],
    [$._parameter_list_item],
    [$._field_initializer_list_item],
    [$.comma_expression, $._initializer_list_real],
    [$._in_parameter_list_item, $._in_argument_list_item],
    [$.preproc_ifdef_in_parameter_list, $.preproc_ifdef_in_argument_list],
    [$.preproc_else_in_parameter_list, $.preproc_else_in_argument_list],
    [$.preproc_if_in_parameter_list, $.preproc_if_in_argument_list],
    [$.preproc_elifdef_in_parameter_list, $.preproc_elifdef_in_argument_list],
    [$.preproc_elif_in_parameter_list, $.preproc_elif_in_argument_list],
    [$.binary_expression, $._argument_list_real],
    [$.binary_expression, $.extension_expression],
    [$.binary_expression, $.delete_expression],
    [$.binary_expression],
    [$.binary_expression, $.optional_parameter_declaration],
    [$.expression, $.field_designator],
    [$.labeled_statement, $.dangling_label],
    [$.labeled_statement, $.dangling_label, $.initializer_pair],
    [$.binary_expression, $.parenthesized_expression],
    [$.parenthesized_expression],
    [$.field_expression, $._in_argument_list_item],
    [$.comma_expression, $.binary_expression],
    [$.expression, $.field_designator, $._class_name],
    [$.preproc_ifdef_in_initializer_list, $.field_expression],
    [$.preproc_else_in_initializer_list, $.field_expression],
    [$.field_initializer_list],
    [$.expression, $.concatenated_string, $.template_type, $.template_function],
    [$.preproc_ifdef, $.type_qualifier, $.extension_expression],
    [$.preproc_ifdef, $.type_qualifier],
    [$.preproc_ifdef, $.extension_expression],
    [$.preproc_ifdef, $.type_definition],
    [$.enumerator_list, $.extension_expression],
    [$.enumerator, $.expression],
    [$.enumerator, $.concatenated_string],
    [$.enumerator_list, $.concatenated_string],
    [$.concatenated_string],
    [$.constructor_or_destructor_definition],
    [$._scope_resolution],
    [$.preproc_else, $.type_qualifier, $.extension_expression],
    [$.preproc_else, $.type_qualifier],
    [$.preproc_else, $.extension_expression],
    [$.preproc_else, $.type_definition],
    [$._block_item, $._initializer_list_preproc],
    [$.concatenated_string, $._class_name],
    [$.preproc_ifdef_in_enumerator_list, $.preproc_ifdef_in_enumerator_list_no_comma],
    [$.preproc_ifdef_in_enumerator_list, $.preproc_ifdef_in_enumerator_list_no_comma, $.extension_expression],
    [$.preproc_ifdef_in_enumerator_list, $.preproc_ifdef_in_enumerator_list_no_comma, $.concatenated_string],
    [$.preproc_else_in_enumerator_list, $.preproc_else_in_enumerator_list_no_comma],
    [$.preproc_else_in_enumerator_list, $.preproc_else_in_enumerator_list_no_comma, $.extension_expression],
    [$.preproc_else_in_enumerator_list, $.preproc_else_in_enumerator_list_no_comma, $.concatenated_string],
    [$.extension_expression],
    [$._block_item, $.preproc_ifdef_in_initializer_list],
    [$.preproc_ifdef_in_initializer_list, $.comma_expression],
    [$._block_item, $.preproc_else_in_initializer_list],
    [$.preproc_else_in_initializer_list, $.comma_expression],
        [$._macro_declaration_top, $.type_specifier, $.expression],
    [$._macro_declaration_top, $._top_level_expression_statement],
    [$.macro_qualifier, $._macro_declaration_top],
    // `asm(s : :: : x)` — the second `:` could extend the classic
    // operand lists or the fused `::` sections; fused wins only when a
    // real `::` token follows (it is the dynamic(-1) fallback).
    // `IDENT {` — braced statement macro vs class name / brace
    // initializer; the dynamic(-1) macro read only survives when the
    // braces hold statements.
    // `:: "lit" (` — fused operand: string+constraint vs
    // concatenated-string call operand.
    // `::`-fused sections vs global-scope `::expr` inside operands.
    // In fused `::` sections a `"lit"(expr)` operand can't statically
    // tell input vs output — output wins deterministically.
    [$.macro_qualifier, $.macro_declaration, $._declarator, $.type_specifier],
    [$.macro_argument_list, $.sized_type_specifier],
    [$.sized_type_specifier, $.expression],
    [$._argument_list_type],
    [$.sized_type_specifier, $._argument_list_type],
    [$._declaration_modifiers, $._argument_list_type],
    [$.type_specifier, $._argument_list_type],
                [$.macro_qualifier, $._macro_declaration_stmt],
        [$._macro_declaration_top, $._macro_declaration_stmt],
    [$.macro_qualifier, $._macro_declaration_top, $._macro_declaration_stmt],
    [$._field_declaration_list_item, $.field_declaration],
    [$._midl_attribute, $.structured_binding_declarator],
    [$._midl_attribute, $._lambda_capture_identifier],
    [$._midl_attribute, $.expression],
    [$._midl_attribute, $.expression, $.structured_binding_declarator],
        [$.macro_argument_list, $.type_qualifier],
    [$.macro_argument_list, $.type_specifier],
    [$.call_expression, $._argument_list_type],
    [$._argument_list_type, $.compound_literal_expression],
    [$.type_specifier, $.call_expression, $._argument_list_type],
    [$._typedef_macro_qualifier, $.type_specifier],
    [$._typedef_macro_qualifier, $.type_specifier, $.sized_type_specifier],
    [$.macro_qualifier, $._macro_declaration_top, $.type_specifier],
    [$._macro_declaration_top, $.expression],
    [$._macro_declaration_top, $.type_specifier],
    [$._macro_declaration_top, $._declarator, $.type_specifier, $.expression],
    [$._macro_declaration_top, $._class_name],
    [$.macro_qualifier, $._macro_declaration_top, $._declarator, $.type_specifier],
    [$.macro_qualifier, $._macro_declaration_top, $.type_specifier, $.expression],
    [$._macro_declaration_top, $.expression, $._class_name],
    [$._macro_declaration_top, $.expression, $.concatenated_string],
    [$.macro_namespace, $._macro_declaration_literals],
            [$.function_definition, $.declaration, $.init_declarator, $._guarded_signature],
    [$.attributed_declarator, $._guarded_signature],
    [$._macro_qualifier_bare, $.type_specifier],
    [$._macro_qualifier_bare, $.sized_type_specifier],
    [$._macro_qualifier_bare, $.type_specifier, $.expression],
    [$._macro_qualifier_bare, $.sized_type_specifier, $.expression],
    [$._macro_qualifier_bare, $.expression],
    [$._macro_qualifier_bare, $._declarator, $.type_specifier],
    [$._macro_qualifier_bare, $.type_specifier, $.sized_type_specifier],
    [$._declaration_modifiers, $.abstract_parenthesized_declarator, $.type_specifier],
    [$.abstract_parenthesized_declarator, $.qualified_type_identifier],
    [$.abstract_parenthesized_declarator, $.qualified_identifier],
    [$.abstract_parenthesized_declarator, $.qualified_identifier, $.qualified_type_identifier],
    [$._type_declarator, $.template_type],
    [$.expression],
    // `NAME(args)` forks: call/declarator/macro_declaration vs a
    // `MACRO(args) { members };` braced declaration — decided only by
    // whether `_macro_braced_open` lexes after the `)`. The loose
    // `_macro_braced_declaration_loose` variant (empty `()` allowed)
    // takes the file/namespace/statement positions.
    [$.macro_qualifier, $._macro_declaration_top, $._declarator, $.type_specifier, $.expression, $.macro_braced_statement, $._macro_braced_declaration_loose, $.macro_braced_expression],
    [$._macro_braced_declaration_loose],
    [$._macro_braced_declaration_paren],
    [$._macro_braced_declaration_loose, $._macro_braced_declaration_paren],
    [$.macro_argument_list, $.template_type],
    [$.macro_qualifier, $._macro_declaration_top, $._macro_declaration_stmt, $._declarator, $.type_specifier, $.expression, $.macro_braced_statement, $._macro_braced_declaration_loose, $._macro_braced_declaration_paren, $.macro_braced_expression],
  ],

  inline: ($, original) => original.concat([
    $._namespace_identifier,
  ]),

  precedences: $ => [
    [$.argument_list, $.type_qualifier],
    [$._expression_not_binary, $._class_name],
  ],

  rules: {
    _top_level_item: ($, original) => choice(
      ...original.members.filter((member) => member.content?.name != '_old_style_function_definition'),
      $.namespace_definition,
      $.concept_definition,
      $.namespace_alias_definition,
      $.using_declaration,
      $.alias_declaration,
      $.static_assert_declaration,
      $.template_declaration,
      $.template_instantiation,
      alias($.constructor_or_destructor_definition, $.function_definition),
      alias($.operator_cast_definition, $.function_definition),
      alias($.operator_cast_declaration, $.declaration),
      // Strict variant — a generic `NAME` needs args or `;` (literal
      // Qt/UE names stay bare). Without the constraint `NAME(args)`
      // fragments into `macro_declaration(NAME)` plus a
      // `parenthesized_expression(args)` statement (MISSING `;`).
      alias(prec.dynamic(-2, prec.right($._macro_declaration_top)), $.macro_declaration),
      $.macro_namespace,
      alias($._macro_braced_declaration_loose, $.macro_braced_declaration),
      $.guarded_function,
      $.guarded_class,
    ),

    _block_item: ($, original) => choice(
      ...original.members.filter((member) => member.content?.name != '_old_style_function_definition'),
      $.namespace_definition,
      $.concept_definition,
      $.namespace_alias_definition,
      $.using_declaration,
      $.alias_declaration,
      $.static_assert_declaration,
      $.template_declaration,
      $.template_instantiation,
      alias($.constructor_or_destructor_definition, $.function_definition),
      alias($.operator_cast_definition, $.function_definition),
      alias($.operator_cast_declaration, $.declaration),
      // No `macro_braced_declaration` here: inside a function body the
      // statement-block read must win, and `NAME(args) {` falls back to
      // `function_definition` either way (upstream parity) — as a
      // member-list read it turned call bodies into phantom member
      // declarations (`ON_SCOPE_EXIT { cleanup(); };` losing the call).
      $.guarded_function,
      $.guarded_class,
      // No `macro_declaration` fallback here: top-level preproc bodies
      // get it via the preprocIf('', ...) item choice in the base
      // grammar, and inside real function bodies `NAME(args)` must
      // always stay a call_expression.
    ),

    // Namespace bodies repeat `_block_item`, but `NAME(args)` there is a
    // declaration macro (KDBINDINGS_DEFINE_*), never a call — expression
    // statements cannot appear in `declaration_list`, so the strict
    // variant is added here rather than to `_block_item` itself.
    declaration_list: $ => seq(
      '{',
      repeat(choice(
        $._block_item,
        alias($._macro_braced_declaration_loose, $.macro_braced_declaration),
        alias(prec.dynamic(-2, prec.right($._macro_declaration_top)), $.macro_declaration),
      )),
      '}',
    ),

    // Types

    placeholder_type_specifier: $ => prec(1, seq(
      field('constraint', optional($.type_specifier)),
      choice($.auto, alias($.decltype_auto, $.decltype)),
    )),

    auto: _ => 'auto',
    decltype_auto: $ => seq(
      'decltype',
      '(',
      $.auto,
      ')',
    ),
    decltype: $ => seq(
      'decltype',
      '(',
      // A comma expression is legal here — `decltype(expr, void())` is the
      // canonical SFINAE "expression is well-formed" probe.
      choice($.expression, $.comma_expression),
      ')',
    ),

    type_specifier: $ => choice(
      $.struct_specifier,
      $.union_specifier,
      $.enum_specifier,
      $.class_specifier,
      $.sized_type_specifier,
      $.primitive_type,
      $.template_type,
      $.dependent_type,
      $.placeholder_type_specifier,
      $.decltype,
      $.macro_attribute,
      prec.right(choice(
        alias($.qualified_type_identifier, $.qualified_identifier),
        $._type_identifier,
      )),
    ),

    // Replaces the base list (which it cannot subtract from): C++ has no
    // `noreturn` keyword (`[[noreturn]]` is an attribute), so the C23
    // spelling must not shadow identifier uses like `auto noreturn = ...`.
    type_qualifier: ($, original) => choice(
      'const',
      'constexpr',
      'volatile',
      'restrict',
      '__restrict__',
      '__extension__',
      '_Atomic',
      '_Noreturn',
      '_Nonnull',
      $.alignas_qualifier,
      'mutable',
      'constinit',
      'consteval',
    ),

    type_descriptor: (_, original) => prec.right(original),

    // When used in a trailing return type, these specifiers can now occur immediately before
    // a compound statement. This introduces a shift/reduce conflict that needs to be resolved
    // with an associativity.
    _class_declaration: $ => seq(
      repeat(choice($.attribute_specifier, $.alignas_qualifier)),
      optional($.ms_declspec_modifier),
      repeat($.attribute_declaration),
      $._class_declaration_item,
    ),
    _class_declaration_item: $ => prec.right(seq(
      // Export/annotation macros between `class`/`struct` and the name:
      // `class DOCKS_EXPORT Foo`, `struct FOO_API Bar`. macro_qualifier
      // covers names the literal list cannot enumerate; its dynamic -1
      // keeps `class FOO` reading FOO as the name.
      repeat(choice($.macro_attribute, $.macro_qualifier)),
      choice(
        field('name', $._class_name),
        seq(
          optional(field('name', $._class_name)),
          optional($.virtual_specifier),
          optional($.base_class_clause),
          field('body', $.field_declaration_list),
        ),
      ),
      optional($.attribute_specifier),
    )),

    class_specifier: $ => seq(
      'class',
      $._class_declaration,
    ),

    union_specifier: $ => seq(
      'union',
      $._class_declaration,
    ),

    struct_specifier: $ => seq(
      'struct',
      $._class_declaration,
    ),

    _class_name: $ => prec.right(choice(
      $._type_identifier,
      $.template_type,
      alias($.qualified_type_identifier, $.qualified_identifier),
    )),

    // `T f(int) = delete;` outside a class is a function definition, as inside one.
    function_definition: ($, original) => ({
      ...original,
      members: original.members.map(
        (e) => e.name !== 'body' ?
          e :
          choice(
            field('body', choice(e.content, $.try_statement)),
            $.delete_method_clause,
          )),
    }),

    declaration: $ => choice(
      seq(
        $._declaration_specifiers,
        commaSep1(field('declarator', choice(
          seq(
            // C uses _declaration_declarator here for some nice macro parsing in function declarators,
            // but this causes a world of pain for C++ so we'll just stick to the normal _declarator here.
            // The literal-only leading repeat covers `T __stdcall F(args);`
            // (calling-convention modifier between specifiers and name);
            // generic macro_qualifier must NOT join it — an identifier here is
            // the declarator, and that fork is the pain the comment above means.
            repeat(choice($.ms_call_modifier, $.macro_attribute)),
            $._declarator,
            optional($.gnu_asm_expression),
            repeat(choice($.ms_call_modifier, $.macro_attribute, $.macro_qualifier)),
          ),
          $.init_declarator,
        ))),
        ';',
      ),
      // `T x =\n#if A v1;\n#else v2;\n#endif`: the initializer and the
      // terminating `;` live inside the preproc arms.
      seq(
        $._declaration_specifiers,
        optional(seq(commaSep1(field('declarator', choice(
          seq(
            repeat(choice($.ms_call_modifier, $.macro_attribute)),
            $._declarator,
            optional($.gnu_asm_expression),
            repeat(choice($.ms_call_modifier, $.macro_attribute, $.macro_qualifier)),
          ),
          $.init_declarator,
        ))), ',')),
        field('declarator', alias($._init_declarator_preproc, $.init_declarator)),
      ),
    ),

    _init_declarator_preproc: $ => seq(
      // Non-function declarators only: a `= #if` initializer can never be a
      // function, and allowing function declarators collides with guarded
      // signatures / `= delete` (`= 0`, `= default`).
      field('declarator', choice(
        $.identifier,
        $.qualified_identifier,
        $.pointer_declarator,
        $.reference_declarator,
        $.array_declarator,
        $.parenthesized_declarator,
      )),
      repeat(choice($.ms_call_modifier, $.macro_attribute, $.macro_qualifier)),
      '=',
      field('value', choice(
        alias($.preproc_if_in_init_declarator, $.preproc_if),
        alias($.preproc_ifdef_in_init_declarator, $.preproc_ifdef),
      )),
    ),

    virtual_specifier: _ => choice(
      'final', // the only legal value here for classes
      'override', // legal for functions in addition to final, plus permutations.
    ),

    _declaration_modifiers: ($, original) => choice(
      original,
      'virtual',
    ),

    explicit_function_specifier: $ => choice(
      'explicit',
      prec(PREC.CALL, seq(
        'explicit',
        '(',
        $.expression,
        ')',
      )),
    ),

    base_class_clause: $ => seq(
      ':',
      commaSep1(seq(
        repeat($.attribute_declaration),
        optional(choice(
          $.access_specifier,
          seq($.access_specifier, optional('virtual')),
          seq('virtual', optional($.access_specifier)),
        )),
        $._class_name,
        optional('...'),
      )),
    ),

    enum_specifier: $ => prec.right(seq(
      'enum',
      choice(
        seq(
          optional(choice('class', 'struct')),
          repeat($.macro_qualifier),
          choice(
            seq(
              field('name', $._class_name),
              optional($._enum_base_clause),
              optional(field('body', $.enumerator_list)),
            ),
            field('body', $.enumerator_list),
          ),
        ),
        // An unnamed unscoped enum may still fix its underlying type: `enum : T { A };`.
        seq(
          $._enum_base_clause,
          field('body', $.enumerator_list),
        ),
      ),
      optional($.attribute_specifier),
    )),

    _enum_base_clause: $ => prec.left(seq(
      ':',
      field('base', choice(
        alias($.qualified_type_identifier, $.qualified_identifier),
        $._type_identifier,
        $.primitive_type,
        $.sized_type_specifier,
      )),
    )),

    // The `auto` storage class is removed in C++0x in order to allow for the `auto` type.
    storage_class_specifier: (_, original) => choice(
      ...original.members.filter((member) => member.value !== 'auto'),
      'thread_local',
    ),

    dependent_type: $ => prec.dynamic(-1, prec.right(seq(
      'typename',
      $.type_specifier,
    ))),

    // Declarations

    template_declaration: $ => seq(
      'template',
      field('parameters', $.template_parameter_list),
      optional($.requires_clause),
      choice(
        $._empty_declaration,
        $.alias_declaration,
        $.declaration,
        $.template_declaration,
        $.function_definition,
        $.concept_definition,
        $.friend_declaration,
        alias($.constructor_or_destructor_declaration, $.declaration),
        alias($.constructor_or_destructor_definition, $.function_definition),
        alias($.operator_cast_declaration, $.declaration),
        alias($.operator_cast_definition, $.function_definition),
      ),
    ),

    template_instantiation: $ => seq(
      optional('extern'),
      'template',
      choice(
        seq(
          optional($._declaration_specifiers),
          field('declarator', $._declarator),
        ),
        // Explicit instantiation of a class: `template class C<int>;`.
        $._declaration_specifiers,
      ),
      ';',
    ),

    template_parameter_list: $ => seq(
      '<',
      commaSep(choice(
        $.parameter_declaration,
        $.optional_parameter_declaration,
        $.type_parameter_declaration,
        $.variadic_parameter_declaration,
        $.variadic_type_parameter_declaration,
        $.optional_type_parameter_declaration,
        $.template_template_parameter_declaration,
      )),
      alias(token(prec(1, '>')), '>'),
    ),

    type_parameter_declaration: $ => prec(1, seq(
      choice('typename', 'class'),
      optional($._type_identifier),
    )),

    variadic_type_parameter_declaration: $ => prec(1, seq(
      choice('typename', 'class'),
      '...',
      optional($._type_identifier),
    )),

    optional_type_parameter_declaration: $ => seq(
      choice('typename', 'class'),
      optional(field('name', $._type_identifier)),
      '=',
      field('default_type', $.type_specifier),
    ),

    template_template_parameter_declaration: $ => seq(
      'template',
      field('parameters', $.template_parameter_list),
      choice(
        $.type_parameter_declaration,
        $.variadic_type_parameter_declaration,
        $.optional_type_parameter_declaration,
      ),
    ),

    // `f(int a #ifdef X , int b #endif , int c)` and
    // `f(int a, #ifdef X int b, #endif int c)`: the separator comma may
    // hide inside an adjacent preproc block, so a real item may follow a
    // block comma-free. Between real items commas stay strict.
    _parameter_list_real: $ => choice(
      $.parameter_declaration,
      $.optional_parameter_declaration,
      $.variadic_parameter_declaration,
      '...',
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
      optional(commaSep1($._parameter_list_item)),
      ')',
    ),

    optional_parameter_declaration: $ => seq(
      $._parameter_specifiers,
      field('declarator', optional(choice($._declarator, $._abstract_declarator, $.abstract_reference_declarator))),
      '=',
      field('default_value', choice($.expression, $.initializer_list)),
    ),

    variadic_parameter_declaration: $ => seq(
      $._parameter_specifiers,
      field('declarator', choice(
        $.variadic_declarator,
        alias($.variadic_reference_declarator, $.reference_declarator),
      )),
    ),

    variadic_declarator: $ => seq(
      '...',
      optional($.identifier),
    ),

    variadic_reference_declarator: $ => seq(
      choice('&&', '&'),
      $.variadic_declarator,
    ),

    init_declarator: ($, original) => choice(
      original,
      seq(
        field('declarator', $._declarator),
        field('value', choice(
          $.argument_list,
          $.initializer_list,
        )),
      ),
      // `T& r(v.get());` and `T* p(v.get());`: the declarator must be complete before the
      // parenthesis, or `r(v.get())` is read as a function declarator with parameters.
      seq(
        field('declarator', choice(
          alias($._init_reference_declarator, $.reference_declarator),
          alias($._init_pointer_declarator, $.pointer_declarator),
        )),
        field('value', $.argument_list),
      ),
    ),

    _init_reference_declarator: $ => seq(choice('&', '&&'), $.identifier),

    _init_pointer_declarator: $ => seq(
      '*',
      repeat($.type_qualifier),
      field('declarator', $.identifier),
    ),

    operator_cast: $ => prec.right(1, seq(
      'operator',
      $._declaration_specifiers,
      field('declarator', $._abstract_declarator),
    )),

    // Avoid ambiguity between compound statement and initializer list in a construct like:
    //   A b {};
    compound_statement: (_, original) => prec(-1, original),

    // `: a(a), b(b) #ifdef X ,c(c) #endif` and
    // `: a(a), #ifdef X c(c), #endif d(d)`: the separator comma may hide
    // inside an adjacent preproc block, so a member may follow a block
    // comma-free.
    _field_initializer_list_item: $ => choice(
      $.field_initializer,
      seq(
        $.field_initializer,
        repeat1($._field_initializer_list_preproc),
        optional(seq(optional(','), $.field_initializer)),
      ),
      seq(
        repeat1($._field_initializer_list_preproc),
        optional(seq(optional(','), $.field_initializer)),
      ),
    ),

    _field_initializer_list_preproc: $ => choice(
      alias($.preproc_if_in_field_initializer_list, $.preproc_if),
      alias($.preproc_ifdef_in_field_initializer_list, $.preproc_ifdef),
    ),

    field_initializer_list: $ => seq(
      ':',
      optional(commaSep1($._field_initializer_list_item)),
    ),

    field_initializer: $ => prec(1, seq(
      choice(
        $._field_identifier,
        $.template_method,
        alias($.qualified_field_identifier, $.qualified_identifier),
      ),
      choice($.initializer_list, $.argument_list),
      optional('...'),
    )),

    _field_declaration_list_item: ($, original) => choice(
      original,
      $.template_declaration,
      alias($.inline_method_definition, $.function_definition),
      alias($.constructor_or_destructor_definition, $.function_definition),
      alias($.constructor_or_destructor_declaration, $.declaration),
      alias($.operator_cast_definition, $.function_definition),
      alias($.operator_cast_declaration, $.declaration),
      $.friend_declaration,
      seq($.access_specifier, ':'),
      seq(optional($.access_specifier), $.qt_section_label, ':'),
      // Access-section macros that expand to `public:`/`private:`/
      // `protected:` (JSON_PRIVATE_UNLESS_TESTED, GST_PAD_STREAM_LOCK
      // style labels) look identical to a member access specifier.
      $.macro_access_label,
      // `MACRO(args) { members };` inside a class body.
      $.macro_braced_declaration,
      $.alias_declaration,
      $.using_declaration,
      $.type_definition,
      $.static_assert_declaration,
      $.macro_declaration,
      // `MACRO(TYPE) name;`/`MACRO(TYPE) name[N];` members — WOW64_POINTER
      // and friends in Win32 headers. A full alternate (not a specifier
      // variant) so the fork only opens on `IDENT (` members; putting
      // macro_type_specifier inside _declaration_specifiers regressed
      // `= delete`/`= default` methods.
      alias(seq(
        field('type', $.macro_type_specifier),
        commaSep(field('declarator', $._field_declarator)),
        ';',
      ), $.field_declaration),
      ';',
    ),

    field_declaration: $ => seq(
      repeat($.midl_attribute_list),
      $._declaration_specifiers,
      commaSep(choice(
        seq(
          field('declarator', $._field_declarator),
          optional($.bitfield_clause),
          optional(choice(
            field('default_value', $.initializer_list),
            seq('=', field('default_value', choice($.expression, $.initializer_list))),
          )),
        ),
        // An unnamed bit-field: `unsigned : 0;`.
        $.bitfield_clause,
      )),
      optional($.attribute_specifier),
      ';',
    ),

    inline_method_definition: $ => seq(
      $._declaration_specifiers,
      field('declarator', $._field_declarator),
      // Trailing attribute/override macros: `f() FLATBUFFERS_OVERRIDE {`.
      repeat(choice($.ms_call_modifier, $.macro_attribute, $.macro_qualifier)),
      choice(
        field('body', choice($.compound_statement, $.try_statement)),
        $.default_method_clause,
        $.delete_method_clause,
        $.pure_virtual_clause,
      ),
    ),

    _constructor_specifiers: $ => choice(
      $._declaration_modifiers,
      $.explicit_function_specifier,
    ),

    operator_cast_definition: $ => seq(
      repeat($._constructor_specifiers),
      field('declarator', choice(
        $.operator_cast,
        alias($.qualified_operator_cast_identifier, $.qualified_identifier),
      )),
      repeat(choice($.ms_call_modifier, $.macro_attribute, $.macro_qualifier)),
      field('body', choice($.compound_statement, $.try_statement)),
    ),

    operator_cast_declaration: $ => prec(1, seq(
      repeat($._constructor_specifiers),
      field('declarator', choice(
        $.operator_cast,
        alias($.qualified_operator_cast_identifier, $.qualified_identifier),
      )),
      optional(seq('=', field('default_value', $.expression))),
      ';',
    )),

    constructor_try_statement: $ => seq(
      'try',
      optional($.field_initializer_list),
      field('body', $.compound_statement),
      repeat1($.catch_clause),
    ),

    constructor_or_destructor_definition: $ => seq(
      repeat($._constructor_specifiers),
      field('declarator', $.function_declarator),
      repeat(choice($.ms_call_modifier, $.macro_attribute, $.macro_qualifier)),
      choice(
        seq(
          optional($.field_initializer_list),
          repeat(choice($.ms_call_modifier, $.macro_attribute, $.macro_qualifier)),
          field('body', $.compound_statement),
        ),
        alias($.constructor_try_statement, $.try_statement),
        $.default_method_clause,
        $.delete_method_clause,
        $.pure_virtual_clause,
      ),
    ),

    constructor_or_destructor_declaration: $ => seq(
      repeat($._constructor_specifiers),
      field('declarator', $.function_declarator),
      ';',
    ),

    default_method_clause: _ => seq('=', 'default', ';'),
    delete_method_clause: _ => seq('=', 'delete', ';'),
    pure_virtual_clause: _ => seq('=', /0/, ';'),

    friend_declaration: $ => seq(
      optional('constexpr'),
      'friend',
      choice(
        $.declaration,
        $.function_definition,
        seq(
          optional(choice(
            'class',
            'struct',
            'union',
          )),
          $._class_name, ';',
        ),
      ),
    ),

    access_specifier: _ => choice(
      'public',
      'private',
      'protected',
    ),

    // Qt section labels: `signals:`, `public slots:`, `Q_SIGNALS:`.
    macro_access_label: $ => seq(
      field('name', $.identifier),
      ':',
    ),

    qt_section_label: _ => choice(
      'signals',
      'slots',
      'Q_SIGNALS',
      'Q_SLOTS',
    ),

    // A bare macro name that expands to declarations: Qt `Q_OBJECT`,
    // `QT_BEGIN_NAMESPACE`, Unreal `GENERATED_BODY()`, Qt6 `QML_ELEMENT`.
    // Literal list — a lone identifier is never a valid member or
    // top-level item, but a regex would shadow real identifiers.
    macro_declaration: $ => prec.right(choice(
      seq(
      $._macro_declaration_literals,
      optional($.macro_argument_list),
      ),
      // Generic fallback for macro names outside the literal list
      // (serialize/test/register macros, vendor markers). Dynamic +2:
      // real named items still win, but this must beat the
      // abstract-declarator reinterpretation of `NAME(&)`/`NAME(^)`
      // and error-tolerant `X == Y(...)` binary expressions — inside
      // declaration_list a bare NAME(args) call is far more often a
      // declaration macro than an unnamed-reference declaration.
      prec.dynamic(-1, seq(
        $.identifier,
        optional($.macro_argument_list),
        optional(';'),
      )),
    )),

    // `NAME(args)`/`NAME(args);`/`NAME;` for `_top_level_item` and
    // `_block_item`; literal names keep their bare form. Aliased back to
    // `macro_declaration` at use sites.
    _macro_declaration_top: $ => prec.right(choice(
      seq(
      $._macro_declaration_literals,
      optional($.macro_argument_list),
      ),
      // No dynamic precedence here: the alias wrappers at the
      // `_top_level_item`/`_block_item` use sites supply dynamic(-2), and
      // an inner dynamic score would compete with it unpredictably.
      // The identifier may stand alone: namespace-macro sentinels like
      // `NLOHMANN_JSON_NAMESPACE_END` carry no argument list and no `;`.
      // Without this arm the bare name becomes an unterminated
      // expression_statement that swallows the following preproc block.
      // The extra dynamic(-1) keeps `ASIO_DECL int f()` on the
      // declaration side — a bare qualifier identifier must lose to
      // macro_qualifier + declaration whenever that fork exists.
      prec.dynamic(-1, seq(
        $.identifier,
        optional($.macro_argument_list),
        optional(';'),
      )),
    )),

    // `NS_BEGIN_MACRO ... NS_END_MACRO` pairs — the nlohmann/abseil/Qt
    // idiom where a namespace's braces hide inside two macros. Without
    // this rule the phantom-open brace leaves `}` MISSING errors at
    // every real close-brace downstream. Names are literal because the
    // END marker is what makes the pair safe to swallow.
    macro_namespace: $ => prec.dynamic(10, seq(
      choice(
        'NLOHMANN_JSON_NAMESPACE_BEGIN',
        'QT_BEGIN_NAMESPACE',
        'ABSL_NAMESPACE_BEGIN',
        'PROTOBUF_NAMESPACE_OPEN',
        'GTEST_NAMESPACE_BEGIN',
      ),
      repeat($._top_level_item),
      choice(
        'NLOHMANN_JSON_NAMESPACE_END',
        'QT_END_NAMESPACE',
        'ABSL_NAMESPACE_END',
        'PROTOBUF_NAMESPACE_CLOSE',
        'GTEST_NAMESPACE_END',
      ),
    )),

    // `#if`/`#else`-split function signatures: each branch holds a
    // signature (specifiers + declarator, no `;`) and the shared `{`
    // body follows `#endif`. json.hpp gates `operator<=>`/`operator<`
    // this way; the MISSING `;` path used to ERROR the `#if` arm and
    // orphan the body into a naked compound_statement.
    guarded_function: $ => prec.dynamic(10, seq(
      choice(
        $.preproc_if_in_guarded_signature,
        $.preproc_ifdef_in_guarded_signature,
      ),
      $.compound_statement,
    )),

    _guarded_signature: $ => seq(
      $._declaration_specifiers,
      $._declarator,
    ),

    ...preprocIf('_in_guarded_signature', $ => $._guarded_signature),

    // `#if`/`#else`-split class heads: kddockwidgets gates the base list of
    // `class DOCKS_EXPORT DockWidget` this way — each branch carries a head
    // (keyword + name + optional bases, no body) and the shared
    // field_declaration_list follows `#endif`.
    guarded_class: $ => prec.dynamic(10, seq(
      choice(
        $.preproc_if_in_guarded_class,
        $.preproc_ifdef_in_guarded_class,
      ),
      $.field_declaration_list,
      ';',
    )),

    _guarded_class_head: $ => prec.right(seq(
      choice('class', 'struct', 'union'),
      repeat(choice($.attribute_specifier, $.alignas_qualifier)),
      optional($.ms_declspec_modifier),
      repeat($.attribute_declaration),
      repeat(choice($.macro_attribute, $.macro_qualifier)),
      field('name', $._class_name),
      optional($.virtual_specifier),
      optional($.base_class_clause),
    )),

    ...preprocIf('_in_guarded_class', $ => $._guarded_class_head),

    // The inherited un-suffixed `#if`/`#ifdef` arms take `_block_item`,
    // which deliberately lacks the loose braced decl — so a guarded
    // `WIDGET() { int bits: 3; };` errored even at file scope. Adding
    // it here (child rules shadow the parent's same-named rules) keeps
    // unguarded statement bodies on `macro_braced_statement` while
    // guarded positions gain the member-form read.
    ...preprocIf('', $ => choice(
      $._block_item,
      alias($._macro_braced_declaration_loose, $.macro_braced_declaration),
      alias(prec.dynamic(-2, prec.right($._macro_declaration_top)), $.macro_declaration),
      $.dangling_else,
      $.dangling_label,
      '__extension__',
    )),

    _macro_declaration_literals: $ => choice(
      'Q_OBJECT',
      'Q_GADGET',
      'Q_NAMESPACE',
      'QML_ELEMENT',
      'QML_ANONYMOUS',
      'QML_SINGLETON',
      'QML_INTERFACE',
      'QML_UNAVAILABLE',
      'QML_STRUCTURED_VALUE',
      'QML_CONSTRUCTIBLE_VALUE',
      'QT_BEGIN_NAMESPACE',
      'QT_END_NAMESPACE',
      'QT_BEGIN_INCLUDE_NAMESPACE',
      'QT_END_INCLUDE_NAMESPACE',
      'GENERATED_BODY',
      'GENERATED_UCLASS_BODY',
      'GENERATED_USTRUCT_BODY',
      'GENERATED_UINTERFACE_BODY',
      'GENERATED_IINTERFACE_BODY',
      'Q_PROPERTY',
      'Q_ENUM',
      'Q_ENUM_NS',
      'Q_FLAG',
      'Q_FLAG_NS',
      'Q_ENUMS',
      'Q_FLAGS',
      'Q_CLASSINFO',
      'Q_INTERFACES',
      'Q_PLUGIN_METADATA',
      'Q_DISABLE_COPY',
      'Q_DISABLE_MOVE',
      'Q_DISABLE_COPY_MOVE',
      'QDOC_PROPERTY',
      'Q_GLOBAL_STATIC',
      'Q_GLOBAL_STATIC_WITH_ARGS',
      'Q_COREAPP_STARTUP_FUNC',
      'Q_IMPORT_PLUGIN',
      'Q_DECLARE_METATYPE',
      'Q_DECLARE_TYPEINFO',
      'Q_DECLARE_FLAGS',
      'Q_DECLARE_OPERATORS_FOR_FLAGS',
      'Q_DECLARE_INTERFACE',
      'Q_DECLARE_TR_FUNCTIONS',
      'QML_NAMED_ELEMENT',
      'QML_ATTACHED',
      'QML_EXTENDED',
      'QML_FOREIGN',
      'QML_VALUE_TYPE',
      'QML_UNCREATABLE',
      'QML_ADDED_IN_VERSION',
      'QML_REMOVED_IN_VERSION',
      'QML_FOREIGN_NAMESPACE',
      ),

    // Macro arguments that are not expressions: `Q_PROPERTY(QString title
    // READ title WRITE setTitle NOTIFY titleChanged)` mixes types, member
    // names, and DSL keywords as bare words.
    macro_argument_list: $ => seq(
      '(',
      repeat(choice(
        $.identifier,
        $.primitive_type,
        $.sized_type_specifier,
        $.template_type,
        $.qualified_identifier,
        $.number_literal,
        $.pointer_expression,
        $.unary_expression,
        $.sizeof_expression,
        $.string_literal,
        $.char_literal,
        ',',
        '::',
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
        '->*',
        '.*',
        '[',
        ']',
        // Brace-init inside macro args: `M(name, T{})` / `M({a, b})`.
        '{',
        '}',
        'operator',
        'const',
        'noexcept',
        $.macro_argument_list,
      )),
      ')',
    ),

    // Qt `emit sig(...)` / `Q_EMIT sig(...)`: emit is a no-op marker macro.
    // `emit` is a literal, so it is a keyword token: a statement that
    // starts with an `emit` variable (`emit = 0;`) no longer parses —
    // the same trade-off as macro_attribute's literals, and Qt reserves
    // `emit` when keywords are enabled.
    emit_statement: $ => prec(-1, seq(
      field('name', choice('emit', 'Q_EMIT')),
      $.expression_statement,
    )),

    qt_foreach_statement: $ => seq(
      choice('Q_FOREACH', 'BOOST_FOREACH', 'foreach'),
      '(',
      choice(
        seq(
          $._declaration_specifiers,
          field('declarator', $._declarator),
        ),
        $.qt_foreach_assignment,
      ),
      ',',
      field('right', choice($.expression, $.initializer_list)),
      ')',
      field('body', $.qt_foreach_body),
    ),

    qt_foreach_assignment: $ => field('left', $.expression),

    // The iterator binding begins after the container expression is evaluated.
    qt_foreach_body: $ => $.statement,

    // Qt statement macros are conventionally written without a trailing
    // semicolon (Q_UNUSED(parent) before the next statement). The listed
    // literals are keyword tokens: real code that calls a
    // function named Q_UNUSED et al. now parses as this rule — the same
    // trade-off as macro_attribute's literals.
    qt_statement_macro: $ => prec.right(seq(
      field('name', choice(
        'Q_UNUSED', 'Q_ASSERT', 'Q_ASSERT_X', 'Q_VERIFY', 'Q_VERIFY_X',
        'Q_CHECK_PTR', 'Q_UNREACHABLE', 'Q_FALLTHROUGH',
        'QT_REQUIRE_VERSION',
      )),
      $.macro_argument_list,
      optional(';'),
    )),

    // GCC `asm(...)` with `::`-fused operand sections. In C++ `::` is
    // the scope-resolution token, so `"" ::: "memory"` lexes `::` + `:`:
    // the classic nested lists (which need a `:` per section) never see
    // three colons and fail — `asm(code : out : in : clob)` with an
    // empty section is the atmosphere/ACO idiom. The fused path keeps
    // the real operand node kinds in generic sections: a `::` closes an
    // empty section AND opens the next, so which list an operand
    // belongs to is not statically recoverable. Only reachable when a
    // `::` token is present, so the classic path stays unambiguous.
    gnu_asm_expression: $ => prec(PREC.CALL, seq(
      choice('asm', '__asm__', '__asm', '__ASM'),
      repeat($.gnu_asm_qualifier),
      '(',
      field('assembly_code', $._string),
      optional(choice(
        seq(
          field('output_operands', $.gnu_asm_output_operand_list),
          optional(seq(
            field('input_operands', $.gnu_asm_input_operand_list),
            optional(seq(
              field('clobbers', $.gnu_asm_clobber_list),
              optional(field('goto_labels', $.gnu_asm_goto_list)),
            )),
          )),
        ),
        $._gnu_asm_fused_sections,
      )),
      ')',
    )),

    // Which section a fused list lands in depends on the separators
    // that FOLLOW it, so middle sections take the operand|string union
    // while the trailing goto slot is typed strictly (bare identifiers
    // only, emitted as `gnu_asm_goto_list` like the structured branch).
    // Separators are enumerated, so a fifth colon can't parse.
    _gnu_asm_fused_sections: $ => prec.dynamic(-1, choice(
      // `::`-led (outputs empty): `x :: inputs`, `x :: inputs : clobbers`,
      // `x :: inputs : clobbers : goto`, `x ::: clobbers`,
      // `x :: clobbers :: goto` (`::::`).
      seq('::', optional(field('input_operands', alias($._fused_asm_items, $.gnu_asm_input_operand_list))),
        optional(choice(
          seq(':', optional(field('clobbers', alias($._fused_asm_clobber_items, $.gnu_asm_clobber_list))),
            optional(seq(
              ':',
              optional(field('goto_labels', $._gnu_asm_goto_items)),
            ))),
          seq('::', optional(field('goto_labels', $._gnu_asm_goto_items))),
        ))),
      // `x : outputs :: clobbers [: goto]` (inputs empty).
      seq(
        field('output_operands', $.gnu_asm_output_operand_list),
        '::',
        optional(field('clobbers', alias($._fused_asm_clobber_items, $.gnu_asm_clobber_list))),
        optional(seq(':', optional(field('goto_labels', $._gnu_asm_goto_items)))),
      ),
      // `x : outputs : inputs :: goto` (clobbers empty).
      seq(
        field('output_operands', $.gnu_asm_output_operand_list),
        field('input_operands', $.gnu_asm_input_operand_list),
        '::',
        optional(field('goto_labels', $._gnu_asm_goto_items)),
      ),
    )),

    // Middle-section items in fused sections: `"c"(expr)` operands or
    // clobber strings — `gnu_asm_clobber_list` takes `_string`, but the
    // full `concatenated_string` also allows `call_expression` members
    // whose `::name(...)` callee would collide with the `::` section
    // separator — so fused items take a concat variant with no call
    // members. A bare identifier can't live here — only goto labels take
    // those — so `asm("x" :: ident)` fails like GCC.
    _fused_asm_items: $ => commaSep1(field('operand', choice(
      prec.dynamic(1, $.gnu_asm_input_operand),
      $._fused_asm_string,
    ))),

    // Clobbers emit `register:` fields like upstream
    // `gnu_asm_clobber_list`, so queries find them in fused sections.
    _fused_asm_clobber_items: $ => commaSep1(field('register', $._fused_asm_string)),

    _fused_asm_string: $ => choice(
      $.string_literal,
      $.raw_string_literal,
      alias($._fused_asm_concat_string, $.concatenated_string),
    ),
    _fused_asm_concat_string: $ => prec.right(seq(
      choice($.identifier, $.string_literal, $.raw_string_literal),
      choice($.identifier, $.string_literal, $.raw_string_literal),
      repeat(choice($.identifier, $.string_literal, $.raw_string_literal)),
    )),

    // Goto labels in fused sections; aliased so emitted nodes keep the
    // `gnu_asm_goto_list` name queries match.
    _gnu_asm_goto_items: $ => alias(
      $._gnu_asm_goto_list_body,
      $.gnu_asm_goto_list,
    ),
    _gnu_asm_goto_list_body: $ => commaSep1(field('label', $.identifier)),

    macro_attribute: ($, original) => choice(
      original,
      // Qt declaration modifiers (moc-expanded): seen in class members and
      // free declarations throughout Qt-based code.
      'Q_INVOKABLE',
      'Q_SIGNAL',
      'Q_SLOT',
      'Q_DECL_OVERRIDE',
      'Q_DECL_FINAL',
      'Q_DECL_CONSTEXPR',
      'Q_DECL_RELAXED_CONSTEXPR',
      'Q_DECL_DEPRECATED',
      'Q_DECL_NOTHROW',
      'Q_REQUIRED_RESULT',
      'Q_NORETURN',
      'Q_CONSTEXPR',
      'Q_ALWAYS_INLINE',
      'Q_NEVER_INLINE',
      'Q_PACKED',
      'Q_DECL_UNUSED',
      'Q_DECL_EXPORT',
      'Q_DECL_IMPORT',
      seq('Q_REVISION', $.argument_list),
      seq('Q_DECL_DEPRECATED_X', $.argument_list),
    ),

    _declarator: ($, original) => choice(
      original,
      $.reference_declarator,
      $.qualified_identifier,
      $.template_function,
      $.operator_name,
      $.destructor_name,
      $.structured_binding_declarator,
    ),

    _field_declarator: ($, original) => choice(
      original,
      alias($.reference_field_declarator, $.reference_declarator),
      $.template_method,
      $.operator_name,
    ),

    _type_declarator: ($, original) => choice(
      original,
      alias($.reference_type_declarator, $.reference_declarator),
      // `typedef void (dummy::*sb)()` — scoped member pointers inside
      // parenthesized type declarators (inherited `parenthesized_type_declarator`
      // wraps `_type_declarator`, which upstream leaves without
      // qualified_identifier).
      $.qualified_identifier,
    ),

    _abstract_declarator: ($, original) => choice(
      original,
      $.abstract_reference_declarator,
    ),

    // Member pointers in type position (`R (C::*)(Args...)`): the C base has
    // no scope prefix on `*`, so `C::*` fell into call-expression reads with a
    // MISSING type. The scope-prefixed alternative lives only inside
    // parenthesized declarators — not on `_abstract_declarator` itself —
    // because a scope-starting abstract declarator makes `delete [] p`
    // ambiguous with a lambda capture. `A::B::*` and global `::*` both cover;
    // named member-pointer variables (`int C::* p`) are the accepted loss.
    abstract_parenthesized_declarator: $ => prec(1, seq(
      '(',
      repeat(choice($.ms_call_modifier, $.macro_attribute)),
      choice(
        $._abstract_declarator,
        seq(
          repeat1($._scope_resolution),
          '*',
          repeat(choice(
            $.ms_pointer_modifier,
            $.type_qualifier,
            $.ms_call_modifier,
            $.far_ptr_modifier,
            $.macro_attribute,
          )),
          optional($._abstract_declarator),
        ),
      ),
      ')',
    )),

    reference_declarator: $ => prec.dynamic(1, prec.right(seq(choice('&', '&&'), $._declarator))),
    reference_field_declarator: $ => prec.dynamic(1, prec.right(seq(choice('&', '&&'), $._field_declarator))),
    reference_type_declarator: $ => prec.dynamic(1, prec.right(seq(choice('&', '&&'), $._type_declarator))),
    abstract_reference_declarator: $ => prec.right(seq(choice('&', '&&'), optional($._abstract_declarator))),

    structured_binding_declarator: $ => prec.dynamic(PREC.STRUCTURED_BINDING, seq(
      '[', commaSep1($.identifier), ']',
    )),

    ref_qualifier: _ => choice('&', '&&'),

    _function_declarator_seq: $ => seq(
      field('parameters', $.parameter_list),
      optional($._function_attributes_start),
      optional($.ref_qualifier),
      optional($._function_exception_specification),
      optional($._function_attributes_end),
      optional($.trailing_return_type),
      optional($._function_postfix),
    ),

    _function_attributes_start: $ => prec(1, choice(
      seq(repeat1($.attribute_specifier), repeat($.type_qualifier)),
      seq(repeat($.attribute_specifier), repeat1($.type_qualifier)),
    )),

    _function_exception_specification: $ => choice(
      $.noexcept,
      $.throw_specifier,
    ),

    _function_attributes_end: $ => prec.right(seq(
      optional($.gnu_asm_expression),
      choice(
        seq(repeat1($.attribute_specifier), repeat($.attribute_declaration)),
        seq(repeat($.attribute_specifier), repeat1($.attribute_declaration)),
      ),
    )),

    // Post-declarator qualifiers are bare names only (`f() Q_DECL_OVERRIDE`).
    // An arg-carrying qualifier (`f() ATTR(x)`) makes consecutive
    // `NAME(args)` lines read as a function declarator + attributes,
    // which starves the macro-declaration read — bare names keep both.
    // Uses the base grammar's `_macro_qualifier_bare` (dynamic -1), so real
    // type/declarator reads still win — do not redefine it here without
    // the negative precedence.
    _function_postfix: $ => prec.right(choice(
      repeat1(choice($.virtual_specifier, $.macro_attribute, $._macro_qualifier_bare)),
      $.requires_clause,
    )),

    function_declarator: $ => prec.dynamic(1, seq(
      field('declarator', $._declarator),
      $._function_declarator_seq,
    )),

    // The C base's function_type_declarator (aliased to function_declarator)
    // ends at the parameter list — typedef'd function types may carry
    // cv/ref/noexcept qualifiers (`typedef void (C::*f)() const;`).
    function_type_declarator: $ => prec(1, seq(
      field('declarator', $._type_declarator),
      $._function_declarator_seq,
    )),

    function_field_declarator: $ => prec.dynamic(1, seq(
      field('declarator', $._field_declarator),
      $._function_declarator_seq,
    )),

    abstract_function_declarator: $ => seq(
      field('declarator', optional($._abstract_declarator)),
      $._function_declarator_seq,
    ),

    trailing_return_type: $ => seq('->', $.type_descriptor),

    noexcept: $ => prec.right(seq(
      'noexcept',
      optional(
        seq(
          '(',
          optional($.expression),
          ')',
        ),
      ),
    )),

    throw_specifier: $ => seq(
      'throw',
      seq(
        '(',
        commaSep($.type_descriptor),
        ')',
      ),
    ),

    template_type: $ => seq(
      field('name', $._type_identifier),
      field('arguments', $.template_argument_list),
    ),

    template_method: $ => seq(
      field('name', choice($._field_identifier, $.operator_name)),
      field('arguments', $.template_argument_list),
    ),

    template_function: $ => seq(
      field('name', $.identifier),
      field('arguments', $.template_argument_list),
    ),

    _template_argument: $ => choice(
      prec.dynamic(3, $.type_descriptor),
      prec.dynamic(2, alias($.type_parameter_pack_expansion, $.parameter_pack_expansion)),
      prec.dynamic(1, $.expression),
    ),

    template_argument_list: $ => seq(
      '<',
      choice(
        seq(
          commaSep($._template_argument),
          alias(token(prec(1, '>')), '>'),
        ),
        // `token(prec(1, '>'))` splits `>=` into `>` `=` even inside template
        // arguments (`enable_if_t<N >= 1>`). Re-join the split tokens in the
        // final argument: `token.immediate('=')` restricts the rejoin to a
        // literally adjacent `>=` (a spaced `> =` keeps the close read).
        seq(
          optional(seq(commaSep($._template_argument), ',')),
          $._template_relational_expression,
          alias(token(prec(1, '>')), '>'),
        ),
      ),
    ),

    // `a >= b` re-joined from the lexed `>` `=` split. The operand must
    // bind tighter than `>=` (C++ parses `(a >= b) op c`, never
    // `a >= (b op c)` for a looser `op`), and it carries real
    // left/operator/right fields so the emitted binary_expression is
    // structurally identical to one a normal expression produces.
    _template_ge_operator: $ => seq(
      token(prec(1, '>')),
      token.immediate('='),
    ),

    _template_relational_argument: $ => seq(
      field('left', $.expression),
      field('operator', alias($._template_ge_operator, '>=')),
      field('right', $._template_relational_operand),
    ),

    // Left-recursive continuation so `W<a >= b && c>` groups as
    // `(a >= b) && c` like C++, and a `?:`/`=` tail keeps its C++ shape.
    // The variants stay hidden; the alias at each reference site names the
    // node (alias cannot wrap a seq), so the argument takes its real
    // expression kind with no wrapper.
    _template_relational_binary: $ => prec.left(PREC.LOGICAL_OR, seq(
      field('left', $._template_relational_expression),
      field('operator', choice(
        '||', '&&', '|', '^', '&', '==', '!=', '<=', '<', '<=>',
        'or', 'and', 'bitor', 'xor', 'bitand', 'not_eq',
      )),
      field('right', $.expression),
    )),

    _template_relational_conditional: $ => seq(
      field('condition', $._template_relational_expression),
      '?',
      field('consequence', $.expression),
      ':',
      field('alternative', $.expression),
    ),

    _template_relational_assignment: $ => prec.right(PREC.ASSIGNMENT, seq(
      field('left', $._template_relational_expression),
      field('operator', choice('=', '+=', '-=', '*=', '/=', '%=', '<<=', '>>=', '&=', '|=', '^=')),
      field('right', $.expression),
    )),

    _template_relational_expression: $ => choice(
      alias($._template_relational_argument, $.binary_expression),
      alias($._template_relational_binary, $.binary_expression),
      alias($._template_relational_conditional, $.conditional_expression),
      alias($._template_relational_assignment, $.assignment_expression),
    ),

    // Operand of a re-joined `>=`: unary-or-tighter expressions and binary
    // chains whose operators all outrank relational (`* / % + - << >>`).
    // Looser ops (`&&`, `==`, `?:`, ...) continue through
    // _template_relational_expression instead.
    _template_relational_operand: $ => choice(
      $.identifier,
      $.qualified_identifier,
      $.number_literal,
      $.char_literal,
      $.string_literal,
      $.concatenated_string,
      $.raw_string_literal,
      $.true,
      $.false,
      $.null,
      $.call_expression,
      $.unary_expression,
      $.update_expression,
      $.pointer_expression,
      $.cast_expression,
      $.sizeof_expression,
      $.field_expression,
      $.subscript_expression,
      $.parenthesized_expression,
      $.template_function,
      $.template_method,
      $.new_expression,
      $.delete_expression,
      $.co_await_expression,
      $.requires_expression,
      $.fold_expression,
      $.user_defined_literal,
      alias($._template_tight_binary, $.binary_expression),
    ),

    _template_tight_binary: $ => prec.left(PREC.SHIFT, seq(
      field('left', $.expression),
      field('operator', choice('*', '/', '%', '+', '-', '<<', '>>')),
      field('right', $.expression),
    )),

    namespace_definition: $ => seq(
      optional('inline'),
      'namespace',
      optional($.attribute_declaration),
      field('name', optional(
        choice(
          $._namespace_identifier,
          $.nested_namespace_specifier,
        ))),
      field('body', $.declaration_list),
    ),

    namespace_alias_definition: $ => seq(
      'namespace',
      field('name', $._namespace_identifier),
      '=',
      choice(
        $._namespace_identifier,
        $.nested_namespace_specifier,
      ),
      ';',
    ),

    _namespace_specifier: $ => seq(
      optional('inline'),
      $._namespace_identifier,
    ),

    nested_namespace_specifier: $ => prec(1, seq(
      optional($._namespace_specifier),
      '::',
      choice(
        $.nested_namespace_specifier,
        $._namespace_specifier,
      ),
    )),

    using_declaration: $ => seq(
      'using',
      optional(choice('namespace', 'enum')),
      choice(
        $.identifier,
        $.qualified_identifier,
      ),
      ';',
    ),

    alias_declaration: $ => seq(
      'using',
      field('name', $._type_identifier),
      repeat($.attribute_declaration),
      '=',
      field('type', $.type_descriptor),
      ';',
    ),

    static_assert_declaration: $ => seq(
      'static_assert',
      '(',
      field('condition', $.expression),
      optional(seq(
        ',',
        field('message', $._string),
      )),
      ')',
      ';',
    ),

    concept_definition: $ => seq(
      'concept',
      field('name', $.identifier),
      '=',
      $.expression,
      ';',
    ),

    // `NAME { stmts };` — statement macros whose expansion carries the
    // braced block (`ON_SCOPE_EXIT { fs::CloseFile(f); };`,
    // atmosphere/folly). Dynamic -1 keeps the fork below the
    // braced-temporary read: `erase_fixture{}.m()`, `OStream{&buf} << x`
    // and `alloc_checker{0}, f(x)` all stay compound literals because
    // their braces hold no `;`-terminated block items, while real macro
    // bodies (`{ f(); }`) can only parse here. The block must be
    // non-empty.
    macro_braced_statement: $ => prec.right(seq(
      // Macro names are conventionally ALL_CAPS, which lex as
      // `type_identifier` — both must open the block. `R_TRY_CATCH(e) {`
      // shows the argument list is attested at statement level too.
      field('name', choice($.identifier, alias($._type_identifier, $.identifier))),
      optional($.macro_argument_list),
      alias($._macro_braced_body, $.compound_statement),
      optional(';'),
    )),

    // Declaration-level braced macros (`MACRO(args) { members };`,
    // mesosphere's accessor classes). The body allows statements as
    // well as declarations — `SCOPE_GUARD(lock) { ReleaseSpinLock(x); };`
    // keeps its call visible instead of reading as a phantom member.
    macro_braced_declaration: $ => prec.dynamic(1, prec.right(seq(
      field('name', choice($.identifier, alias($._type_identifier, $.identifier))),
      optional(alias($._macro_argument_list_nonempty, $.macro_argument_list)),
      alias($._macro_braced_members, $.field_declaration_list),
      optional(';'),
    ))),

    // Same shape with the base (empty-matching) `macro_argument_list` —
    // used at file/namespace/statement scope, where `NAME() {}` can
    // never be a constructor, so empty `()` is unambiguously a macro.
    _macro_braced_declaration_loose: $ => prec.dynamic(1, prec.right(seq(
      field('name', choice($.identifier, alias($._type_identifier, $.identifier))),
      optional($.macro_argument_list),
      alias($._macro_braced_members, $.field_declaration_list),
      optional(';'),
    ))),

    // Statement-scope braced decl macros (`WIDGET() { int bits : 3; };`
    // inside a function body). Parens are required — bare `NAME {` stays
    // with `macro_braced_statement`, which is the only read that can
    // hold `ON_SCOPE_EXIT { cleanup(); };` (its block is statements).
    _macro_braced_declaration_paren: $ => prec.dynamic(1, prec.right(seq(
      field('name', choice($.identifier, alias($._type_identifier, $.identifier))),
      $.macro_argument_list,
      alias($._macro_braced_members, $.field_declaration_list),
      optional(';'),
    ))),

    // The base `macro_argument_list` also matches `()` — with empty
    // parens the braced read would steal `S() { this->x = 1; };`
    // constructors, so the declaration form demands ≥1 argument.
    _macro_argument_list_nonempty: $ => seq(
      '(',
      repeat1(choice(
        $.identifier,
        $.primitive_type,
        $.sized_type_specifier,
        $.template_type,
        $.qualified_identifier,
        $.number_literal,
        $.pointer_expression,
        $.unary_expression,
        $.sizeof_expression,
        $.string_literal,
        $.char_literal,
        ',',
        '::',
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
        '->*',
        '.*',
        '[',
        ']',
        '{',
        '}',
        'operator',
        'const',
        'noexcept',
        $.macro_argument_list,
      )),
      ')',
    ),

    // `auto g = SCOPE_GUARD { stmts };` — the braced block sits in
    // expression position. `_expression` lives in the C base, so it's
    // overridden wholesale like `_top_level_item`.
    macro_braced_expression: $ => prec.right(seq(
      field('name', choice($.identifier, alias($._type_identifier, $.identifier))),
      optional($.macro_argument_list),
      alias($._macro_braced_body, $.compound_statement),
    )),

    expression: ($, original) => choice(
      ...original.members,
      $.macro_braced_expression,
    ),

    // The `{` is an external token that only lexes when the brace
    // region contains a `;` — braced temporaries (`Type{arg}`) never
    // do, so they always take the regular `{`.
    _macro_braced_body: $ => seq(alias($._macro_braced_open, '{'), repeat1($._block_item), '}'),

    // Member-only forms that `_block_item` lacks: bit-fields
    // (`int x : 3;`), `friend class T;`, `private:` access labels.
    // `f(x);` still reads as a call — `field_declaration` requires a
    // type specifier and `constructor_or_destructor_declaration`
    // (which would phantom-read it) is deliberately not offered.
    _macro_braced_members: $ => seq(
      alias($._macro_braced_open, '{'),
      repeat(choice(
        $._block_item,
        $.field_declaration,
        $.friend_declaration,
        seq($.access_specifier, ':'),
        seq(optional($.access_specifier), $.qt_section_label, ':'),
        $.macro_access_label,
        $.type_definition,
      )),
      '}',
    ),

    // Statements

    _top_level_statement: ($, original) => choice(
      original,
      $.co_return_statement,
      $.co_yield_statement,
      $.for_range_loop,
      $.try_statement,
      $.throw_statement,
      $.macro_braced_statement,
    ),

    _non_case_statement: ($, original) => choice(
      original,
      $.co_return_statement,
      $.co_yield_statement,
      $.for_range_loop,
      $.try_statement,
      $.throw_statement,
      $.emit_statement,
      $.qt_statement_macro,
      $.qt_foreach_statement,
      $.macro_braced_statement,
      alias($._macro_braced_declaration_paren, $.macro_braced_declaration),
    ),

    switch_statement: $ => seq(
      'switch',
      field('condition', $.condition_clause),
      field('body', $.compound_statement),
    ),

    while_statement: $ => seq(
      'while',
      field('condition', $.condition_clause),
      field('body', $._statement_or_preproc),
    ),

    // `_statement_or_preproc` (base): the body slot also accepts a whole
    // `#if`/`#ifdef` group, e.g. `if (x) #ifdef MAC a(); #else b(); #endif`.
    if_statement: $ => prec.right(seq(
      'if',
      optional('constexpr'),
      field('condition', $.condition_clause),
      field('consequence', $._statement_or_preproc),
      optional(field('alternative', $.else_clause)),
    )),
    dangling_else: $ => prec.right(seq(
      'if',
      optional('constexpr'),
      field('condition', $.condition_clause),
      field('consequence', $._statement_or_preproc),
      'else',
      optional($.dangling_else),
    )),


    // Using prec(1) instead of prec.dynamic(1) causes issues with the
    // range loop's declaration specifiers if `int` is passed in, it'll
    // always prefer the standard for loop and give us a parse error.
    _for_statement_body: ($, original) => prec.dynamic(1, original),
    for_range_loop: $ => seq(
      'for',
      '(',
      $._for_range_loop_body,
      ')',
      field('body', $._statement_or_preproc),
    ),
    _for_range_loop_body: $ => seq(
      field('initializer', optional($.init_statement)),
      $._declaration_specifiers,
      field('declarator', $._declarator),
      ':',
      field('right', choice(
        $.expression,
        $.initializer_list,
      )),
    ),

    init_statement: $ => choice(
      $.alias_declaration,
      $.type_definition,
      $.declaration,
      $.expression_statement,
    ),

    condition_clause: $ => seq(
      '(',
      field('initializer', optional($.init_statement)),
      field('value', choice(
        $.expression,
        $.comma_expression,
        alias($.condition_declaration, $.declaration),
      )),
      ')',
    ),

    condition_declaration: $ => seq(
      $._declaration_specifiers,
      field('declarator', $._declarator),
      choice(
        seq(
          '=',
          field('value', $.expression),
        ),
        field('value', $.initializer_list),
      ),
    ),

    return_statement: ($, original) => seq(
      choice(
        original,
        seq('return', $.initializer_list, ';'),
      ),
    ),

    co_return_statement: $ => seq(
      'co_return',
      optional($.expression),
      ';',
    ),

    co_yield_statement: $ => seq(
      'co_yield',
      $.expression,
      ';',
    ),

    throw_statement: $ => seq(
      'throw',
      optional($.expression),
      ';',
    ),

    try_statement: $ => seq(
      'try',
      field('body', $.compound_statement),
      repeat1($.catch_clause),
    ),

    catch_clause: $ => seq(
      'catch',
      field('parameters', $.parameter_list),
      field('body', $.compound_statement),
    ),

    // Expressions

    _expression_not_binary: ($, original) => choice(
      original,
      $.co_await_expression,
      $.requires_expression,
      $.requires_clause,
      $.template_function,
      $.qualified_identifier,
      $.new_expression,
      $.delete_expression,
      $.lambda_expression,
      $.parameter_pack_expansion,
      $.this,
      $.user_defined_literal,
      $.fold_expression,
    ),

    _string: $ => choice(
      $.string_literal,
      $.raw_string_literal,
      $.concatenated_string,
    ),

    raw_string_literal: $ => seq(
      choice('R"', 'LR"', 'uR"', 'UR"', 'u8R"'),
      choice(
        seq(
          field('delimiter', $.raw_string_delimiter),
          '(',
          $.raw_string_content,
          ')',
          $.raw_string_delimiter,
        ),
        seq('(', $.raw_string_content, ')'),
      ),
      '"',
    ),

    subscript_expression: $ => prec(PREC.SUBSCRIPT, seq(
      field('argument', $.expression),
      field('indices', $.subscript_argument_list),
    )),

    subscript_argument_list: $ => seq(
      '[',
      commaSep(choice($.expression, $.initializer_list)),
      ']',
    ),

    call_expression: ($, original) => choice(
      original,
      seq(
        field('function', $.primitive_type),
        field('arguments', $.argument_list),
      ),
      // A functional cast to a dependent type: `typename S::First()`.
      seq(
        field('function', $.dependent_type),
        field('arguments', $.argument_list),
      ),
      $.qt_signature_call,
    ),

    // Qt string-based connect macros take a signal/slot signature whose
    // arguments are bare types: SIGNAL(sendData(const char*,int)). The
    // signature is not a call expression, so it gets its own rule.
    qt_signature_call: $ => seq(
      field('function', choice('SIGNAL', 'SLOT')),
      '(',
      field('signature', $.qt_signature),
      ')',
    ),

    qt_signature: $ => seq(
      field('name', choice($.identifier, $.qualified_identifier, $.template_function)),
      '(',
      optional(commaSep1(field('argument', $.qt_signature_type))),
      ')',
    ),

    // Loose commas inside a parameter-list block: the separator may be
    // split into the block (`f(a #ifdef X , b #endif c)`).
    _in_parameter_list_item: $ => choice(
      $.parameter_declaration,
      $.optional_parameter_declaration,
      $.variadic_parameter_declaration,
      '...',
      ',',
    ),

    preproc_if_in_parameter_list: $ => seq(
      alias(token(/#[ \t]*if/), '#if'),
      field('condition', $._preproc_expression),
      '\n',
      repeat($._in_parameter_list_item),
      field('alternative', optional(choice(
        alias($.preproc_else_in_parameter_list, $.preproc_else),
        alias($.preproc_elif_in_parameter_list, $.preproc_elif),
        alias($.preproc_elifdef_in_parameter_list, $.preproc_elifdef),
      ))),
      alias(token(/#[ \t]*endif/), '#endif'),
    ),

    preproc_ifdef_in_parameter_list: $ => seq(
      choice(
        alias(token(/#[ \t]*ifdef/), '#ifdef'),
        alias(token(/#[ \t]*ifndef/), '#ifndef'),
      ),
      field('name', $.identifier),
      repeat($._in_parameter_list_item),
      field('alternative', optional(choice(
        alias($.preproc_else_in_parameter_list, $.preproc_else),
        alias($.preproc_elif_in_parameter_list, $.preproc_elif),
        alias($.preproc_elifdef_in_parameter_list, $.preproc_elifdef),
      ))),
      alias(token(/#[ \t]*endif/), '#endif'),
    ),

    preproc_else_in_parameter_list: $ => seq(
      alias(token(/#[ \t]*else/), '#else'),
      repeat($._in_parameter_list_item),
    ),

    preproc_elif_in_parameter_list: $ => seq(
      alias(token(/#[ \t]*elif/), '#elif'),
      field('condition', $._preproc_expression),
      '\n',
      repeat($._in_parameter_list_item),
      field('alternative', optional(choice(
        alias($.preproc_else_in_parameter_list, $.preproc_else),
        alias($.preproc_elif_in_parameter_list, $.preproc_elif),
        alias($.preproc_elifdef_in_parameter_list, $.preproc_elifdef),
      ))),
    ),

    preproc_elifdef_in_parameter_list: $ => seq(
      choice(
        alias(token(/#[ \t]*elifdef/), '#elifdef'),
        alias(token(/#[ \t]*elifndef/), '#elifndef'),
      ),
      field('name', $.identifier),
      repeat($._in_parameter_list_item),
      field('alternative', optional(choice(
        alias($.preproc_else_in_parameter_list, $.preproc_else),
        alias($.preproc_elif_in_parameter_list, $.preproc_elif),
        alias($.preproc_elifdef_in_parameter_list, $.preproc_elifdef),
      ))),
    ),

    _in_argument_list_item: $ => choice(
      $.expression,
      $.initializer_list,
      $.compound_statement,
      ',',
      seq(
        choice(
          '||', '&&', '|', '^', '&', '==', '!=', '<', '<=', '>', '>=',
          '<<', '>>', '+', '-', '*', '/', '%', '.', '->', '.*', '->*',
          '<=>', 'or', 'and', 'bitor', 'xor', 'bitand', 'not_eq',
        ),
        $.expression,
      ),
      seq(
        $.expression,
        choice(
          '||', '&&', '|', '^', '&', '==', '!=', '<', '<=', '>', '>=',
          '<<', '>>', '+', '-', '*', '/', '%', '.', '->', '.*', '->*',
          '<=>', 'or', 'and', 'bitor', 'xor', 'bitand', 'not_eq',
        ),
      ),
    ),

    preproc_if_in_argument_list: $ => seq(
      alias(token(/#[ \t]*if/), '#if'),
      field('condition', $._preproc_expression),
      '\n',
      repeat($._in_argument_list_item),
      field('alternative', optional(choice(
        alias($.preproc_else_in_argument_list, $.preproc_else),
        alias($.preproc_elif_in_argument_list, $.preproc_elif),
        alias($.preproc_elifdef_in_argument_list, $.preproc_elifdef),
      ))),
      alias(token(/#[ \t]*endif/), '#endif'),
    ),

    preproc_ifdef_in_argument_list: $ => seq(
      choice(
        alias(token(/#[ \t]*ifdef/), '#ifdef'),
        alias(token(/#[ \t]*ifndef/), '#ifndef'),
      ),
      field('name', $.identifier),
      repeat($._in_argument_list_item),
      field('alternative', optional(choice(
        alias($.preproc_else_in_argument_list, $.preproc_else),
        alias($.preproc_elif_in_argument_list, $.preproc_elif),
        alias($.preproc_elifdef_in_argument_list, $.preproc_elifdef),
      ))),
      alias(token(/#[ \t]*endif/), '#endif'),
    ),

    preproc_else_in_argument_list: $ => seq(
      alias(token(/#[ \t]*else/), '#else'),
      repeat($._in_argument_list_item),
    ),

    preproc_elif_in_argument_list: $ => seq(
      alias(token(/#[ \t]*elif/), '#elif'),
      field('condition', $._preproc_expression),
      '\n',
      repeat($._in_argument_list_item),
      field('alternative', optional(choice(
        alias($.preproc_else_in_argument_list, $.preproc_else),
        alias($.preproc_elif_in_argument_list, $.preproc_elif),
        alias($.preproc_elifdef_in_argument_list, $.preproc_elifdef),
      ))),
    ),

    preproc_elifdef_in_argument_list: $ => seq(
      choice(
        alias(token(/#[ \t]*elifdef/), '#elifdef'),
        alias(token(/#[ \t]*elifndef/), '#elifndef'),
      ),
      field('name', $.identifier),
      repeat($._in_argument_list_item),
      field('alternative', optional(choice(
        alias($.preproc_else_in_argument_list, $.preproc_else),
        alias($.preproc_elif_in_argument_list, $.preproc_elif),
        alias($.preproc_elifdef_in_argument_list, $.preproc_elifdef),
      ))),
    ),

    _in_field_initializer_list_item: $ => choice(
      $.field_initializer,
      ',',
    ),

    preproc_if_in_field_initializer_list: $ => seq(
      alias(token(/#[ \t]*if/), '#if'),
      field('condition', $._preproc_expression),
      '\n',
      repeat($._in_field_initializer_list_item),
      field('alternative', optional(choice(
        alias($.preproc_else_in_field_initializer_list, $.preproc_else),
        alias($.preproc_elif_in_field_initializer_list, $.preproc_elif),
        alias($.preproc_elifdef_in_field_initializer_list, $.preproc_elifdef),
      ))),
      alias(token(/#[ \t]*endif/), '#endif'),
    ),

    preproc_ifdef_in_field_initializer_list: $ => seq(
      choice(
        alias(token(/#[ \t]*ifdef/), '#ifdef'),
        alias(token(/#[ \t]*ifndef/), '#ifndef'),
      ),
      field('name', $.identifier),
      repeat($._in_field_initializer_list_item),
      field('alternative', optional(choice(
        alias($.preproc_else_in_field_initializer_list, $.preproc_else),
        alias($.preproc_elif_in_field_initializer_list, $.preproc_elif),
        alias($.preproc_elifdef_in_field_initializer_list, $.preproc_elifdef),
      ))),
      alias(token(/#[ \t]*endif/), '#endif'),
    ),

    preproc_else_in_field_initializer_list: $ => seq(
      alias(token(/#[ \t]*else/), '#else'),
      repeat($._in_field_initializer_list_item),
    ),

    preproc_elif_in_field_initializer_list: $ => seq(
      alias(token(/#[ \t]*elif/), '#elif'),
      field('condition', $._preproc_expression),
      '\n',
      repeat($._in_field_initializer_list_item),
      field('alternative', optional(choice(
        alias($.preproc_else_in_field_initializer_list, $.preproc_else),
        alias($.preproc_elif_in_field_initializer_list, $.preproc_elif),
        alias($.preproc_elifdef_in_field_initializer_list, $.preproc_elifdef),
      ))),
    ),

    preproc_elifdef_in_field_initializer_list: $ => seq(
      choice(
        alias(token(/#[ \t]*elifdef/), '#elifdef'),
        alias(token(/#[ \t]*elifndef/), '#elifndef'),
      ),
      field('name', $.identifier),
      repeat($._in_field_initializer_list_item),
      field('alternative', optional(choice(
        alias($.preproc_else_in_field_initializer_list, $.preproc_else),
        alias($.preproc_elif_in_field_initializer_list, $.preproc_elif),
        alias($.preproc_elifdef_in_field_initializer_list, $.preproc_elifdef),
      ))),
    ),

    qt_signature_type: $ => seq(
      repeat(choice($.type_qualifier, $.ms_unaligned_ptr_modifier, $.ms_pointer_modifier)),
      field('type', $.type_specifier),
      optional(field('declarator', choice($._declarator, $._abstract_declarator))),
    ),

    co_await_expression: $ => prec.left(PREC.UNARY, seq(
      field('operator', 'co_await'),
      field('argument', $.expression),
    )),

    new_expression: $ => prec.right(PREC.NEW, seq(
      optional('::'),
      'new',
      field('placement', optional($.argument_list)),
      field('type', $.type_specifier),
      field('declarator', optional($.new_declarator)),
      field('arguments', optional(choice(
        $.argument_list,
        $.initializer_list,
      ))),
    )),

    new_declarator: $ => prec.right(seq(
      choice(
        seq(
          '[',
          field('length', $.expression),
          ']',
        ),
        // A pointer in the allocated type: `new int*[n]`.
        seq('*', repeat($.type_qualifier)),
      ),
      optional($.new_declarator),
    )),

    delete_expression: $ => seq(
      optional('::'),
      'delete',
      optional(seq('[', ']')),
      $.expression,
    ),

    field_expression: $ => seq(
      prec(PREC.FIELD, seq(
        field('argument', $.expression),
        field('operator', choice('.', '.*', '->', '->*')),
      )),
      field('field', choice(
        prec.dynamic(1, $._field_identifier),
        alias($.qualified_field_identifier, $.qualified_identifier),
        $.destructor_name,
        $.template_method,
        alias($.dependent_field_identifier, $.dependent_name),
        $.operator_name,
      )),
    ),

    type_requirement: $ => seq('typename', $._class_name),

    compound_requirement: $ => seq(
      '{', $.expression, '}',
      optional('noexcept'),
      optional($.trailing_return_type),
      ';',
    ),

    _requirement: $ => choice(
      alias($.expression_statement, $.simple_requirement),
      $.type_requirement,
      $.compound_requirement,
    ),

    requirement_seq: $ => seq('{', repeat($._requirement), '}'),

    constraint_conjunction: $ => prec.left(PREC.LOGICAL_AND, seq(
      field('left', $._requirement_clause_constraint),
      field('operator', choice('&&', 'and')),
      field('right', $._requirement_clause_constraint)),
    ),

    constraint_disjunction: $ => prec.left(PREC.LOGICAL_OR, seq(
      field('left', $._requirement_clause_constraint),
      field('operator', choice('||', 'or')),
      field('right', $._requirement_clause_constraint)),
    ),

    _requirement_clause_constraint: $ => choice(
      // Primary expressions"
      $.true,
      $.false,
      $._class_name,
      $.fold_expression,
      $.lambda_expression,
      $.requires_expression,

      // Parenthesized expressions
      seq('(', $.expression, ')'),

      // conjunction or disjunction of the above
      $.constraint_conjunction,
      $.constraint_disjunction,
    ),

    requires_clause: $ => seq(
      'requires',
      field('constraint', $._requirement_clause_constraint),
    ),

    requires_parameter_list: $ => seq(
      '(',
      commaSep(choice(
        $.parameter_declaration,
        $.optional_parameter_declaration,
        $.variadic_parameter_declaration,
      )),
      ')',
    ),

    requires_expression: $ => seq(
      'requires',
      field('parameters', optional(alias($.requires_parameter_list, $.parameter_list))),
      field('requirements', $.requirement_seq),
    ),

    lambda_expression: $ => seq(
      field('captures', $.lambda_capture_specifier),
      optional(seq(
        field('template_parameters', $.template_parameter_list),
        optional(field('constraint', $.requires_clause)),
      )),
      optional(field('declarator', $.abstract_function_declarator)),
      field('body', $.compound_statement),
    ),

    lambda_capture_specifier: $ => prec(PREC.LAMBDA, seq(
      '[',
      choice(
        $.lambda_default_capture,
        commaSep($._lambda_capture),
        seq(
          $.lambda_default_capture,
          ',', commaSep1($._lambda_capture),
        ),
      ),
      ']',
    )),

    lambda_default_capture: _ => choice('=', '&'),

    _lambda_capture_identifier: $ => seq(
      optional('&'),
      choice(
        $.identifier,
        $.qualified_identifier,
        alias($.identifier_parameter_pack_expansion, $.parameter_pack_expansion),
      ),
    ),

    lambda_capture_initializer: $ => seq(
      optional('&'),
      optional('...'),
      field('left', $.identifier),
      '=',
      field('right', $.expression),
    ),

    _lambda_capture: $ => choice(
      seq(optional('*'), $.this),
      $._lambda_capture_identifier,
      $.lambda_capture_initializer,
    ),

    _fold_operator: _ => choice(...FOLD_OPERATORS),
    _binary_fold_operator: _ => choice(...FOLD_OPERATORS.map((operator) => seq(field('operator', operator), '...', operator))),

    _unary_left_fold: $ => seq(
      field('left', '...'),
      field('operator', $._fold_operator),
      field('right', $.expression),
    ),
    _unary_right_fold: $ => seq(
      field('left', $.expression),
      field('operator', $._fold_operator),
      field('right', '...'),
    ),
    _binary_fold: $ => seq(
      field('left', $.expression),
      $._binary_fold_operator,
      field('right', $.expression),
    ),

    fold_expression: $ => seq(
      '(',
      choice(
        $._unary_right_fold,
        $._unary_left_fold,
        $._binary_fold,
      ),
      ')',
    ),

    parameter_pack_expansion: $ => prec(-1, seq(
      field('pattern', $.expression),
      '...',
    )),

    type_parameter_pack_expansion: $ => seq(
      field('pattern', $.type_descriptor),
      '...',
    ),

    identifier_parameter_pack_expansion: $ => seq(
      field('pattern', $.identifier),
      '...',
    ),

    sizeof_expression: ($, original) => prec.right(PREC.SIZEOF, choice(
      original,
      seq(
        'sizeof', '...',
        '(',
        field('value', $.identifier),
        ')',
      ),
    )),

    unary_expression: ($, original) => choice(
      original,
      prec.left(PREC.UNARY, seq(
        field('operator', choice('not', 'compl')),
        field('argument', $.expression),
      )),
    ),

    binary_expression: ($, original) => {
      const table = [
        ['<=>', PREC.THREE_WAY],
        ['or', PREC.LOGICAL_OR],
        ['and', PREC.LOGICAL_AND],
        ['bitor', PREC.INCLUSIVE_OR],
        ['xor', PREC.EXCLUSIVE_OR],
        ['bitand', PREC.BITWISE_AND],
        ['not_eq', PREC.EQUAL],
      ];

      return choice(
        original,
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
        // (Also present in the base binary_expression; needed here so the
        // cpp-only operators share the same tail.)
        seq(
          field('left', $.expression),
          repeat1($._infix_continuation_preproc),
        ),
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

    // The compound_statement is added to parse macros taking statements as arguments, e.g. MYFORLOOP(1, 10, i, { foo(i); bar(i); })
    // A real argument may follow a preproc block comma-free because the
    // separator comma often hides inside the block (`f(a #ifdef X , b
    // #endif c)`); between real arguments commas stay strict.
    _argument_list_real: $ => choice(
      $.expression,
      $.initializer_list,
      $.compound_statement,
      $._argument_list_type,
      $._argument_list_operator,
    ),

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
    ),

    argument_list: $ => seq(
      '(',
      repeat(choice(
        $._argument_list_real,
        $._argument_list_preproc,
        ',',
      )),
      ')',
    ),

    destructor_name: $ => prec(1, seq('~', $.identifier)),

    compound_literal_expression: ($, original) => choice(
      original,
      seq(
        field('type', choice($._class_name, $.primitive_type, $.dependent_type, $.decltype)),
        field('value', $.initializer_list),
      ),
    ),

    dependent_identifier: $ => seq('template', $.template_function),
    dependent_field_identifier: $ => seq('template', $.template_method),
    dependent_type_identifier: $ => seq('template', $.template_type),

    _scope_resolution: $ => prec(1, seq(
      field('scope', optional(choice(
        $._namespace_identifier,
        $.template_type,
        $.decltype,
        $._type_identifier,
        alias($.dependent_type_identifier, $.dependent_name),
      ))),
      '::',
    )),

    qualified_field_identifier: $ => seq(
      $._scope_resolution,
      field('name', choice(
        alias($.dependent_field_identifier, $.dependent_name),
        alias($.qualified_field_identifier, $.qualified_identifier),
        $.template_method,
        prec.dynamic(1, $._field_identifier),
        $.operator_name,
      )),
    ),

    qualified_identifier: $ => seq(
      $._scope_resolution,
      field('name', choice(
        alias($.dependent_identifier, $.dependent_name),
        $.qualified_identifier,
        $.template_function,
        seq(optional('template'), $.identifier),
        $.operator_name,
        $.destructor_name,
        $.pointer_type_declarator,
      )),
    ),

    qualified_type_identifier: $ => seq(
      $._scope_resolution,
      field('name', choice(
        alias($.dependent_type_identifier, $.dependent_name),
        alias($.qualified_type_identifier, $.qualified_identifier),
        $.template_type,
        $._type_identifier,
      )),
    ),

    qualified_operator_cast_identifier: $ => seq(
      $._scope_resolution,
      field('name', choice(
        alias($.qualified_operator_cast_identifier, $.qualified_identifier),
        $.operator_cast,
      )),
    ),

    _assignment_left_expression: ($, original) => choice(
      original,
      $.qualified_identifier,
      $.user_defined_literal,
    ),

    assignment_expression: $ => prec.right(PREC.ASSIGNMENT, choice(
      seq(
        field('left', $._assignment_left_expression),
        field('operator', choice(...ASSIGNMENT_OPERATORS)),
        field('right', choice($.expression, $.initializer_list)),
      ),
      // `x = #ifndef A v1 #else v2 #endif`: the right operand may live
      // inside a preproc block, or the block may sit before it.
      seq(
        field('left', $._assignment_left_expression),
        field('operator', choice(...ASSIGNMENT_OPERATORS)),
        repeat1($._infix_continuation_preproc),
        optional(field('right', choice($.expression, $.initializer_list))),
      ),
    )),

    _assignment_expression_lhs: $ => seq(
      field('left', $.expression),
      field('operator', choice(...ASSIGNMENT_OPERATORS)),
      field('right', choice($.expression, $.initializer_list)),
    ),

    // This prevents an ambiguity between fold expressions
    // and assignment expressions within parentheses.
    parenthesized_expression: ($, original) => choice(
      original,
      seq('(', alias($._assignment_expression_lhs, $.assignment_expression), ')'),
    ),

    operator_name: $ => prec(1, seq(
      'operator',
      choice(
        'co_await',
        '+', '-', '*', '/', '%',
        '^', '&', '|', '~',
        '!', '=', '<', '>',
        '+=', '-=', '*=', '/=', '%=', '^=', '&=', '|=',
        '<<', '>>', '>>=', '<<=',
        '==', '!=', '<=', '>=',
        '<=>',
        '&&', '||',
        '++', '--',
        ',',
        '->*',
        '->',
        '()', '[]',
        'xor', 'bitand', 'bitor', 'compl',
        'not', 'xor_eq', 'and_eq', 'or_eq', 'not_eq',
        'and', 'or',
        seq(choice('new', 'delete'), optional('[]')),
        seq('""', $.identifier),
      ),
    )),

    this: _ => 'this',

    // Preprocessor operands that are not expressions: `__has_include(<x.h>)`,
    // `__has_embed("x")`, `__has_warning("-Wx")` and `__has_cpp_attribute(ns::name)`.
    preproc_argument_list: $ => seq(
      '(',
      commaSep(choice(
        $._preproc_expression,
        $.string_literal,
        $.system_lib_string,
        alias($._preproc_qualified_name, $.qualified_identifier),
        // Operator-with-optional-operand item, for workaround/assertion
        // macros: `BOOST_WORKAROUND(BOOST_MSVC, < 1300)`, `CHECK(x, ==, y)`.
        seq(
          choice('==', '!=', '<=', '>=', '<', '>'),
          optional($._preproc_expression),
        ),
      )),
      ')',
    ),

    _preproc_qualified_name: $ => seq(
      field('scope', $._namespace_identifier),
      '::',
      field('name', $.identifier),
    ),

    // Must concatenate at least 2 nodes, one of which must be a string
    // literal. Identifier is added to parse macros that are strings, like
    // PRIu64 — so a `"lit" MACRO` / `MACRO "lit"` start is valid too.
    concatenated_string: $ => prec.dynamic(2, prec.right(seq(
      choice(
        seq(
          choice($.identifier, $.string_literal, $.raw_string_literal, $.call_expression),
          choice($.string_literal, $.raw_string_literal),
        ),
        seq(
          choice($.string_literal, $.raw_string_literal),
          choice($.identifier, $.call_expression),
        ),
      ),
      repeat(choice($.identifier, $.string_literal, $.raw_string_literal, $.call_expression)),
    ))),

    number_literal: $ => {
      const sign = /[-\+]/;
      const separator = '\'';
      const binary = /[01]/;
      const binaryDigits = seq(repeat1(binary), repeat(seq(separator, repeat1(binary))));
      const decimal = /[0-9]/;
      const firstDecimal = /[1-9]/;
      const intDecimalDigits = seq(firstDecimal, repeat(decimal), repeat(seq(separator, repeat1(decimal))));
      const floatDecimalDigits = seq(repeat1(decimal), repeat(seq(separator, repeat1(decimal))));
      const hex = /[0-9a-fA-F]/;
      const hexDigits = seq(repeat1(hex), repeat(seq(separator, repeat1(hex))));
      const octal = /[0-7]/;
      const octalDigits = seq('0', repeat(octal), repeat(seq(separator, repeat1(octal))));
      const hexExponent = seq(/[pP]/, optional(sign), floatDecimalDigits);
      const decimalExponent = seq(/[eE]/, optional(sign), floatDecimalDigits);
      const intSuffix = /(ll|LL)[uU]?|[uU](ll|LL)?|[uU][lL]?|[uU][zZ]?|[lL][uU]?|[zZ][uU]?/;
      const floatSuffix = /([fF](16|32|64|128)?)|[lL]|(bf16|BF16)/;

      // No leading sign: `-`/`+` must stay operator tokens so `f(1)-1`
      // and `x -1` don't lex the `-1` as a literal where a merged
      // macro_argument_list state also accepts a bare number.
      return token(seq(
        choice(
          seq(
            choice(
              seq(choice('0b', '0B'), binaryDigits),
              intDecimalDigits,
              seq(choice('0x', '0X'), hexDigits),
              octalDigits,
            ),
            optional(intSuffix),
          ),
          seq(
            choice(
              seq(floatDecimalDigits, decimalExponent),
              seq(floatDecimalDigits, '.', optional(floatDecimalDigits), optional(decimalExponent)),
              seq('.', floatDecimalDigits, optional(decimalExponent)),
              seq(
                choice('0x', '0X'),
                choice(
                  hexDigits,
                  seq(hexDigits, '.', optional(hexDigits)),
                  seq('.', hexDigits)),
                hexExponent,
              ),
            ),
            optional(floatSuffix),
          ),
        ),
      ));
    },

    literal_suffix: _ => token.immediate(/[a-zA-Z_]\w*/),

    user_defined_literal: $ => seq(
      choice(
        $.number_literal,
        $.char_literal,
        $._string,
      ),
      $.literal_suffix,
    ),

    // `concept` doubles as a namespace name in pre-C++20 boost headers
    // (`concept::check<T const>()`); the keyword token is accepted beside
    // identifier wherever a `ns::name` scope is expected.
    _namespace_identifier: $ => alias(choice($.identifier, 'concept'), $.namespace_identifier),
  },
});

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

/**
 * Creates a preprocessor regex rule (copied from the C base — the
 * node_modules copy does not export its helpers).
 *
 * @param {RegExp | Rule | string} command
 *
 * @returns {AliasRule}
 */
function preprocessor(command) {
  return alias(new RegExp('#[ \t]*' + command), '#' + command);
}

/**
 * Creates a set of rules for a preprocessor conditional block
 * (copied from the C base — the node_modules copy does not export
 * its helpers).
 */
function preprocIf(suffix, content, precedence = 0) {
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
