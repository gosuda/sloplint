// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License. See the LICENSE file in the project root for full license information.

#include "tree_sitter/parser.h"
#include <wctype.h>

enum TOKEN_TYPE {
    STATEMENT_TERMINATOR,
    BARE_DASHES,
    METHOD_BLOCK_HEAD,
    LOOSE_DOLLAR,
    HERE_STRING_NEWLINE,
    ELSE_BRANCH,
    ELSEIF_BRANCH,
    COMMENTED_ARGUMENT
};

/* --- API --- */

void *tree_sitter_powershell_external_scanner_create();

void tree_sitter_powershell_external_scanner_destroy(void *p);

unsigned tree_sitter_powershell_external_scanner_serialize(void *payload, char *buffer);

void tree_sitter_powershell_external_scanner_deserialize(void *payload, const char *buffer, unsigned length);

bool tree_sitter_powershell_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols);

/* --- Internal Functions --- */

static void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

// Match PowerShell CharExtensions.IsWhitespace without depending on the C locale.
// CR and LF are statement boundaries and are handled separately.
static bool is_powershell_space(int32_t c)
{
    return c == ' ' || c == '\t' || c == '\v' || c == '\f' ||
        c == 0x0085 || c == 0x00A0 || c == 0x1680 ||
        (c >= 0x2000 && c <= 0x200A) || c == 0x2028 ||
        c == 0x2029 || c == 0x202F || c == 0x205F || c == 0x3000;
}

// Native keyword lookup occurs only after the complete generic token.
// Punctuation such as '.', '/', '#', quotes and format marks stays in the word.
static bool ends_powershell_word(int32_t c)
{
    return c == 0 || is_powershell_space(c) || c == '\r' || c == '\n' ||
        c == '&' || c == '(' || c == ')' || c == ',' || c == ';' ||
        c == '{' || c == '|' || c == '}';
}

// Consume lookahead only. The callback owns the zero-width token's mark_end.
static bool scan_conditional_branch(TSLexer *lexer, const bool *valid_symbols)
{
    static const char word[] = "else";
    for (int i = 0; word[i] != 0; i++) {
        int32_t c = lexer->lookahead;
        if (c >= 'A' && c <= 'Z') c += 'a' - 'A';
        if (c != word[i]) return false;
        skip(lexer);
    }
    enum TOKEN_TYPE symbol = ELSE_BRANCH;
    if (lexer->lookahead == 'i' || lexer->lookahead == 'I') {
        skip(lexer);
        if (lexer->lookahead != 'f' && lexer->lookahead != 'F') return false;
        skip(lexer);
        symbol = ELSEIF_BRANCH;
    }
    if (!valid_symbols[symbol] || !ends_powershell_word(lexer->lookahead)) return false;
    lexer->result_symbol = symbol;
    return true;
}

// This prefix selects only the existing named-parameter and dash-format
// word paths. Bare dashes and negative expressions keep their normal grammar.
// Failure returns from the callback; no later probe reuses this cursor.
static bool scan_commented_argument(TSLexer *lexer)
{
    unsigned dashes = 0;
    while (lexer->lookahead == '-') {
        ++dashes;
        skip(lexer);
    }
    int32_t c = lexer->lookahead;
    // PARAMETER_WORD can use an extra dash as its mandatory first atom.
    // Bare '-' and '--' remain owned by their existing external token.
    bool named = dashes >= 3 || (dashes >= 2 && c >= '0' && c <= '9') || (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
        c == '_' || c == '?' || c == 0x200B || c == 0xFEFF || c == 0x2060;
    if (c == '`') {
        skip(lexer);
        c = lexer->lookahead;
        named = c != 0 && c != '\r' && c != '\n';
    }
    if (!named) return false;
    lexer->result_symbol = COMMENTED_ARGUMENT;
    return true;
}

static bool scan_statement_terminator(void *payload, TSLexer *lexer, const bool *valid_symbols, bool comment_at_entry)
{
    bool terminator_valid = valid_symbols[STATEMENT_TERMINATOR];
    bool branch_valid = valid_symbols[ELSE_BRANCH] || valid_symbols[ELSEIF_BRANCH];
    bool commented_argument_valid = comment_at_entry && valid_symbols[COMMENTED_ARGUMENT];
    if (!terminator_valid && !branch_valid && !commented_argument_valid) return false;
    lexer->result_symbol = STATEMENT_TERMINATOR;
    // Comments and whitespace are lookahead; the callback marked the empty token.
    bool saw_newline = false;
    bool pipe_continuation_blocked = false;
    bool line_has_comment = false;
    for (;;) {
        int32_t c = lexer->lookahead;
        if (c == 0 || c == '}' || c == ';' || c == ')') return terminator_valid;
        if (c == '\r' || c == '\n') {
            pipe_continuation_blocked = pipe_continuation_blocked || (saw_newline && !line_has_comment);
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
            if (lexer->lookahead != '#') return terminator_valid && saw_newline;
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
            // A later escape blocks a leading pipe, without hiding branch lookahead.
            pipe_continuation_blocked = pipe_continuation_blocked || saw_newline;
            skip(lexer);
            if (lexer->lookahead == '\r') {
                skip(lexer);
                if (lexer->lookahead == '\n') skip(lexer);
            } else if (lexer->lookahead == '\n') {
                skip(lexer);
            } else {
                return terminator_valid && saw_newline;
            }
            continue;
        }
        if (commented_argument_valid && !saw_newline && c == '-') {
            return scan_commented_argument(lexer);
        }
        if (branch_valid && scan_conditional_branch(lexer, valid_symbols)) return true;
        if (!terminator_valid || !saw_newline) return false;
        return c != '|' || pipe_continuation_blocked;
    }
}

// A bare `-` or `--` argument: `--` ends option parsing for a native command
// (`git log -- path`, `cargo test -- --nocapture`) and `-` names stdin
// (`tar -xf -`). `--` is also the decrement operator and `-` the unary minus,
// so the lexer cannot tell them apart by length or precedence: the token is
// recognised only when the dashes are followed by whitespace, `(` or `{`,
// a command separator, or the end of the command.
// Use the native whitespace set on both sides; format marks remain word text.
static bool scan_bare_dashes(void *payload, TSLexer *lexer, const bool *valid_symbols)
{
    if (!valid_symbols[BARE_DASHES]) return false;

    if (lexer->lookahead != '-') return false;
    lexer->advance(lexer, false);
    if (lexer->lookahead == '-') lexer->advance(lexer, false);
    lexer->mark_end(lexer);

    if (is_powershell_space(lexer->lookahead)) {
        lexer->result_symbol = BARE_DASHES;
        return true;
    }
    switch (lexer->lookahead) {
        case 0:
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
    if (valid_symbols[LOOSE_DOLLAR] && lexer->lookahead == '$') {
        return scan_loose_dollar(payload, lexer, valid_symbols);
    }
    // Keep empty boundary tokens at callback entry, before shared lookahead.
    bool comment_at_entry = lexer->lookahead == '<';
    lexer->mark_end(lexer);
    while (is_powershell_space(lexer->lookahead)) skip(lexer);
    // A failed consuming probe must return to tree-sitter before another probe.
    if (valid_symbols[BARE_DASHES] && lexer->lookahead == '-') {
        return scan_bare_dashes(payload, lexer, valid_symbols);
    }
    return scan_statement_terminator(payload, lexer, valid_symbols, comment_at_entry);
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
