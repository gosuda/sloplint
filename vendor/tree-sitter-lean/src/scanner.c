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

/*
 * Indent stack: each entry is the column (in columns, not bytes) of an
 * open tactic/do block. Always contains at least one entry — the base
 * (0). Tokens push and pop based on observed line indentation.
 */
struct Scanner {
  Array(uint16_t) indents;
  /* Set when a DEDENT lands exactly on an enclosing block level: the
     newline that terminated the inner block also separates statements
     at the outer level, but the DEDENT consumed it.  The next scan
     re-emits it as NEWLINE when the grammar wants one, so a sibling
     statement can follow a nested block (`try`/`let x <- do`/...
     then another stmt at the outer indent). */
  bool pending_newline;
  /* Column an in-flight multi-level dedent unwinds toward, or -1.
     scan_layout consumes the newline while measuring the next line's
     indent, so after emitting the first DEDENT there is no newline
     left to trigger the remaining pops: this target replays DEDENTs
     on subsequent scans until the stack unwinds to it (`for … do` /
     `finally` bodies followed by a shallower declaration). */
  int32_t dedent_target;
};

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
static bool scan_block_comment_body(TSLexer *lexer) {
  unsigned depth = 1;
  while (depth > 0) {
    if (lexer->eof(lexer)) return false;
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

  if (!scan_block_comment_body(lexer)) return false;
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
        lexer->result_symbol = RAW_STRING_LITERAL;
        return true;
      }
    } else {
      advance(lexer);
    }
  }
}

/* ---------- layout (indent / dedent / newline) -------------------------- */

/*
 * If we're sitting on a newline AND any layout token is valid, walk
 * forward to the next non-blank, non-comment line and report what
 * happened to the indent level.
 *
 *   indent > top  → push, emit INDENT
 *   indent < top  → pop one, emit DEDENT (caller will re-invoke us if
 *                   more pops are needed)
 *   indent == top → emit NEWLINE
 *
 * Lines that are entirely blank or contain only a comment don't
 * participate — they're skipped, and we use the indent of the next
 * real content line.
 */
static bool scan_layout(struct Scanner *scanner, TSLexer *lexer,
                        const bool *valid_symbols) {
  if (!(valid_symbols[INDENT]
     || valid_symbols[DEDENT]
     || valid_symbols[NEWLINE])) {
    return false;
  }

  /* Skip trailing spaces/tabs on the current line; we only care about
     layout once we reach a newline. */
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t'
      || lexer->lookahead == '\r') {
    skip(lexer);
  }

  /* If we're not at a newline (or EOF), no layout token applies. */
  if (lexer->lookahead != '\n' && !lexer->eof(lexer)) return false;

  uint32_t indent = 0;
  bool saw_newline = false;
  for (;;) {
    if (lexer->lookahead == '\n') {
      indent = 0;
      saw_newline = true;
      skip(lexer);
    } else if (lexer->lookahead == ' ') {
      indent++;
      skip(lexer);
    } else if (lexer->lookahead == '\t') {
      indent += 8;
      skip(lexer);
    } else if (lexer->lookahead == '\r') {
      skip(lexer);
    } else if (lexer->eof(lexer)) {
      /* EOF closes every open block. */
      if (valid_symbols[DEDENT] && scanner->indents.size > 1) {
        array_pop(&scanner->indents);
        lexer->result_symbol = DEDENT;
        return true;
      }
      return false;
    } else {
      break;
    }
  }
  if (!saw_newline) return false;

  uint16_t top = scanner->indents.size > 0
    ? *array_back(&scanner->indents)
    : 0;

  /* `|` always continues the surrounding rule (match alt, ctor alt,
     pipe-operator) — never terminates a layout block. Suppress
     NEWLINE/DEDENT at the start of a `|`-line so the parser stays
     inside the match/inductive/etc. */
  if (lexer->lookahead == '|') return false;

  if (indent > top) {
    if (valid_symbols[INDENT]) {
      array_push(&scanner->indents, (uint16_t)indent);
      lexer->result_symbol = INDENT;
      return true;
    }
    /* Continuation of a multi-line expression: the new line is more
       indented than the surrounding block, so neither INDENT, DEDENT,
       nor NEWLINE applies. Returning nothing lets the parser keep
       consuming tokens as part of the same enclosing rule. */
    return false;
  }
  if (indent < top && valid_symbols[DEDENT]) {
    array_pop(&scanner->indents);
    lexer->result_symbol = DEDENT;
    scanner->pending_newline =
        scanner->indents.size > 0
        && *array_back(&scanner->indents) == indent;
    /* The newline is consumed: if the stack is still deeper than this
       line, record the target so scan() keeps emitting DEDENTs. */
    scanner->dedent_target =
        scanner->indents.size > 0
        && *array_back(&scanner->indents) > indent
        ? (int32_t)indent : -1;
    return true;
  }
  if (indent == top && valid_symbols[NEWLINE]) {
    /* A `catch`/`finally` line closes out a do_try's inner block and
       attaches via extras — emitting NEWLINE here would feed the
       enclosing statement separator instead, orphaning the clause.
       mark_end before the peek: the word's characters must stay
       outside the NEWLINE token or any other `c`/`f`-led line
       (`for`, `fun`, `cases`, `calc`, …) would lose its first word. */
    if (lexer->lookahead == 'c' || lexer->lookahead == 'f') {
      lexer->mark_end(lexer);
      char word[8];
      int n = 0;
      while (n < 7 && is_alpha_ascii(lexer->lookahead)) {
        word[n++] = (char)lexer->lookahead;
        advance(lexer);
      }
      word[n] = 0;
      if (((n == 5 && memcmp(word, "catch", 5) == 0)
           || (n == 7 && memcmp(word, "finally", 7) == 0))
          && !is_id_rest(lexer->lookahead)) {
        return false;
      }
    }
    lexer->result_symbol = NEWLINE;
    return true;
  }
  return false;
}

/* ---------- dispatch ----------------------------------------------------- */

static bool scan(struct Scanner *scanner, TSLexer *lexer,
                 const bool *valid_symbols) {
  if (valid_symbols[ERROR_SENTINEL]) return false;

  /* Multi-level dedent continuation: the triggering newline is already
     consumed, so replay the remaining DEDENTs without re-measuring.
     Serialized with the indents stack, a checkpoint replay sees a
     consistent (target, stack) pair.  When the stack has unwound past
     the target the flag is stale — clear it and scan normally. */
  if (scanner->dedent_target >= 0 && valid_symbols[DEDENT]) {
    uint16_t t = scanner->indents.size > 0
      ? *array_back(&scanner->indents) : 0;
    if (t > (uint16_t)scanner->dedent_target) {
      uint16_t tgt = (uint16_t)scanner->dedent_target;
      array_pop(&scanner->indents);
      lexer->result_symbol = DEDENT;
      uint16_t b = scanner->indents.size > 0
        ? *array_back(&scanner->indents) : 0;
      /* Landing exactly on an enclosing level: the consumed newline
         is still that level's statement separator. */
      scanner->pending_newline =
          scanner->indents.size > 0 && b == tgt;
      if (b <= tgt)
        scanner->dedent_target = -1;
      return true;
    }
    scanner->dedent_target = -1;
  }

  /* The DEDENT that closed an inner block also consumed the newline
     that separates statements at the outer level — re-emit it here.
     This is a one-shot flag: it must NOT be restored by deserialize
     (the runtime only serializes after a scan that returns true, so
     clearing it in a return-false call never persists — the stale
     checkpoint would resurrect it at every later position).  A
     `catch`/`finally` word peeks out of the emission: those clauses
     attach to the enclosing rule via extras, so a NEWLINE would be
     misread as a sibling-statement separator.
     mark_end before the peek, same as in scan_layout: without it
     the peeked word is swallowed into the NEWLINE token. */
  if (scanner->pending_newline) {
    if (lexer->lookahead == 'c' || lexer->lookahead == 'f') {
      lexer->mark_end(lexer);
      char word[8];
      int n = 0;
      while (n < 7 && is_alpha_ascii(lexer->lookahead)) {
        word[n++] = (char)lexer->lookahead;
        advance(lexer);
      }
      word[n] = 0;
      if (((n == 5 && memcmp(word, "catch", 5) == 0)
           || (n == 7 && memcmp(word, "finally", 7) == 0))
          && !is_id_rest(lexer->lookahead)) {
        scanner->pending_newline = false;
        return false;
      }
    }
    scanner->pending_newline = false;
    if (valid_symbols[NEWLINE]) {
      lexer->result_symbol = NEWLINE;
      return true;
    }
  }

  /* Layout tokens have to be considered before the whitespace skip —
     they're triggered by newlines and leading-line indentation. */
  if (valid_symbols[INDENT] || valid_symbols[DEDENT] || valid_symbols[NEWLINE]) {
    if (scan_layout(scanner, lexer, valid_symbols)) return true;
  }

  /* Skip plain whitespace so the rest of the scanner's view matches
     the lexer's. */
  while (lexer->lookahead == ' '
      || lexer->lookahead == '\t'
      || lexer->lookahead == '\r'
      || lexer->lookahead == '\n') {
    skip(lexer);
  }

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
  struct Scanner *s = ts_calloc(1, sizeof(struct Scanner));
  array_init(&s->indents);
  array_push(&s->indents, 0);
  s->dedent_target = -1;
  return s;
}

void tree_sitter_lean_external_scanner_destroy(void *payload) {
  struct Scanner *s = payload;
  array_delete(&s->indents);
  ts_free(s);
}

unsigned tree_sitter_lean_external_scanner_serialize(void *payload, char *buffer) {
  struct Scanner *s = payload;
  /* Always serialize pending_newline as 0: the flag is a one-shot
     transient consumed by the next scan, and the runtime only
     serializes after scan() returns true — persisting a 1 would let a
     checkpoint resurrect it at an unrelated position and fire a
     spurious NEWLINE (the do-block catch/finally regression). */
  buffer[0] = 0;
  /* dedent_target IS serialized: it is a real outstanding obligation
     bound to the indents stack below, so a checkpoint must replay
     both together.  0xFFFF = no pending dedent. */
  uint16_t dt = s->dedent_target < 0 ? 0xFFFFu : (uint16_t)s->dedent_target;
  memcpy(buffer + 1, &dt, sizeof dt);
  unsigned offset = 3;
  for (uint32_t i = 0;
       i < s->indents.size && offset + sizeof(uint16_t) <= TREE_SITTER_SERIALIZATION_BUFFER_SIZE;
       i++) {
    uint16_t v = *array_get(&s->indents, i);
    memcpy(buffer + offset, &v, sizeof v);
    offset += sizeof v;
  }
  return offset;
}

void tree_sitter_lean_external_scanner_deserialize(void *payload,
                                                   const char *buffer,
                                                   unsigned length) {
  struct Scanner *s = payload;
  array_clear(&s->indents);
  /* pending_newline is deliberately NOT restored: it is a one-shot
     flag consumed by the first scan after a dedent.  The runtime only
     serializes scanner state after scan() returns true, so a flag
     cleared inside a return-false call would never be persisted —
     restoring it here would resurrect it at every later position and
     fire a spurious NEWLINE wherever one happens to be valid.  A
     zero-length deserialize is a full reset, so it clears the flag. */
  if (length == 0) {
    s->pending_newline = false;
    s->dedent_target = -1;
  }
  s->dedent_target = -1;
  if (length >= 3) {
    uint16_t dt;
    memcpy(&dt, buffer + 1, sizeof dt);
    s->dedent_target = dt == 0xFFFFu ? -1 : (int32_t)dt;
  }
  unsigned offset = 3;
  while (offset + sizeof(uint16_t) <= length) {
    uint16_t v;
    memcpy(&v, buffer + offset, sizeof v);
    array_push(&s->indents, v);
    offset += sizeof v;
  }
  if (s->indents.size == 0) array_push(&s->indents, 0);
}

bool tree_sitter_lean_external_scanner_scan(void *payload, TSLexer *lexer,
                                            const bool *valid_symbols) {
  return scan((struct Scanner *)payload, lexer, valid_symbols);
}
