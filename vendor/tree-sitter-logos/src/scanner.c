#include "tree_sitter/array.h"
#include "tree_sitter/parser.h"

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

enum TokenType {
    INTERP_STRING,
    NEWLINE_COMMENT,
};

// LoGOs interpolated string literal: `$"..."`.
// The literal may contain `{expr}` interpolations. Inside an interpolation
// brace run, nested strings (including further `$"..."` literals), nested
// braces, escapes and raw newlines may appear, so the end quote cannot be
// found with a regular expression. This scanner tracks brace depth and
// string nesting instead.
//
// `{{` and `}}` are literal braces in the text portions.

static inline void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

// Bound on nested `$"..."` literals inside one interpolation. Each nesting
// level recurses through scan_interp_expr and scan_interp_body, so without a
// bound a hostile input can exhaust the scanner stack and abort the lint job.
#define INTERP_MAX_DEPTH 32

// Scans the interior of one `$"..."` literal. On entry the lexer is
// positioned just after the opening `"`. Returns NULL on success (lexer
// parked just past the closing quote), or non-NULL on failure.
static const char *scan_interp_body(TSLexer *lexer, int depth);

// The lexer is positioned on the opening quote of a plain string or rune
// literal (`"`, '`' or `'`). Returns NULL on success (lexer parked just past
// the closing quote), or non-NULL on failure.
static const char *skip_quoted(TSLexer *lexer) {
    int32_t quote = lexer->lookahead;
    advance(lexer);
    for (;;) {
        int32_t c = lexer->lookahead;
        if (c == 0) {
            return "eof";
        }
        if (c == '\\' && quote != '`') {
            advance(lexer);
            if (lexer->lookahead == 0) {
                return "eof";
            }
            advance(lexer);
            continue;
        }
        advance(lexer);
        if (c == quote) {
            return NULL;
        }
    }
}

// Scans one `{expr}` interpolation body. On entry the lexer is positioned
// just after the `{`. Returns NULL on success (lexer parked just past the
// matching `}`), or non-NULL on failure.
static const char *scan_interp_expr(TSLexer *lexer, int depth) {
    int32_t nesting = 1;
    for (;;) {
        int32_t d = lexer->lookahead;
        if (d == 0) {
            return "eof";
        }
        if (d == '{') {
            nesting++;
            advance(lexer);
            continue;
        }
        if (d == '}') {
            nesting--;
            advance(lexer);
            if (nesting == 0) {
                return NULL;
            }
            continue;
        }
        if (d == '/') {
            // Skip `//` line and `/* */` block comments so braces and
            // quotes inside them stay out of the balance. A lone `/`
            // (division) is already consumed here and needs no handling.
            advance(lexer);
            if (lexer->lookahead == '/') {
                advance(lexer);
                while (lexer->lookahead != 0 && lexer->lookahead != '\n') {
                    advance(lexer);
                }
                continue;
            }
            if (lexer->lookahead == '*') {
                advance(lexer);
                int32_t prev = 0;
                for (;;) {
                    int32_t c = lexer->lookahead;
                    if (c == 0) {
                        return "eof";
                    }
                    advance(lexer);
                    if (prev == '*' && c == '/') {
                        break;
                    }
                    prev = c;
                }
                continue;
            }
            continue;
        }
        if (d == '"' || d == '`' || d == '\'') {
            if (skip_quoted(lexer) != NULL) {
                return "unterminated string";
            }
            continue;
        }
        if (d == '$') {
            // Possibly a nested `$"..."` literal.
            advance(lexer);
            if (lexer->lookahead == '"') {
                if (depth + 1 > INTERP_MAX_DEPTH) {
                    return "nesting too deep";
                }
                advance(lexer);
                if (scan_interp_body(lexer, depth + 1) != NULL) {
                    return "unterminated interp";
                }
            }
            continue;
        }
        if (d == '\\') {
            advance(lexer);
            if (lexer->lookahead == 0) {
                return "eof";
            }
            advance(lexer);
            continue;
        }
        advance(lexer);
    }
}

static const char *scan_interp_body(TSLexer *lexer, int depth) {
    for (;;) {
        int32_t c = lexer->lookahead;
        if (c == 0) {
            return "eof";
        }
        if (c == '"') {
            advance(lexer);
            return NULL;
        }
        if (c == '\\') {
            advance(lexer);
            if (lexer->lookahead == 0) {
                return "eof";
            }
            advance(lexer);
            continue;
        }
        if (c == '{') {
            advance(lexer);
            if (lexer->lookahead == '{') {
                advance(lexer);
                continue;
            }
            if (scan_interp_expr(lexer, depth) != NULL) {
                return "unterminated interpolation";
            }
            continue;
        }
        if (c == '}') {
            advance(lexer);
            if (lexer->lookahead == '}') {
                advance(lexer);
            }
            continue;
        }
        advance(lexer);
    }
}

// A block comment that starts on the current line (only spaces, tabs and
// other non-newline whitespace since the last token) and contains a newline.
// Go inserts a semicolon after the preceding token in that case, so the
// comment separates grouped declarations where a single-line block comment
// does not. Newlines are never skipped here: after a real newline the
// internal `newline` terminal wins and the comment lexes as an ordinary
// `comment` extra, so leading and trailing comments stay visible in the
// tree. Unterminated input is not a comment; fail and let the internal
// lexer report it.
static bool scan_newline_comment(TSLexer *lexer) {
    for (;;) {
        int32_t c = lexer->lookahead;
        if (c != ' ' && c != '\t' && c != '\r' && c != '\v' && c != '\f') {
            break;
        }
        lexer->advance(lexer, true);
    }
    if (lexer->lookahead != '/') {
        return false;
    }
    advance(lexer);
    if (lexer->lookahead != '*') {
        return false;
    }
    advance(lexer);
    bool saw_newline = false;
    int32_t prev = 0;
    for (;;) {
        int32_t c = lexer->lookahead;
        if (c == 0) {
            return false;
        }
        advance(lexer);
        if (c == '\n') {
            saw_newline = true;
        }
        if (prev == '*' && c == '/') {
            return saw_newline;
        }
        prev = c;
    }
}

bool tree_sitter_logos_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
    (void)payload;

    if (valid_symbols[NEWLINE_COMMENT] && scan_newline_comment(lexer)) {
        lexer->result_symbol = NEWLINE_COMMENT;
        lexer->mark_end(lexer);
        return true;
    }
    // A failed comment scan restores the lexer position, so fall through
    // to the interpolated-string scan: both tokens can be valid together
    // (for example after `return`), and a non-comment must not hide `$"`.
    if (!valid_symbols[INTERP_STRING]) {
        return false;
    }
    // The parser calls this scanner before extras are skipped and does
    // not call it again after internal extras skipping, so the scanner
    // skips whitespace itself. The parser restores the lexer position
    // when this scan returns false, so the skip is speculative.
    for (;;) {
        int32_t c = lexer->lookahead;
        if (c != ' ' && c != '\t' && c != '\n' && c != '\r') {
            break;
        }
        lexer->advance(lexer, true);
    }
    if (lexer->lookahead != '$') {
        return false;
    }
    advance(lexer);
    if (lexer->lookahead != '"') {
        return false;
    }
    advance(lexer);
    if (scan_interp_body(lexer, 0) != NULL) {
        return false;
    }
    lexer->result_symbol = INTERP_STRING;
    lexer->mark_end(lexer);
    return true;
}

unsigned tree_sitter_logos_external_scanner_serialize(void *payload, char *buffer) {
    (void)payload;
    (void)buffer;
    return 0;
}

void tree_sitter_logos_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
    (void)payload;
    (void)buffer;
    (void)length;
}

void *tree_sitter_logos_external_scanner_create() {
    return NULL;
}

void tree_sitter_logos_external_scanner_destroy(void *payload) {
    (void)payload;
}
