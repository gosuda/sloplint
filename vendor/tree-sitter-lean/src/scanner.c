/*
 * External scanner for tree-sitter-lean.
 *
 * Externalized tokens:
 *   1. `_scanned_ident` — dotted, `«quoted»`, or Unicode identifiers.
 *      Plain ASCII single-component idents go through tree-sitter's
 *      internal lexer so word/keyword extraction works.
 *   2. Nested block/doc/module-doc comments.
 *   3. Raw string literals with matched `#`-delimiter counts.
 *   4. Layout-sensitive tokens `_indent`, `_dedent`, `_newline` that
 *      delimit tactic and do-block bodies.
 *
 * Lean's character classifications mirror `src/Init/Meta/Defs.lean`
 * upstream.
 */

#include "tree_sitter/parser.h"
#include "tree_sitter/alloc.h"
#include "tree_sitter/array.h"

#include <stdbool.h>
#include <stdint.h>
#include <string.h>

enum TokenType {
  SCANNED_IDENT,
  BLOCK_COMMENT,
  DOC_COMMENT,
  MODULE_DOC_COMMENT,
  RAW_STRING_LITERAL,
  INDENT,
  DEDENT,
  NEWLINE,
  BODY_INLINE,
  BODY_INDENT,
  BRACE_OPEN,
  BRACE_CLOSE,
  RECORD_ANCHOR,
  RECORD_NEWLINE,
  RECORD_COMMA,
  COMMA,
  COLON,
  ASSIGN,
  RECORD_ASSIGN,
  GENERALIZE_EQ,
  FOR_DO,
  ELSE,
  CATCH,
  FINALLY,
  BAR,
  WHERE,
  NORM_OPEN,
  NORM_CLOSE,
  ERROR_SENTINEL,
};

#define NORM_BAR        0x2016 // ‖
#define SUBSCRIPT_PLUS  0x208a // ₊

#define ID_BEGIN_ESCAPE 0x00ab // «
#define ID_END_ESCAPE   0x00bb // »

static inline bool is_alpha_ascii(int32_t c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z');
}

static inline bool is_digit_ascii(int32_t c) {
  return c >= '0' && c <= '9';
}

static bool is_letter_like(int32_t c) {
  return (0x3b1   <= c && c <= 0x3c9   && c != 0x3bb)
      || (0x391   <= c && c <= 0x3a9   && c != 0x3a0 && c != 0x3a3)
      || (0x3ca   <= c && c <= 0x3fb)
      || (0x1f00  <= c && c <= 0x1ffe)
      || (0x2100  <= c && c <= 0x214f)
      || (0x1d400 <= c && c <= 0x1d7ff)  /* Math alphanumeric symbols (𝟙, 𝒜, 𝔽, …) */
      || (0x00c0  <= c && c <= 0x00ff && c != 0x00d7 && c != 0x00f7)
      || (0x0100  <= c && c <= 0x017f);
}

static bool is_subscript_alnum(int32_t c) {
  return (0x2080 <= c && c <= 0x2089)
      || (0x2090 <= c && c <= 0x209c)
      || (0x1d62 <= c && c <= 0x1d6a)
      || c == 0x2c7c;
}

/* Superscripts are not id-rest upstream, but they can't start a
   notation/operator of their own — user notations like `ℝ³` or `xⁿ`
   otherwise split at the superscript and break the whole command.
   Identifying them keeps one name per atom; they are never
   id-first, so `x ²` can't produce this path. */
static bool is_superscript_alnum(int32_t c) {
  return c == 0x00b2 || c == 0x00b3 || c == 0x00b9
      || (0x2070 <= c && c <= 0x207f)
      || (0x1d2c <= c && c <= 0x1d61);
}

static bool is_id_first(int32_t c) {
  return is_alpha_ascii(c) || c == '_' || is_letter_like(c);
}

static bool is_id_rest(int32_t c) {
  return is_alpha_ascii(c)
      || is_digit_ascii(c)
      || c == '_' || c == '\'' || c == '!' || c == '?'
      || is_letter_like(c) || is_subscript_alnum(c)
      || is_superscript_alnum(c)
      /* Mathlib postfix-numeral atoms (`ℝ≥0`, `ℝ≥0∞`, `ℕ∞`, `ℤ≥0`):
         the postfix chars join the identifier instead of splitting
         into a fresh operator token. Side effect: unspaced `x≥0`
         scans as one identifier — linting only cares that it parses. */
      || c == 0x2265 || c == 0x2264 || c == '>' || c == '<'
      || c == 0x221e;
}

/* Every layout delimiter owns a serialized frame. Inline bodies need the
   same boundary state as indented bodies; braces shield enclosing frames. */
enum FrameKind { ROOT_FRAME, INDENT_FRAME, BODY_FRAME, BRACE_FRAME, RECORD_FRAME };
struct Frame { uint32_t column; enum FrameKind kind; };
struct Scanner {
  Array(struct Frame) frames;
  unsigned state_size;
};

static unsigned column_size(uint32_t column) {
  unsigned size = 1;
  while (column >= 128) { column >>= 7; size++; }
  return size;
}

static unsigned frame_size(struct Frame frame) {
  return 1 + column_size(frame.column);
}

static bool push_frame(struct Scanner *scanner, enum FrameKind kind,
                       uint32_t column) {
  struct Frame frame = { .column = column, .kind = kind };
  unsigned size = frame_size(frame);
  if (scanner->state_size + size > TREE_SITTER_SERIALIZATION_BUFFER_SIZE)
    return false;
  array_push(&scanner->frames, frame);
  scanner->state_size += size;
  return true;
}

static struct Frame *top_frame(struct Scanner *scanner) {
  return array_back(&scanner->frames);
}

static bool is_brace(struct Frame frame) {
  return frame.kind == BRACE_FRAME || frame.kind == RECORD_FRAME;
}

static bool pop_layout(struct Scanner *scanner, TSLexer *lexer,
                       const bool *valid_symbols) {
  if (!valid_symbols[DEDENT] || scanner->frames.size <= 1
      || is_brace(*top_frame(scanner))) return false;
  struct Frame frame = array_pop(&scanner->frames);
  scanner->state_size -= frame_size(frame);
  lexer->result_symbol = DEDENT;
  return true;
}

static inline void advance(TSLexer *lexer) { lexer->advance(lexer, false); }
static inline void skip(TSLexer *lexer)    { lexer->advance(lexer, true); }

/* ---------- identifier --------------------------------------------------- */

/*
 * Consume a single identifier part: `«…»` or `isIdFirst isIdRest*`.
 * Three out-params are vestigial from an earlier keyword-deferral
 * approach; kept so callers don't need to change shape.
 */
static bool scan_ident_part(TSLexer *lexer,
                            char *buf, size_t buf_size,
                            size_t *out_len, bool *out_is_ascii) {
  (void)buf; (void)buf_size;
  *out_len = 0;
  *out_is_ascii = true;

  int32_t c = lexer->lookahead;
  if (c == ID_BEGIN_ESCAPE) {
    *out_is_ascii = false;
    advance(lexer);
    while (lexer->lookahead != 0
        && lexer->lookahead != ID_END_ESCAPE
        && lexer->lookahead != '\n') {
      advance(lexer);
    }
    if (lexer->lookahead != ID_END_ESCAPE) return false;
    advance(lexer);
    return true;
  }
  if (!is_id_first(c)) return false;

  do {
    if (lexer->lookahead >= 0x80) *out_is_ascii = false;
    advance(lexer);
  } while (is_id_rest(lexer->lookahead));

  return true;
}

/*
 * Continue scanning an identifier after `len` chars have already been
 * consumed and buffered in `buf`. Used by the dispatch when a leading
 * character (e.g. `r`, `s`, `m`) was eaten to peek for a non-identifier
 * token but turned out to belong to an identifier after all.
 */
/*
 * Continue scanning an identifier after the first character has been
 * consumed. `seen_unusual` tracks whether anything beyond a plain ASCII
 * single-component identifier has been seen — non-ASCII, an escape, or
 * a dot-continuation. If nothing unusual appears, the scanner returns
 * false so tree-sitter's internal lexer (with its keyword-extraction)
 * handles the token instead.
 */
static bool scan_identifier_continue(TSLexer *lexer, bool seen_unusual) {
  while (is_id_rest(lexer->lookahead)) {
    if (lexer->lookahead >= 0x80) seen_unusual = true;
    advance(lexer);
  }
  lexer->mark_end(lexer);

  while (lexer->lookahead == '.') {
    advance(lexer);
    int32_t next = lexer->lookahead;
    if (next != ID_BEGIN_ESCAPE && !is_id_first(next)) break;
    seen_unusual = true;
    char dummy_buf[2];
    size_t dummy_len;
    bool dummy_ascii;
    if (!scan_ident_part(lexer, dummy_buf, sizeof(dummy_buf),
                         &dummy_len, &dummy_ascii)) break;
    lexer->mark_end(lexer);
  }

  if (!seen_unusual) return false;
  lexer->result_symbol = SCANNED_IDENT;
  return true;
}

static bool scan_identifier(TSLexer *lexer) {
  int32_t c = lexer->lookahead;
  if (c == ID_BEGIN_ESCAPE) {
    char dummy_buf[2];
    size_t dummy_len;
    bool dummy_ascii;
    if (!scan_ident_part(lexer, dummy_buf, sizeof(dummy_buf),
                         &dummy_len, &dummy_ascii)) return false;
    lexer->mark_end(lexer);
    while (lexer->lookahead == '.') {
      advance(lexer);
      int32_t next = lexer->lookahead;
      if (next != ID_BEGIN_ESCAPE && !is_id_first(next)) break;
      if (!scan_ident_part(lexer, dummy_buf, sizeof(dummy_buf),
                           &dummy_len, &dummy_ascii)) break;
      lexer->mark_end(lexer);
    }
    lexer->result_symbol = SCANNED_IDENT;
    return true;
  }
  if (!is_id_first(c)) return false;
  bool seen_unusual = (c >= 0x80);
  advance(lexer);
  return scan_identifier_continue(lexer, seen_unusual);
}

/* ---------- comments ----------------------------------------------------- */

/*
 * Consume the body of a `/- … -/` style comment, honoring nesting.
 * The opening `/-` (or `/--`, `/-!`) must already have been consumed.
 */
static bool scan_block_comment_body(TSLexer *lexer, bool *saw_newline) {
  unsigned depth = 1;
  while (depth > 0) {
    if (lexer->eof(lexer)) return false;
    if (saw_newline && lexer->lookahead == '\n') *saw_newline = true;
    if (lexer->lookahead == '/') {
      advance(lexer);
      if (lexer->lookahead == '-') {
        advance(lexer);
        depth++;
      }
    } else if (lexer->lookahead == '-') {
      advance(lexer);
      if (lexer->lookahead == '/') {
        advance(lexer);
        depth--;
      }
    } else {
      advance(lexer);
    }
  }
  return true;
}

/*
 * Dispatch among `/- … -/`, `/-- … -/`, `/-! … -/` based on the third
 * character after the opening `/-`. The caller has consumed nothing yet.
 */
static bool scan_block_or_doc_comment(TSLexer *lexer,
                                      const bool *valid_symbols) {
  if (lexer->lookahead != '/') return false;
  advance(lexer);
  if (lexer->lookahead != '-') return false;
  advance(lexer);

  int kind;
  if (lexer->lookahead == '-' && valid_symbols[DOC_COMMENT]) {
    advance(lexer);
    /* Avoid eating `/---` style ASCII rulers as doc comments? Lean does
       treat them as doc comments, so accept. */
    kind = DOC_COMMENT;
  } else if (lexer->lookahead == '!' && valid_symbols[MODULE_DOC_COMMENT]) {
    advance(lexer);
    kind = MODULE_DOC_COMMENT;
  } else if (valid_symbols[BLOCK_COMMENT]) {
    kind = BLOCK_COMMENT;
  } else {
    return false;
  }

  if (!scan_block_comment_body(lexer, NULL)) return false;
  lexer->mark_end(lexer);
  lexer->result_symbol = kind;
  return true;
}

/* ---------- raw strings -------------------------------------------------- */

/*
 * Recognize r"…", r#"…"#, r##"…"##, … with matching `#` counts. The
 * leading `r` is assumed already consumed by the dispatcher.
 */
static bool scan_raw_string_after_r(TSLexer *lexer) {
  unsigned hashes = 0;
  while (lexer->lookahead == '#') {
    advance(lexer);
    hashes++;
  }
  if (lexer->lookahead != '"') return false;
  advance(lexer);

  for (;;) {
    if (lexer->eof(lexer)) return false;
    if (lexer->lookahead == '"') {
      advance(lexer);
      unsigned seen = 0;
      while (seen < hashes && lexer->lookahead == '#') {
        advance(lexer);
        seen++;
      }
      if (seen == hashes) {
        lexer->mark_end(lexer);
        lexer->result_symbol = RAW_STRING_LITERAL;
        return true;
      }
    } else {
      advance(lexer);
    }
  }
}

/* ---------- layout ------------------------------------------------------ */

/* Look ahead without moving mark_end. Comment text remains an ordinary
   comment node when the caller emits its zero-width boundary token. */
static bool preview_content(TSLexer *lexer, bool *saw_newline,
                            uint32_t *column, int32_t *first) {
  for (;;) {
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t'
        || lexer->lookahead == '\r' || lexer->lookahead == '\n') {
      if (lexer->lookahead == '\n') *saw_newline = true;
      advance(lexer);
    }
    if (lexer->eof(lexer)) return false;
    *column = lexer->get_column(lexer);
    *first = lexer->lookahead;
    if (lexer->lookahead == '/') {
      advance(lexer);
      if (lexer->lookahead != '-') return true;
      advance(lexer);
      if (!scan_block_comment_body(lexer, saw_newline)) return false;
      continue;
    }
    if (lexer->lookahead == '-') {
      advance(lexer);
      if (lexer->lookahead != '-') return true;
      while (!lexer->eof(lexer) && lexer->lookahead != '\n') advance(lexer);
      continue;
    }
    return true;
  }
}

static bool begin_body(struct Scanner *scanner, TSLexer *lexer,
                       const bool *valid_symbols) {
  bool saw_newline = false;
  uint32_t column = 0;
  int32_t first = 0;
  lexer->mark_end(lexer);
  if (!preview_content(lexer, &saw_newline, &column, &first)) return false;
  enum TokenType token = saw_newline ? BODY_INDENT : BODY_INLINE;
  if (!valid_symbols[token] || !push_frame(scanner, BODY_FRAME, column))
    return false;
  lexer->result_symbol = token;
  return true;
}

static bool record_anchor(struct Scanner *scanner, TSLexer *lexer) {
  struct Frame *frame = top_frame(scanner);
  if (frame->kind != BRACE_FRAME) return false;
  bool saw_newline = false;
  uint32_t column = 0;
  int32_t first = 0;
  lexer->mark_end(lexer);
  if (!preview_content(lexer, &saw_newline, &column, &first)) return false;
  struct Frame replacement = { .column = column, .kind = RECORD_FRAME };
  unsigned size = scanner->state_size - frame_size(*frame) + frame_size(replacement);
  if (size > TREE_SITTER_SERIALIZATION_BUFFER_SIZE) return false;
  *frame = replacement;
  scanner->state_size = size;
  lexer->result_symbol = RECORD_ANCHOR;
  return true;
}

static bool scan_brace_open(struct Scanner *scanner, TSLexer *lexer) {
  advance(lexer);
  if (lexer->lookahead == '{') return false;
  lexer->mark_end(lexer);
  bool saw_newline = false;
  uint32_t column = lexer->get_column(lexer);
  int32_t first = 0;
  (void)preview_content(lexer, &saw_newline, &column, &first);
  if (!push_frame(scanner, BRACE_FRAME, column)) return false;
  lexer->result_symbol = BRACE_OPEN;
  return true;
}

static bool scan_brace_close(struct Scanner *scanner, TSLexer *lexer,
                             const bool *valid_symbols) {
  if (pop_layout(scanner, lexer, valid_symbols)) return true;
  if (!valid_symbols[BRACE_CLOSE] || !is_brace(*top_frame(scanner)))
    return false;
  struct Frame frame = array_pop(&scanner->frames);
  scanner->state_size -= frame_size(frame);
  advance(lexer);
  lexer->mark_end(lexer);
  lexer->result_symbol = BRACE_CLOSE;
  return true;
}

/* Both update sources and record fields use COMMA. RECORD_COMMA is a
   grammar-context marker: once := or with has identified the record,
   only a field separator checks Lean's first-field indentation anchor. */
static bool scan_comma(struct Scanner *scanner, TSLexer *lexer,
                       const bool *valid_symbols) {
  struct Frame frame = *top_frame(scanner);
  advance(lexer);
  lexer->mark_end(lexer);
  if (frame.kind == RECORD_FRAME && valid_symbols[RECORD_COMMA]) {
    bool saw_newline = false;
    uint32_t column = 0;
    int32_t first = 0;
    if (!preview_content(lexer, &saw_newline, &column, &first)) return false;
    if (first != '}' && first != ':' && first != '.' && column < frame.column)
      return false;
  }
  lexer->result_symbol = COMMA;
  return true;
}

/* A grammar-owned assignment stays in its current declaration or statement.
   Only an assignment belonging to an enclosing construct closes an inner
   layout body first. Record fields retain their distinct brace context. */
static bool scan_colon(struct Scanner *scanner, TSLexer *lexer,
                       const bool *valid_symbols) {
  advance(lexer);
  if (lexer->lookahead == '=') {
    if (!valid_symbols[RECORD_ASSIGN] && !valid_symbols[ASSIGN])
      return pop_layout(scanner, lexer, valid_symbols);
    advance(lexer);
    if (valid_symbols[RECORD_ASSIGN]) {
      if (top_frame(scanner)->kind == BRACE_FRAME)
        top_frame(scanner)->kind = RECORD_FRAME;
      lexer->result_symbol = RECORD_ASSIGN;
    } else {
      lexer->result_symbol = ASSIGN;
    }
  } else {
    if (lexer->lookahead == ':') return false;
    if (!valid_symbols[COLON])
      return pop_layout(scanner, lexer, valid_symbols);
    lexer->result_symbol = COLON;
  }
  lexer->mark_end(lexer);
  return true;
}

static int clause_token(const char *word) {
  if (strcmp(word, "else") == 0) return ELSE;
  if (strcmp(word, "catch") == 0) return CATCH;
  if (strcmp(word, "finally") == 0) return FINALLY;
  if (strcmp(word, "where") == 0) return WHERE;
  if (strcmp(word, "do") == 0) return FOR_DO;
  return -1;
}

static int scan_clause_word(TSLexer *lexer) {
  char word[8];
  unsigned length = 0;
  while (length + 1 < sizeof word && is_alpha_ascii(lexer->lookahead)) {
    word[length++] = (char)lexer->lookahead;
    advance(lexer);
  }
  word[length] = 0;
  if (is_id_rest(lexer->lookahead) || lexer->lookahead == '.') return -1;
  return clause_token(word);
}

static bool explicit_close(int32_t c) {
  return c == ')' || c == ']' || c == '}' || c == ',' || c == ':'
      || c == 0x27e9 || c == 0x203a || c == 0x27eb;
}

static bool can_pop_layout(struct Scanner *scanner,
                           const bool *valid_symbols) {
  return valid_symbols[DEDENT] && scanner->frames.size > 1
      && !is_brace(*top_frame(scanner));
}

/* Determine the pending offside boundary without mutating its frame. A
   grammar-owned comma, colon or clause must be recognized first: it may
   continue the current construct even when an inner inline body has a
   larger first-element column. */
static int layout_token(struct Scanner *scanner, TSLexer *lexer,
                        const bool *valid_symbols, bool saw_newline) {
  struct Frame top = *top_frame(scanner);
  bool can_pop = can_pop_layout(scanner, valid_symbols);
  if (lexer->eof(lexer)) return can_pop ? DEDENT : -1;
  if (explicit_close(lexer->lookahead)) return can_pop ? DEDENT : -1;
  if (!saw_newline) return -1;
  uint32_t column = lexer->get_column(lexer);
  if (!is_brace(top) && column < top.column) return can_pop ? DEDENT : -1;
  if (lexer->lookahead == '|') return -1;
  if (is_brace(top))
    return valid_symbols[RECORD_NEWLINE] && column == top.column
        ? RECORD_NEWLINE : -1;
  if (column > top.column && valid_symbols[INDENT]) return INDENT;
  return column == top.column && valid_symbols[NEWLINE] ? NEWLINE : -1;
}

static bool emit_layout(struct Scanner *scanner, TSLexer *lexer,
                        const bool *valid_symbols, int token,
                        uint32_t column) {
  if (token < 0) return false;
  if (token == DEDENT) return pop_layout(scanner, lexer, valid_symbols);
  if (token == INDENT && !push_frame(scanner, INDENT_FRAME, column))
    return false;
  lexer->result_symbol = (TSSymbol)token;
  return true;
}

/* ---------- dispatch ----------------------------------------------------- */

static bool scan(struct Scanner *scanner, TSLexer *lexer,
                 const bool *valid_symbols) {
  if (valid_symbols[ERROR_SENTINEL]) return false;
  if (valid_symbols[BODY_INLINE] || valid_symbols[BODY_INDENT])
    return begin_body(scanner, lexer, valid_symbols);
  if (valid_symbols[RECORD_ANCHOR])
    return record_anchor(scanner, lexer);

  /* DEDENT retains the triggering whitespace. Separators consume it once.
     Lookahead never becomes hidden state: every successful zero-width token
     changes the complete serialized frame stack. */
  lexer->mark_end(lexer);
  bool saw_newline = false;
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t'
      || lexer->lookahead == '\r' || lexer->lookahead == '\n') {
    if (lexer->lookahead == '\n') saw_newline = true;
    skip(lexer);
  }
  uint32_t column = lexer->get_column(lexer);
  int layout = layout_token(scanner, lexer, valid_symbols, saw_newline);
  if (layout != DEDENT) lexer->mark_end(lexer);
  if (lexer->eof(lexer))
    return emit_layout(scanner, lexer, valid_symbols, layout, column);

  if (lexer->lookahead == ',' && valid_symbols[COMMA])
    return scan_comma(scanner, lexer, valid_symbols);
  if (lexer->lookahead == ':' &&
      (valid_symbols[COLON] || valid_symbols[ASSIGN] || valid_symbols[RECORD_ASSIGN]))
    return scan_colon(scanner, lexer, valid_symbols);
  if (lexer->lookahead == '=' && valid_symbols[GENERALIZE_EQ]) {
    advance(lexer);
    if (lexer->lookahead == '=' || lexer->lookahead == '>') return false;
    lexer->mark_end(lexer);
    lexer->result_symbol = GENERALIZE_EQ;
    return true;
  }
  if (lexer->lookahead == '|') {
    advance(lexer);
    if (lexer->lookahead == '|' || lexer->lookahead == '>') return false;
    if (valid_symbols[BAR]) {
      lexer->mark_end(lexer);
      lexer->result_symbol = BAR;
      return true;
    }
    return pop_layout(scanner, lexer, valid_symbols);
  }
  if (lexer->lookahead == 'e' || lexer->lookahead == 'c'
      || lexer->lookahead == 'f' || lexer->lookahead == 'w'
      || (lexer->lookahead == 'd' && valid_symbols[FOR_DO])) {
    int token = scan_clause_word(lexer);
    if (token >= 0 && valid_symbols[token]) {
      lexer->mark_end(lexer);
      lexer->result_symbol = (TSSymbol)token;
      return true;
    }
    if (token >= 0 && token != FOR_DO &&
        pop_layout(scanner, lexer, valid_symbols)) return true;
    if (emit_layout(scanner, lexer, valid_symbols, layout, column)) return true;
    if (valid_symbols[SCANNED_IDENT])
      return scan_identifier_continue(lexer, false);
    return false;
  }
  if (emit_layout(scanner, lexer, valid_symbols, layout, column)) return true;
  if (lexer->lookahead == '}' && valid_symbols[BRACE_CLOSE])
    return scan_brace_close(scanner, lexer, valid_symbols);
  if (lexer->lookahead == '{' && valid_symbols[BRACE_OPEN])
    return scan_brace_open(scanner, lexer);

  /* Norm bars `‖ … ‖`. The same glyph opens and closes, disambiguated
     by parser context: prefer CLOSE (only valid once a norm is open and
     its term parsed) so non-nested `‖x‖` closes eagerly; otherwise OPEN.
     A trailing `₊` (non-negative norm `‖x‖₊`) folds into the close. */
  if (lexer->lookahead == NORM_BAR
      && (valid_symbols[NORM_OPEN] || valid_symbols[NORM_CLOSE])) {
    advance(lexer);
    if (valid_symbols[NORM_CLOSE]) {
      if (lexer->lookahead == SUBSCRIPT_PLUS) advance(lexer);
      lexer->mark_end(lexer);
      lexer->result_symbol = NORM_CLOSE;
      return true;
    }
    lexer->mark_end(lexer);
    lexer->result_symbol = NORM_OPEN;
    return true;
  }

  if ((valid_symbols[BLOCK_COMMENT]
    || valid_symbols[DOC_COMMENT]
    || valid_symbols[MODULE_DOC_COMMENT])
      && lexer->lookahead == '/') {
    if (scan_block_or_doc_comment(lexer, valid_symbols)) return true;
    return false;
  }

  /* `r` may start either a raw string or an identifier. Both can be
     valid at the same parser position, so we peek past the `r` and
     dispatch based on what follows. If neither path matches, we still
     have to consume the `r` we just ate — fall through as a 1-char
     identifier. */
  if (lexer->lookahead == 'r'
      && (valid_symbols[RAW_STRING_LITERAL] || valid_symbols[SCANNED_IDENT])) {
    advance(lexer);
    int32_t next = lexer->lookahead;
    if (valid_symbols[RAW_STRING_LITERAL]
        && (next == '"' || next == '#')) {
      return scan_raw_string_after_r(lexer);
    }
    if (valid_symbols[SCANNED_IDENT]) {
      /* Continue an identifier that already has one ASCII char. Plain
         single-component idents are handled by tree-sitter's internal
         lexer, so this path only succeeds if a dot continuation or a
         non-ASCII char shows up. */
      return scan_identifier_continue(lexer, false);
    }
    return false;
  }

  if (valid_symbols[SCANNED_IDENT]
      && (lexer->lookahead == ID_BEGIN_ESCAPE
       || is_id_first(lexer->lookahead))) {
    if (scan_identifier(lexer)) return true;
  }

  return false;
}

/* ---------- tree-sitter ABI --------------------------------------------- */

void *tree_sitter_lean_external_scanner_create(void) {
  struct Scanner *scanner = ts_calloc(1, sizeof(struct Scanner));
  array_init(&scanner->frames);
  (void)push_frame(scanner, ROOT_FRAME, 0);
  return scanner;
}

void tree_sitter_lean_external_scanner_destroy(void *payload) {
  struct Scanner *scanner = payload;
  array_delete(&scanner->frames);
  ts_free(scanner);
}

unsigned tree_sitter_lean_external_scanner_serialize(void *payload, char *buffer) {
  struct Scanner *scanner = payload;
  unsigned offset = 0;
  for (uint32_t i = 0; i < scanner->frames.size; i++) {
    struct Frame frame = *array_get(&scanner->frames, i);
    buffer[offset++] = (char)frame.kind;
    uint32_t column = frame.column;
    do {
      uint8_t byte = column & 127;
      column >>= 7;
      buffer[offset++] = (char)(byte | (column ? 128 : 0));
    } while (column);
  }
  return offset;
}

void tree_sitter_lean_external_scanner_deserialize(void *payload,
                                                  const char *buffer,
                                                  unsigned length) {
  struct Scanner *scanner = payload;
  array_clear(&scanner->frames);
  scanner->state_size = 0;
  unsigned offset = 0;
  while (offset < length) {
    enum FrameKind kind = (enum FrameKind)(uint8_t)buffer[offset++];
    uint32_t column = 0;
    unsigned shift = 0;
    uint8_t byte;
    do {
      if (offset >= length || shift > 28) {
        array_clear(&scanner->frames);
        scanner->state_size = 0;
        (void)push_frame(scanner, ROOT_FRAME, 0);
        return;
      }
      byte = (uint8_t)buffer[offset++];
      column |= (uint32_t)(byte & 127) << shift;
      shift += 7;
    } while (byte & 128);
    (void)push_frame(scanner, kind, column);
  }
  if (scanner->frames.size == 0) (void)push_frame(scanner, ROOT_FRAME, 0);
}

bool tree_sitter_lean_external_scanner_scan(void *payload, TSLexer *lexer,
                                            const bool *valid_symbols) {
  return scan((struct Scanner *)payload, lexer, valid_symbols);
}
