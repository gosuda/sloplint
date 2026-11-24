// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License. See the LICENSE file in the project root for full license information.

#include "tree_sitter/parser.h"
#include <wctype.h>

enum TOKEN_TYPE {
    STATEMENT_TERMINATOR,
    BARE_DASHES,
    METHOD_BLOCK_HEAD
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

static bool scan_statement_terminator(void *payload, TSLexer *lexer, const bool *valid_symbols)
{
    if (valid_symbols[STATEMENT_TERMINATOR]) {
        lexer->result_symbol = STATEMENT_TERMINATOR;
        // This token has no characters -- everything is lookahead to determine its existence
        lexer->mark_end(lexer);

        bool saw_newline = false;
        for (;;) {
            if (lexer->lookahead == 0) return true;
            if (lexer->lookahead == '}') return true;
            if (lexer->lookahead == ';') return true;
            if (lexer->lookahead == ')') return true;
            if (lexer->lookahead == '\n') saw_newline = true;
            else if (lexer->lookahead == '`') {
                // A backtick before a line break joins the lines, so it is
                // white space here: `$x -replace "a", "b" `` newline newline `}`.
                skip(lexer);
                if (lexer->lookahead == '\r') skip(lexer);
                if (lexer->lookahead != '\n') return false;
            }
            else if (!iswspace(lexer->lookahead)) {
                // A line break ends the statement, except when the next
                // line continues an if statement: `}` newline `else {`.
                if (saw_newline) return !at_else_keyword(lexer);
                return false;
            }
            skip(lexer);
        }
    }

    return false;
}

// A bare `-` or `--` argument: `--` ends option parsing for a native command
// (`git log -- path`, `cargo test -- --nocapture`) and `-` names stdin
// (`tar -xf -`). `--` is also the decrement operator and `-` the unary minus,
// so the lexer cannot tell them apart by length or precedence: the token is
// recognised only when the dashes are followed by whitespace or the end of the
// command.
static bool scan_bare_dashes(void *payload, TSLexer *lexer, const bool *valid_symbols)
{
    if (!valid_symbols[BARE_DASHES]) return false;

    while (lexer->lookahead == ' ' || lexer->lookahead == '\t') skip(lexer);
    if (lexer->lookahead != '-') return false;
    lexer->advance(lexer, false);
    if (lexer->lookahead == '-') lexer->advance(lexer, false);
    lexer->mark_end(lexer);

    switch (lexer->lookahead) {
        case 0:
        case ' ':
        case '\t':
        case '\r':
        case '\n':
        case ';':
        case '|':
        case '&':
        case ')':
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

bool tree_sitter_powershell_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols)
{
    if (valid_symbols[METHOD_BLOCK_HEAD] &&
        (lexer->lookahead == '.' || lexer->lookahead == ':')) {
        return scan_method_block_head(lexer);
    }
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
