; Syntax highlighting queries for Alloy 6

; Comments
(line_comment) @comment
(block_comment) @comment

; Strings
(string) @string

; Numbers
(number) @number

; Keywords
[
  "module"
  "open"
  "as"
  "sig"
  "extends"
  "in"
  "fact"
  "pred"
  "fun"
  "assert"
  "run"
  "check"
  "enum"
  "let"
  "for"
  "but"
  "expect"
  "exactly"
  "abstract"
  "private"
  "var"
  "disj"
] @keyword

; Quantifier/multiplicity keywords
[
  "all"
  "no"
  "some"
  "lone"
  "one"
  "sum"
  "set"
  "seq"
] @keyword

; Logical/temporal operators (keyword form)
[
  "not"
  "and"
  "or"
  "implies"
  "iff"
  "else"
  "always"
  "eventually"
  "after"
  "before"
  "historically"
  "once"
  "until"
  "since"
  "releases"
  "triggered"
] @keyword.operator

; Builtin atoms
(this_expr) @variable.builtin
(iden_expr) @variable.builtin
(none_expr) @constant.builtin
(univ_expr) @constant.builtin
(int_expr) @type.builtin

; Type names in sig/enum declarations
(sig_decl names: (name_list (name) @type.definition))
(enum_decl name: (name) @type.definition)

; Sig extension type references
(sig_extension (sig_ref (qual_name (name) @type)))
(sig_extension (sig_ref_union (sig_ref (qual_name (name) @type))))

; Function/predicate names
(pred_decl name: (name) @function)
(fun_decl name: (name) @function)

; Fact/assert names
(fact_decl name: (name) @label)
(assert_decl name: (name) @label)

; Command labels
(command label: (name) @label)

; Module name
(module_decl name: (qual_name) @module)

; Open/import module name
(open_decl name: (qual_name) @module)
(open_decl alias: (name) @module)

; Field declarations — field names
(field_decl (name_list (name) @variable.member))

; Declaration variables
(decl (name_list (name) @variable.parameter))

; @ prefix names
(at_name (name) @variable)

; Operators (symbolic)
[
  "="
  "!="
  "!"
  "!in"
  "!<"
  "!>"
  "!<="
  "!>="
  "=<"
  "<"
  ">"
  "<="
  ">="
  "<=>"
  "=>"
  "&&"
  "||"
  "+"
  "-"
  "++"
  "&"
  "->"
  "<:"
  ":>"
  "."
  "~"
  "^"
  "*"
  "#"
  "<<"
  ">>"
  ">>>"
  ";"
  "'"
] @operator

; Punctuation
[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  ":"
  "|"
  "/"
  ".."
] @punctuation.delimiter
