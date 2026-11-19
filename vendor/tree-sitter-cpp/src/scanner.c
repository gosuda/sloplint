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

/// `{` that opens a macro block (`ON_SCOPE_EXIT { f(); };`): it is
/// only a macro block when a `;` appears before the matching `}`.
/// Braced initializer lists (`Type{arg}`, `Type{arg}.m()`) never
/// contain `;`, so they always fall back to the regular `{` token.
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
    size_t scanned = 0;
    for (;;) {
        if (lexer->eof(lexer) || scanned++ > MACRO_BRACE_SCAN_LIMIT) {
            return false;
        }
        int32_t c = lexer->lookahead;
        if (c == '"' || c == '\'') {
            // Skip string/char literals so their contents can't count
            // toward the `;` or `}` search.
            int32_t quote = c;
            advance(lexer);
            while (!lexer->eof(lexer)) {
                if (lexer->lookahead == '\\') {
                    advance(lexer);
                    if (!lexer->eof(lexer)) advance(lexer);
                    continue;
                }
                if (lexer->lookahead == quote) {
                    advance(lexer);
                    break;
                }
                if (lexer->lookahead == '\n' && quote == '\'') break;
                advance(lexer);
            }
            continue;
        }
        if (c == '/') {
            // Skip // and /* */ comments.
            advance(lexer);
            if (lexer->lookahead == '/') {
                while (!lexer->eof(lexer) && lexer->lookahead != '\n') advance(lexer);
                continue;
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
                continue;
            }
            continue;
        }
        if (c == '{') depth++;
        if (c == '}') {
            depth--;
            if (depth == 0) return false;
        }
        if (c == ';') return true;
        advance(lexer);
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
