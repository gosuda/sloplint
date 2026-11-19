#include "tree_sitter/alloc.h"
#include "tree_sitter/parser.h"

#include <assert.h>
#include <string.h>
#include <wctype.h>

enum TokenType { RAW_STRING_DELIMITER, RAW_STRING_CONTENT, MACRO_BRACED_OPEN };

/// Bounds the look-ahead for the macro-brace probe.
#define MACRO_BRACE_SCAN_LIMIT 65536

/// The spec limits delimiters to 16 chars
#define MAX_DELIMITER_LENGTH 16

typedef struct {
    uint8_t delimiter_length;
    wchar_t delimiter[MAX_DELIMITER_LENGTH];
} Scanner;

static inline void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

static inline void reset(Scanner *scanner) {
    scanner->delimiter_length = 0;
    memset(scanner->delimiter, 0, sizeof scanner->delimiter);
}

/// Scan the raw string delimiter in R"delimiter(content)delimiter"
static bool scan_raw_string_delimiter(Scanner *scanner, TSLexer *lexer) {
    if (scanner->delimiter_length > 0) {
        // Closing delimiter: must exactly match the opening delimiter.
        // We already checked this when scanning content, but this is how we
        // know when to stop. We can't stop at ", because R"""hello""" is valid.
        for (int i = 0; i < scanner->delimiter_length; ++i) {
            if (lexer->lookahead != scanner->delimiter[i]) {
                return false;
            }
            advance(lexer);
        }
        reset(scanner);
        return true;
    }

    // Opening delimiter: record the d-char-sequence up to (.
    // d-char is any basic character except parens, backslashes, and spaces.
    for (;;) {
        if (scanner->delimiter_length >= MAX_DELIMITER_LENGTH || lexer->eof(lexer) || lexer->lookahead == '\\' ||
            iswspace(lexer->lookahead)) {
            return false;
        }
        if (lexer->lookahead == '(') {
            // Rather than create a token for an empty delimiter, we fail and
            // let the grammar fall back to a delimiter-less rule.
            return scanner->delimiter_length > 0;
        }
        scanner->delimiter[scanner->delimiter_length++] = lexer->lookahead;
        advance(lexer);
    }
}

/// Scan the raw string content in R"delimiter(content)delimiter"
static bool scan_raw_string_content(Scanner *scanner, TSLexer *lexer) {
    // The progress made through the delimiter since the last ')'.
    // The delimiter may not contain ')' so a single counter suffices.
    for (int delimiter_index = -1;;) {
        // If we hit EOF, consider the content to terminate there.
        // This forms an incomplete raw_string_literal, and models the code
        // well.
        if (lexer->eof(lexer)) {
            lexer->mark_end(lexer);
            return true;
        }

        if (delimiter_index >= 0) {
            if (delimiter_index == scanner->delimiter_length) {
                if (lexer->lookahead == '"') {
                    return true;
                }
                delimiter_index = -1;
            } else {
                if (lexer->lookahead == scanner->delimiter[delimiter_index]) {
                    delimiter_index += 1;
                } else {
                    delimiter_index = -1;
                }
            }
        }

        if (delimiter_index == -1 && lexer->lookahead == ')') {
            // The content doesn't include the )delimiter" part.
            // We must still scan through it, but exclude it from the token.
            lexer->mark_end(lexer);
            delimiter_index = 0;
        }

        advance(lexer);
    }
}

static bool is_ident_start(int32_t c) {
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_' || c > 127;
}

static bool is_ident_char(int32_t c) {
    return is_ident_start(c) || (c >= '0' && c <= '9');
}

// Consume the rest of a "..." string (the opening quote is already
// consumed). Stops after the closing quote or at EOF.
static void skip_dq(TSLexer *lexer) {
    while (!lexer->eof(lexer)) {
        if (lexer->lookahead == '\\') {
            advance(lexer);
            if (!lexer->eof(lexer)) advance(lexer);
            continue;
        }
        if (lexer->lookahead == '"') {
            advance(lexer);
            break;
        }
        advance(lexer);
    }
}

// Consume a '...' char literal inside a region whose contents don't
// matter (skipped lambda bodies, skipped statement expressions): the
// opening quote is already consumed; consume the literal if it
// completes, else just one more char (digit separator like 1'000).
static void skip_sq(TSLexer *lexer) {
    if (lexer->lookahead == '\\') {
        advance(lexer);
        if (!lexer->eof(lexer)) advance(lexer);
    } else if (!lexer->eof(lexer)) {
        advance(lexer);
    }
    if (lexer->lookahead == '\'') advance(lexer);
}

// Consume a raw string after `R"` (the R and the quote are already
// consumed): delimiter up to `(`, then contents up to `)delim"`.
// Malformed opens bail out having consumed only delimiter-legal chars
// (no `;`, `{`, `}`), which is safe to resume scanning from.
static void skip_raw_string(TSLexer *lexer) {
    char delim[16];
    int dlen = 0;
    while (dlen < 16) {
        int32_t d = lexer->lookahead;
        if (d == '(') break;
        if (d == ')' || d == '"' || d == ' ' || d == '\\' || d == '\t' ||
            d == '\n' || d == '\r' || d == '\f' || d == '\v' ||
            lexer->eof(lexer)) {
            return;
        }
        delim[dlen++] = (char)d;
        advance(lexer);
    }
    if (lexer->lookahead != '(') return;
    advance(lexer);
    while (!lexer->eof(lexer)) {
        if (lexer->lookahead == ')') {
            advance(lexer);
            int i = 0;
            while (i < dlen && lexer->lookahead == delim[i]) {
                advance(lexer);
                i++;
            }
            if (i == dlen && lexer->lookahead == '"') {
                advance(lexer);
                return;
            }
            continue;
        }
        advance(lexer);
    }
}

// Skip // and /* */ comments; returns false when the `/` was division.
static bool skip_comment(TSLexer *lexer) {
    advance(lexer);
    if (lexer->lookahead == '/') {
        while (!lexer->eof(lexer) && lexer->lookahead != '\n') advance(lexer);
        return true;
    }
    if (lexer->lookahead == '*') {
        advance(lexer);
        while (!lexer->eof(lexer)) {
            if (lexer->lookahead == '*') {
                advance(lexer);
                if (lexer->lookahead == '/') {
                    advance(lexer);
                    break;
                }
                continue;
            }
            advance(lexer);
        }
        return true;
    }
    return false;
}

// Skip a `({ ... })` GNU statement expression: the `(` and `{` are
// already consumed; runs to the paren matching the opener. Contents
// (including interior `;`) are ignored — a statement expression is
// legal inside an initializer, so its semicolons are no macro signal.
static void skip_paren_body(TSLexer *lexer) {
    int parens = 1;
    while (!lexer->eof(lexer) && parens > 0) {
        int32_t c = lexer->lookahead;
        if (c == '"') { advance(lexer); skip_dq(lexer); continue; }
        if (c == '\'') { advance(lexer); skip_sq(lexer); continue; }
        if (c == 'R') {
            advance(lexer);
            if (lexer->lookahead == '"') { advance(lexer); skip_raw_string(lexer); }
            continue;
        }
        if (c == '/') { if (skip_comment(lexer)) continue; continue; }
        if (c == '(') parens++;
        if (c == ')') parens--;
        advance(lexer);
    }
}

// Skip a `{ ... }` region whose contents must not be probed (a lambda
// body inside a braced initializer): the `{` is already consumed.
static void skip_brace_body(TSLexer *lexer) {
    int depth = 1;
    while (!lexer->eof(lexer) && depth > 0) {
        int32_t c = lexer->lookahead;
        if (c == '"') { advance(lexer); skip_dq(lexer); continue; }
        if (c == '\'') { advance(lexer); skip_sq(lexer); continue; }
        if (c == 'R') {
            advance(lexer);
            if (lexer->lookahead == '"') { advance(lexer); skip_raw_string(lexer); }
            continue;
        }
        if (c == '/') { if (skip_comment(lexer)) continue; continue; }
        if (c == '{') depth++;
        if (c == '}') depth--;
        advance(lexer);
    }
}

static bool is_block_keyword(const char *word, int len) {
    // Control-flow keywords that can open a semicolon-free statement
    // body (`MACRO { if (c) f(); }`, `{ return; }`, ...) and can never
    // begin an initializer-list element. Declaration/specifier words
    // are excluded on purpose: `const`/`static`/`mutable`/... appear
    // legitimately inside initializers (`T{sizeof(const int)}`,
    // `T{std::move(static_)})`), and a decl-statement macro
    // (`MACRO { int x = 0; }`) is already proven by the `;` probe.
    // Also excluded: expression-valid keywords (new, delete, throw,
    // noexcept, sizeof, decltype, requires, co_await, co_yield, this,
    // *_cast, operator), elaborated-type-specifier leads
    // (struct/class/union/enum/typename — `struct S(x)` is a legal
    // functional cast), contextual identifiers (import/module/export/
    // concept/final/override) and primitive types (int, char, ...
    // `int(x)` casts).
    // `namespace` is the one declaration keyword safe to include: it
    // opens a semicolon-free body and can never start an expression.
    static const char *kw[] = {
        "if", "else", "do", "for", "while", "switch", "case", "default",
        "try", "catch", "return", "break", "continue", "goto",
        "co_return", "asm", "__asm__", "__asm", "static_assert",
        "namespace",
    };
    for (size_t i = 0; i < sizeof(kw) / sizeof(kw[0]); i++) {
        if ((int)strlen(kw[i]) == len && memcmp(word, kw[i], len) == 0) return true;
    }
    return false;
}

/// `{` that opens a macro block (`ON_SCOPE_EXIT { f(); };`,
/// `R_TRY_CATCH(e) { try { ... } catch { ... } }`,
/// `MACRO { if (ok) {} }`): it is a macro block when the region holds
/// statement evidence — a `;` or a statement-only keyword — before the
/// matching `}`. Braced initializer lists (`Type{arg}`) contain neither
/// outside lambdas and `({ ... })` statement expressions, so they fall
/// back to the regular `{` token.
/// The token covers just the `{` itself; the look-ahead chars are
/// re-lexed afterwards via mark_end.
static bool scan_macro_braced_open(TSLexer *lexer) {
    // External scans start at the raw position — skip whitespace up
    // to the `{` itself (`NAME { ... }` has a space).
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
           lexer->lookahead == '\n' || lexer->lookahead == '\r') {
        lexer->advance(lexer, true);
    }
    if (lexer->lookahead != '{') {
        return false;
    }
    advance(lexer);
    lexer->mark_end(lexer);

    int depth = 1;
    // `[captures] (params) qualifiers { body }` inside a braced init is a
    // lambda: its body's `;`/keywords are not macro evidence. A `]` sets
    // maybe_lambda; `(`, `)`, qualifier idents and trailing-return-type
    // chars keep it; the clearing set drops it.
    bool maybe_lambda = false;
    int pdepth = 0;
    // A `]` has already closed a bracket run: commas after it live in a
    // lambda trailer (`-> std::pair<int,int>`), not an init list, so
    // they must not clear maybe_lambda.
    bool capture_done = false;
    // Inside a numeric literal: `'` is a digit separator (`1'000`,
    // `0xAB'CD`), not a character constant. Set on digits; kept across
    // ident runs that follow them (`xAB`, `e5`, `ul` tails).
    bool after_number = false;
    // `operator[]` is not a lambda capture — when the last identifier
    // scanned was `operator`, a following `]` must not mark maybe_lambda
    // or the operator's `{` body gets skipped as a lambda body and its
    // `;` evidence is lost.
    bool prev_was_operator = false;
    char word[64];
    size_t scanned = 0;
    for (;;) {
        if (lexer->eof(lexer) || scanned++ > MACRO_BRACE_SCAN_LIMIT) {
            return false;
        }
        int32_t c = lexer->lookahead;
        if (c == '"') {
            advance(lexer);
            skip_dq(lexer);
            after_number = false;
            continue;
        }
        if (c == '\'') {
            advance(lexer);
            if (after_number) {
                // Digit separator (1'000, 0xAB'CD): `'` inside a
                // number, not a char literal — keep scanning.
                continue;
            }
            // Char literal: scan to the closing `'`. Multi-char
            // constants ('A;', '}x') are legal C++, so any `;`/`{`/`}`
            // seen before the closing quote is literal content.
            while (!lexer->eof(lexer)) {
                int32_t x = lexer->lookahead;
                if (x == '\\') {
                    advance(lexer);
                    if (!lexer->eof(lexer)) advance(lexer);
                    continue;
                }
                if (x == '\n' || x == '\r') break;  // unterminated
                advance(lexer);
                if (x == '\'') break;
            }
            after_number = false;
            continue;
        }
        if (c == '/') {
            if (skip_comment(lexer)) { after_number = false; continue; }
            // Division: skip_comment already consumed the `/`, so the
            // next lookahead (`"x"` in `base/"x"`) stays live.
            maybe_lambda = false;
            after_number = false;
            continue;
        }
        if (c == '(') {
            advance(lexer);
            while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
                   lexer->lookahead == '\n' || lexer->lookahead == '\r') {
                advance(lexer);
            }
            if (lexer->lookahead == '{') {
                advance(lexer);
                skip_paren_body(lexer);   // GNU ({ stmts }) — ignore contents
                after_number = false;
                continue;
            }
            if (maybe_lambda) pdepth++;
            after_number = false;
            continue;
        }
        if (c == ')') {
            if (pdepth > 0) pdepth--;
            advance(lexer);
            after_number = false;
            continue;
        }
        if (c == '[') {
            advance(lexer);
            if (lexer->lookahead == '[') {
                // `[[attribute]]`: balanced `]`s inside attributes do
                // not end a lambda capture — skip the whole attribute,
                // including quoted arguments (`[[x("a]]b")]]`).
                advance(lexer);
                int closes = 0;
                while (!lexer->eof(lexer) && closes < 2) {
                    if (lexer->lookahead == '"') {
                        advance(lexer);
                        skip_dq(lexer);
                        continue;
                    }
                    if (lexer->lookahead == ']') closes++;
                    else closes = 0;
                    advance(lexer);
                }
                after_number = false;
                continue;
            }
            after_number = false;
            continue;
        }
        if (c == ']') {
            if (!prev_was_operator) {
                maybe_lambda = true;
                capture_done = true;
                pdepth = 0;
            }
            prev_was_operator = false;
            advance(lexer);
            after_number = false;
            continue;
        }
        if (c == '{') {
            if (maybe_lambda && pdepth == 0) {
                advance(lexer);
                skip_brace_body(lexer);   // lambda body — ignore contents
                maybe_lambda = false;
                after_number = false;
                continue;
            }
            depth++;
            maybe_lambda = false;
            advance(lexer);
            after_number = false;
            continue;
        }
        if (c == '}') {
            depth--;
            maybe_lambda = false;
            if (depth == 0) return false;
            advance(lexer);
            after_number = false;
            continue;
        }
        if (c == ';') return true;
        if (is_ident_start(c)) {
            bool was_num = after_number;
            int len = 0;
            while (is_ident_char(lexer->lookahead) && len < 63) {
                word[len++] = (char)lexer->lookahead;
                advance(lexer);
            }
            while (is_ident_char(lexer->lookahead)) advance(lexer);
            word[len] = '\0';
            // Raw string opener: only the exact prefixes R / u8R / LR /
            // uR / UR directly followed by `"` start one — `Rif` or
            // `R + 1` keep scanning as an ordinary identifier.
            if (lexer->lookahead == '"' &&
                ((len == 1 && word[0] == 'R') ||
                 (len == 2 && (word[0] == 'L' || word[0] == 'u' ||
                               word[0] == 'U') && word[1] == 'R') ||
                 (len == 3 && word[0] == 'u' && word[1] == '8' &&
                  word[2] == 'R'))) {
                advance(lexer);  // past `"`
                skip_raw_string(lexer);
                after_number = false;
                continue;
            }
            // Inside `[...]` a possible lambda is collecting its
            // qualifiers (mutable/constexpr/noexcept -> T) — those names
            // must not fire the statement-keyword check.
            if (!maybe_lambda && is_block_keyword(word, len)) return true;
            prev_was_operator = (len == 8 && memcmp(word, "operator", 8) == 0);
            // An ident right after digits is a numeric tail (`0xAB`,
            // `1e5`, `42ul`) — the number continues.
            after_number = was_num;
            continue;
        }
        // Init-clause separators / operators clear the lambda flag only
        // at paren depth 0 and before any `]` closed a capture — inside
        // `[&](int a, int b)` a `,` is part of the parameter list, and
        // in `-> std::pair<int,int>` it lives inside the return type.
        if (pdepth == 0 && !capture_done &&
            (c == ',' || c == '=' || c == '?' || c == '|' || c == '^' ||
             c == '%' || c == '!' || c == '#')) {
            maybe_lambda = false;
        }
        if (c != ' ' && c != '\t' && c != '\n' && c != '\r') {
            prev_was_operator = false;
        }
        advance(lexer);
        after_number = (c >= '0' && c <= '9');
    }
}

void *tree_sitter_cpp_external_scanner_create() {
    Scanner *scanner = (Scanner *)ts_calloc(1, sizeof(Scanner));
    memset(scanner, 0, sizeof(Scanner));
    return scanner;
}

bool tree_sitter_cpp_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
    Scanner *scanner = (Scanner *)payload;

    if (valid_symbols[RAW_STRING_DELIMITER] && valid_symbols[RAW_STRING_CONTENT]) {
        // we're in error recovery
        return false;
    }

    if (valid_symbols[MACRO_BRACED_OPEN]) {
        lexer->result_symbol = MACRO_BRACED_OPEN;
        return scan_macro_braced_open(lexer);
    }

    // No skipping leading whitespace: raw-string grammar is space-sensitive.
    if (valid_symbols[RAW_STRING_DELIMITER]) {
        lexer->result_symbol = RAW_STRING_DELIMITER;
        return scan_raw_string_delimiter(scanner, lexer);
    }

    if (valid_symbols[RAW_STRING_CONTENT]) {
        lexer->result_symbol = RAW_STRING_CONTENT;
        return scan_raw_string_content(scanner, lexer);
    }

    return false;
}

unsigned tree_sitter_cpp_external_scanner_serialize(void *payload, char *buffer) {
    static_assert(MAX_DELIMITER_LENGTH * sizeof(wchar_t) < TREE_SITTER_SERIALIZATION_BUFFER_SIZE,
                  "Serialized delimiter is too long!");

    Scanner *scanner = (Scanner *)payload;
    size_t size = scanner->delimiter_length * sizeof(wchar_t);
    memcpy(buffer, scanner->delimiter, size);
    return (unsigned)size;
}

void tree_sitter_cpp_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
    Scanner *scanner = (Scanner *)payload;
    if (length % sizeof(wchar_t) != 0 || length > sizeof(scanner->delimiter)) {
        reset(scanner);
        return;
    }
    scanner->delimiter_length = length / sizeof(wchar_t);
    if (length > 0) {
        memcpy(&scanner->delimiter[0], buffer, length);
    }
}

void tree_sitter_cpp_external_scanner_destroy(void *payload) {
    Scanner *scanner = (Scanner *)payload;
    ts_free(scanner);
}
