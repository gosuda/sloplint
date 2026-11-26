// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License. See the LICENSE file in the project root for full license information.

#include "tree_sitter/parser.h"
#include <wctype.h>

enum TOKEN_TYPE {
    STATEMENT_TERMINATOR,
    BARE_DASHES,
    METHOD_BLOCK_HEAD,
    LOOSE_DOLLAR,
    HERE_STRING_NEWLINE
};

/* --- API --- */

void *tree_sitter_powershell_external_scanner_create();

void tree_sitter_powershell_external_scanner_destroy(void *p);

unsigned tree_sitter_powershell_external_scanner_serialize(void *payload, char *buffer);

void tree_sitter_powershell_external_scanner_deserialize(void *payload, const char *buffer, unsigned length);

bool tree_sitter_powershell_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols);

/* --- Internal Functions --- */

static void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

// sloplint: true when the lookahead starts the keyword `else` or `elseif`,
// as a whole word. The lookahead is consumed; the caller has already marked
// the end of the zero-width terminator token.
static bool at_else_keyword(TSLexer *lexer)
{
    static const char word[] = "else";
    for (int i = 0; word[i] != 0; i++) {
        if (towlower(lexer->lookahead) != (wint_t)word[i]) return false;
        skip(lexer);
    }
    if (towlower(lexer->lookahead) == 'i') {
        skip(lexer);
        if (towlower(lexer->lookahead) != 'f') return false;
        skip(lexer);
    }
    wint_t c = lexer->lookahead;
    return !(iswalnum(c) || c == '_' || c == '-');
}

// Match PowerShell CharExtensions.IsWhitespace without depending on the C locale.
// CR and LF are statement boundaries and are handled separately.
static bool is_powershell_space(int32_t c)
{
    return c == ' ' || c == '\t' || c == '\v' || c == '\f' ||
        c == 0x0085 || c == 0x00A0 || c == 0x1680 ||
        (c >= 0x2000 && c <= 0x200A) || c == 0x2028 ||
        c == 0x2029 || c == 0x202F || c == 0x205F || c == 0x3000;
}

static bool scan_statement_terminator(void *payload, TSLexer *lexer, const bool *valid_symbols)
{
    if (!valid_symbols[STATEMENT_TERMINATOR]) return false;
    lexer->result_symbol = STATEMENT_TERMINATOR;
    // The token is empty; comments and whitespace are only lookahead.
    lexer->mark_end(lexer);
    bool saw_newline = false;
    bool saw_blank_line = false;
    bool line_has_comment = false;
    for (;;) {
        int32_t c = lexer->lookahead;
        if (c == 0 || c == '}' || c == ';' || c == ')') return true;
        if (c == '\r' || c == '\n') {
            saw_blank_line = saw_blank_line || (saw_newline && !line_has_comment);
            saw_newline = true;
            line_has_comment = false;
            skip(lexer);
            if (c == '\r' && lexer->lookahead == '\n') skip(lexer);
            continue;
        }
        if (is_powershell_space(c)) {
            skip(lexer);
            continue;
        }
        if (c == '#') {
            line_has_comment = true;
            while (lexer->lookahead != 0 && lexer->lookahead != '\r' &&
                   lexer->lookahead != '\n') skip(lexer);
            continue;
        }
        if (c == '<') {
            skip(lexer);
            if (lexer->lookahead != '#') return saw_newline;
            line_has_comment = true;
            skip(lexer);
            // PowerShell block comments end at the first #>, without nesting.
            while (lexer->lookahead != 0) {
                if (lexer->lookahead == '#') {
                    skip(lexer);
                    if (lexer->lookahead == '>') {
                        skip(lexer);
                        break;
                    }
                } else {
                    skip(lexer);
                }
            }
            continue;
        }
        if (c == '`') {
            skip(lexer);
            if (lexer->lookahead == '\r') {
                skip(lexer);
                if (lexer->lookahead == '\n') skip(lexer);
            } else if (lexer->lookahead == '\n') {
                skip(lexer);
            } else {
                return false;
            }
            continue;
        }
        if (!saw_newline) return false;
        if (c == '|' && !saw_blank_line) return false;
        return !at_else_keyword(lexer);
    }
}

// A bare `-` or `--` argument: `--` ends option parsing for a native command
// (`git log -- path`, `cargo test -- --nocapture`) and `-` names stdin
// (`tar -xf -`). `--` is also the decrement operator and `-` the unary minus,
// so the lexer cannot tell them apart by length or precedence: the token is
// recognised only when the dashes are followed by whitespace, `(` or `{`,
// a command separator, or the end of the command.
// sloplint: PowerShell treats non-breaking and zero-width spaces as
// horizontal whitespace (see grammar extras). The external scanner skips
// exactly those plus ASCII space and tab, never line breaks, so a `--`
// after ` \u00A0` is still an option terminator.
static bool is_horizontal_space(int32_t c)
{
    return c == ' ' || c == '\t' || c == 0x00A0 || c == 0xFEFF || c == 0x200B || c == 0x2060;
}

static bool scan_bare_dashes(void *payload, TSLexer *lexer, const bool *valid_symbols)
{
    if (!valid_symbols[BARE_DASHES]) return false;

    while (is_horizontal_space(lexer->lookahead)) skip(lexer);
    if (lexer->lookahead != '-') return false;
    lexer->advance(lexer, false);
    if (lexer->lookahead == '-') lexer->advance(lexer, false);
    lexer->mark_end(lexer);

    switch (lexer->lookahead) {
        case 0:
        case ' ':
        case '\t':
        case 0x00A0:
        case 0xFEFF:
        case 0x200B:
        case 0x2060:
        case '\r':
        case '\n':
        case ';':
        case '|':
        case '&':
        case '(':
        case ')':
        case '{':
        case '}':
            lexer->result_symbol = BARE_DASHES;
            return true;
        default:
            return false;
    }
}

/* --- API Implementation --- */

// sloplint: `.Name` or `::Name` when `{` follows the name at once, as in
// $a.Where{ $_ }. The token ends before the brace. Anything else is left to
// the internal lexer, which re-reads the input from the start.
static bool scan_method_block_head(TSLexer *lexer)
{
    if (lexer->lookahead == ':') {
        lexer->advance(lexer, false);
        if (lexer->lookahead != ':') return false;
        lexer->advance(lexer, false);
    } else if (lexer->lookahead == '.') {
        lexer->advance(lexer, false);
    } else {
        return false;
    }
    if (!(iswalpha(lexer->lookahead) || lexer->lookahead == '_')) return false;
    while (iswalnum(lexer->lookahead) || lexer->lookahead == '_') {
        lexer->advance(lexer, false);
    }
    if (lexer->lookahead != '{') return false;
    lexer->mark_end(lexer);
    lexer->result_symbol = METHOD_BLOCK_HEAD;
    return true;
}
// sloplint: a lone `$` inside an expandable string, when it does not open
// `${`, `$(` or a variable. The internal lexer cannot express `$` not
// followed by `{` (no lookahead), so an external token covers the lone case.
// `${name` without `}` and `${}` then have no valid token and raise an error
// as PowerShell does. Variables and sub-expressions stay with the internal
// lexer. A plain dollar before whitespace, a backslash or a backtick ends
// here, leaving the following here-string closing newline untouched.
static bool scan_loose_dollar(void *payload, TSLexer *lexer, const bool *valid_symbols)
{
    if (!valid_symbols[LOOSE_DOLLAR]) return false;
    if (lexer->lookahead != '$') return false;
    lexer->advance(lexer, false);
    int32_t next = lexer->lookahead;
    if (next == '{' || next == '(') return false;
    if (next == '$' || next == '^' || next == '?') return false;
    if (iswalnum(next) || next == '_') return false;
    lexer->mark_end(lexer);
    lexer->result_symbol = LOOSE_DOLLAR;
    return true;
}

// sloplint: only separate a here-string line break when the next line
// starts interpolation. Never take the closing delimiter's line break.
static bool scan_here_string_newline(TSLexer *lexer)
{
    bool saw_newline = false;
    while (lexer->lookahead == '\r' || lexer->lookahead == '\n') {
        if (lexer->lookahead == '\r') {
            lexer->advance(lexer, false);
            if (lexer->lookahead != '\n') return false;
        }
        lexer->advance(lexer, false);
        saw_newline = true;
    }
    if (!saw_newline || lexer->lookahead != '$') return false;
    lexer->mark_end(lexer);
    lexer->result_symbol = HERE_STRING_NEWLINE;
    return true;
}

bool tree_sitter_powershell_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols)
{
    if (valid_symbols[HERE_STRING_NEWLINE] &&
        (lexer->lookahead == '\r' || lexer->lookahead == '\n')) {
        return scan_here_string_newline(lexer);
    }
    if (valid_symbols[METHOD_BLOCK_HEAD] &&
        (lexer->lookahead == '.' || lexer->lookahead == ':')) {
        return scan_method_block_head(lexer);
    }
    if (scan_loose_dollar(payload, lexer, valid_symbols)) return true;
    // The statement terminator consumes nothing, so it must be tried last: when
    // both are valid, a `--` has to win over the empty terminator.
    if (scan_bare_dashes(payload, lexer, valid_symbols)) return true;
    return scan_statement_terminator(payload, lexer, valid_symbols);
}

void *tree_sitter_powershell_external_scanner_create()
{
    return NULL;
}

void tree_sitter_powershell_external_scanner_destroy(void *p)
{
}

unsigned tree_sitter_powershell_external_scanner_serialize(void *payload, char *buffer)
{
    return 0;
}

void tree_sitter_powershell_external_scanner_deserialize(void *payload, const char *buffer, unsigned length)
{
}
