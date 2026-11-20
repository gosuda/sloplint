#include "tree_sitter/parser.h"

#include <string.h>
#include <wctype.h>

enum TokenType {
    SEMI,
    CLASS_MEMBER_SEMI,
    BLOCK_COMMENT,
    NOT_IS,
    IN,
    Q_DOT,
    MULTILINE_STRING_CONTENT,
    CONSTRUCTOR,
    GET,
    SET,
    DOLLAR,
    ACCESSOR_SEMI,
    FIELD_SEMI,
    DELEGATION_SEMI,
};

#define MAX_WORD_SIZE 16
// Word tables list every modifier keyword plus a margin — keep the bound
// above the longest initializer or trailing words are silently dropped.
#define MAX_WORDS 40

static inline void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

static bool scan_word(TSLexer *lexer, const char *const word) {
    for (uint8_t i = 0; word[i] != '\0'; i++) {
        if (lexer->lookahead != word[i]) {
            return false;
        }
        skip(lexer);
    }
    return true;
}

static bool scan_words(TSLexer *lexer, const char words[MAX_WORDS][MAX_WORD_SIZE], char scanned_word[16],
                       uint8_t *index) {
    if (!scanned_word[0]) {
        for (uint8_t i = 0; i < MAX_WORD_SIZE - 1; i++) {
            if (!iswalnum(lexer->lookahead) && lexer->lookahead != '_') {
                if (i == 0) {
                    return false;
                }
                break;
            }
            scanned_word[i] = (char)lexer->lookahead;
            skip(lexer);
        }
    }

    for (uint8_t i = 0; i < MAX_WORDS; i++) {
        if (strncmp(scanned_word, words[i], MAX_WORD_SIZE) == 0) {
            if (index != NULL) {
                *index = i;
            }
            return true;
        }
    }

    return false;
}

static bool get_call_follows(TSLexer *lexer) {
    if (lexer->lookahead != '(') {
        return false;
    }
    skip(lexer);
    while (iswspace(lexer->lookahead)) {
        skip(lexer);
    }
    return lexer->lookahead != ')';
}

// Skips a Kotlin string or char literal whose opening quote `quote` is the
// current lookahead. Handles `\` escapes (single-quoted forms) and
// `"""..."""` raw strings. Callers use it while scanning expression lists so
// parens inside literals are not mistaken for syntax.
static void skip_string(TSLexer *lexer, wchar_t quote) {
    skip(lexer);
    bool raw = false;
    if (quote == '"' && lexer->lookahead == '"') {
        skip(lexer);
        if (lexer->lookahead == '"') {
            raw = true;
            skip(lexer);
        } else {
            // `""` — an already-closed empty string.
            return;
        }
    }
    while (!lexer->eof(lexer)) {
        wchar_t c = lexer->lookahead;
        skip(lexer);
        if (!raw && c == '\\') {
            skip(lexer);
            continue;
        }
        if (c != quote) {
            continue;
        }
        if (!raw) {
            return;
        }
        // A raw string ends on three consecutive quotes; the first was just
        // consumed. Consume the whole closing quote run so a literal ending
        // in extra quotes (`@A("""x"""")`) leaves none for the caller to
        // re-read as a new string opener.
        if (lexer->lookahead == '"') {
            skip(lexer);
            if (lexer->lookahead == '"') {
                while (lexer->lookahead == '"') {
                    skip(lexer);
                }
                return;
            }
        }
    }
}

static bool skip_ws_comments(TSLexer *lexer) {
    bool comment = false;
    for (;;) {
        while (iswspace(lexer->lookahead)) {
            skip(lexer);
        }
        if (lexer->lookahead != '/') {
            return comment;
        }
        skip(lexer);
        if (lexer->lookahead == '/') {
            while (lexer->lookahead != '\n' && !lexer->eof(lexer)) {
                skip(lexer);
            }
            comment = true;
            continue;
        }
        if (lexer->lookahead != '*') {
            return comment;
        }
        comment = true;
        // Consume the `*` of `/*` before nesting so `/*/x*/` cannot close early.
        skip(lexer);
        // Block comments may nest in Kotlin.
        unsigned depth = 1;
        while (depth > 0 && !lexer->eof(lexer)) {
            wchar_t c = lexer->lookahead;
            skip(lexer);
            if (c == '/' && lexer->lookahead == '*') {
                depth++;
                skip(lexer);
            } else if (c == '*' && lexer->lookahead == '/') {
                depth--;
                skip(lexer);
            }
        }
    }
}

// As skip_ws_comments, but also reports through `newline` whether a real
// line break was crossed: bare whitespace newlines and the newline that
// ends a `//` comment count, but newlines inside a `/* */` block comment
// do not (Kotlin does not insert a semi inside a block comment, so
// `var x = 1; /* a\nb */ get() = x` still attaches the getter).
static bool skip_ws_comments_nl(TSLexer *lexer, bool *newline) {
    bool comment = false;
    for (;;) {
        while (iswspace(lexer->lookahead)) {
            if (lexer->lookahead == '\n' || lexer->lookahead == '\r') {
                *newline = true;
            }
            skip(lexer);
        }
        if (lexer->lookahead != '/') {
            return comment;
        }
        skip(lexer);
        if (lexer->lookahead == '/') {
            while (lexer->lookahead != '\n' && !lexer->eof(lexer)) {
                skip(lexer);
            }
            comment = true;
            continue;
        }
        if (lexer->lookahead != '*') {
            return comment;
        }
        comment = true;
        skip(lexer);
        unsigned depth = 1;
        while (depth > 0 && !lexer->eof(lexer)) {
            wchar_t c = lexer->lookahead;
            skip(lexer);
            if (c == '/' && lexer->lookahead == '*') {
                depth++;
                skip(lexer);
            } else if (c == '*' && lexer->lookahead == '/') {
                depth--;
                skip(lexer);
            }
        }
    }
}

// Peek after a `;` (already consumed and mark_end'd) for an accessor word.
// `var x = 1; private set` — a `;` before `get`/`set`/`field` (possibly
// behind annotations and modifiers) belongs to the property tail, not the
// member/statement separator. The peeked characters are not marked, so the
// emitted token's extent stays at the `;`.
static void accessor_after_semi(TSLexer *lexer, const bool *valid_symbols) {
    char accessor_word[16] = {0};
    // Whether an annotation or modifier word preceded the accessor word:
    // `private set` is a bare accessor, while `get.run()` / `set + 1` are
    // statements using `get`/`set` as plain identifiers.
    bool had_prefix = false;
    // An accessor must start on the same line as the `;` — a newline
    // (including one inside a comment) ends the property, so
    // `val x = 1;\nfield = 5` is a new statement, not a backing field.
    bool crossed_newline = false;
    skip_ws_comments_nl(lexer, &crossed_newline);
    if (crossed_newline) {
        return;
    }
    for (;;) {
        skip_ws_comments(lexer);
        if (lexer->lookahead == '@') {
            skip(lexer);
            while (iswalnum(lexer->lookahead) || lexer->lookahead == '_' || lexer->lookahead == '.') {
                skip(lexer);
            }
            skip_ws_comments(lexer);
            if (lexer->lookahead == '(') {
                unsigned depth = 0;
                do {
                    wchar_t ch = lexer->lookahead;
                    if (ch == '"' || ch == '\'') {
                        skip_string(lexer, ch);
                        continue;
                    }
                    if (ch == '(') {
                        depth++;
                    } else if (ch == ')') {
                        depth--;
                    }
                    skip(lexer);
                } while (depth > 0 && !lexer->eof(lexer));
            }
            had_prefix = true;
            continue;
        }
        if (scan_words(lexer,
                       (const char[MAX_WORDS][MAX_WORD_SIZE]){"public", "private", "protected", "internal",
                                           "abstract", "final", "open", "override", "lateinit", "vararg",
                                           "noinline", "crossinline", "external", "suspend", "inline", "actual",
                                           "expect"},
                       accessor_word, NULL)) {
            memset(accessor_word, 0, MAX_WORD_SIZE);
            had_prefix = true;
            continue;
        }
        break;
    }
    // `scan_words` buffers a miss, so the word after the modifiers is already
    // in `accessor_word` for this final check.
    uint8_t accessor_index = -1;
    if (!scan_words(lexer, (const char[MAX_WORDS][MAX_WORD_SIZE]){"get", "set", "field"}, accessor_word,
                    &accessor_index)) {
        return;
    }
    // `set_up()`/`get_x`/`field2` match a prefix of an accessor word; a real
    // accessor is never followed by an identifier-continue character.
    if (iswalnum(lexer->lookahead) || lexer->lookahead == '_') {
        return;
    }
    // The accessor slot already used closes the property to a second one:
    // after `var x get() = 1` only `set` remains a valid accessor keyword,
    // so `; get` there is a plain identifier statement (`get\nfoo()` is a
    // call), not a duplicate getter the property grammar would reject.
    if ((accessor_index == 0 && !valid_symbols[GET]) ||
        (accessor_index == 1 && !valid_symbols[SET])) {
        return;
    }
    if (accessor_index == 0 || accessor_index == 1) {
        // A `get`/`set` accessor is either paren'ed (`get() = ...`,
        // `set(v) { }`) or a bare visibility-only accessor behind a
        // modifier (`private set`). Any other unmodified use — `get = 2`,
        // `get.run()`, `set + 1` — is a plain identifier statement, not an
        // accessor, so the `;` must stay a statement separator. A paren'ed
        // accessor must be followed by a body (`= expr`/`{...}`/`: type`)
        // or it is a call, not an accessor.
        bool member_newline = false;
        skip_ws_comments_nl(lexer, &member_newline);
        if (lexer->lookahead == '(') {
            skip(lexer);
            skip_ws_comments(lexer);
            // A getter takes `()`; a setter takes `(<identifier> ...)` where
            // annotations (`@Anno v`) or a backtick-quoted name may precede
            // or form the parameter name.
            if (accessor_index == 0) {
                if (lexer->lookahead != ')') {
                    return;
                }
            } else if (!iswalpha(lexer->lookahead) && lexer->lookahead != '_' &&
                       lexer->lookahead != '@' && lexer->lookahead != '`') {
                return;
            }
            // `(` was already consumed; run until its `)` closes, ignoring
            // strings, char literals, and comments inside the parens.
            unsigned depth = 1;
            do {
                wchar_t ch = lexer->lookahead;
                if (ch == '"' || ch == '\'') {
                    skip_string(lexer, ch);
                    continue;
                }
                if (ch == '/') {
                    skip(lexer);
                    if (lexer->lookahead == '/') {
                        while (lexer->lookahead != '\n' && !lexer->eof(lexer)) {
                            skip(lexer);
                        }
                        continue;
                    }
                    if (lexer->lookahead == '*') {
                        skip(lexer);
                        unsigned comment_depth = 1;
                        while (comment_depth > 0 && !lexer->eof(lexer)) {
                            wchar_t c = lexer->lookahead;
                            skip(lexer);
                            if (c == '/' && lexer->lookahead == '*') {
                                comment_depth++;
                                skip(lexer);
                            } else if (c == '*' && lexer->lookahead == '/') {
                                comment_depth--;
                                skip(lexer);
                            }
                        }
                        continue;
                    }
                    continue;
                }
                if (ch == '(') {
                    depth++;
                } else if (ch == ')') {
                    depth--;
                }
                skip(lexer);
            } while (depth > 0 && !lexer->eof(lexer));
            // The body may start on the next line: `get()\n= 2`. A `=` or
            // `:` that is really an operator (`get() == x`, `get()::class`)
            // keeps the `;` a statement separator.
            skip_ws_comments(lexer);
            if (lexer->lookahead == '=' || lexer->lookahead == ':') {
                wchar_t op = lexer->lookahead;
                skip(lexer);
                if (lexer->lookahead == op) {
                    return;
                }
            } else if (lexer->lookahead != '{') {
                return;
            }
        } else if (!had_prefix) {
            // Kotlin permits bare default accessors (`var x = 1; set`,
            // `var x = 1; get`), the same shape the newline-separated form
            // already accepts. Only expression continuations — `get = 2`,
            // `get.run()`, `set[i] = 1`, `get { }`, `set + 1`, `foo(get)`,
            // `get as T` — keep the `;` as a statement separator.
            skip_ws_comments_nl(lexer, &member_newline);
            switch (lexer->lookahead) {
            case '=':
            case '.':
            case '+':
            case '-':
            case '*':
            case '/':
            case '%':
            case '<':
            case '>':
            case '&':
            case '|':
            case '^':
            case '!':
            case '?':
            case ',':
            case ')':
            case ']':
            case ':':
            case '[':
            case '{':
            case '\'':
            case '"':
            case '`':
            case '$':
                return;
            default:
                break;
            }
            // A member keyword or modifier after a bare accessor starts the
            // next member (`get` + `private set`, `var y = 2`). Any other
            // word followed by an expression start is an infix call
            // (`get until 2`, `x foo y`, `get as Int`) — Kotlin infix
            // callees are arbitrary identifiers, so a fixed word list
            // cannot cover them.
            char member_word[16] = {0};
            if (scan_words(lexer,
                           (const char[MAX_WORDS][MAX_WORD_SIZE]){"val", "var", "fun", "class",
                                               "object", "interface", "enum", "annotation",
                                               "typealias", "constructor", "init", "companion",
                                               "public", "private", "protected", "internal",
                                               "abstract", "final", "open", "override",
                                               "lateinit", "const", "data", "sealed",
                                               "inner", "value", "inline", "tailrec",
                                               "operator", "infix", "external", "suspend",
                                               "expect", "actual", "get", "set",
                                               "context"},
                           member_word, NULL)) {
                // `scan_words` reads letters only, so the match can be a
                // prefix of a longer identifier (`context2`, `datax`); those
                // are infix callees, not member keywords.
                if (iswalnum(lexer->lookahead) || lexer->lookahead == '_') {
                    return;
                }
                skip_ws_comments(lexer);
                // A member keyword is followed by another keyword or a name
                // (`get data class`, `get private set`); a literal or
                // operator makes the word an infix callee instead
                // (`get context 2`, `get data(2)`, `get open -1`,
                // `get data < 2`). `context`/`init`/`companion` pin a narrow
                // follower set, and `(`/`:`/`<` are member syntax for only a
                // few words (`constructor(`, `val (a, b)`, `fun <T>`,
                // `object : Base`) — for the rest they read as calls or
                // operators (`get enum(2)`, `get data < 2`).
                const wchar_t next = lexer->lookahead;
                bool member_next = iswalpha(next) || next == '_' || next == '@' || next == '`';
                if (strcmp(member_word, "context") == 0) {
                    member_next = next == '(';
                } else if (strcmp(member_word, "init") == 0) {
                    member_next = next == '{';
                } else if (strcmp(member_word, "companion") == 0) {
                    member_next = iswalpha(next) || next == '_' || next == '@';
                } else if (next == '(') {
                    member_next = strcmp(member_word, "constructor") == 0 ||
                                  strcmp(member_word, "val") == 0 || strcmp(member_word, "var") == 0 ||
                                  strcmp(member_word, "get") == 0 || strcmp(member_word, "set") == 0 ||
                                  strcmp(member_word, "fun") == 0;
                } else if (next == ':') {
                    member_next = strcmp(member_word, "object") == 0;
                } else if (next == '<') {
                    member_next = strcmp(member_word, "fun") == 0 ||
                                  strcmp(member_word, "val") == 0 || strcmp(member_word, "var") == 0;
                } else if (next == '}' || next == ';') {
                    // A bare accessor ends its own member (`var x = 1; get }`,
                    // `get; set`); only the accessor words themselves close a
                    // member here — `get data }` still fails inside `data`.
                    member_next = strcmp(member_word, "get") == 0 || strcmp(member_word, "set") == 0;
                }
                if (!member_next) {
                    return;
                }
                // The same accessor word cannot start a second accessor of
                // the same kind — `var x = 1; get\nget` or `get\nget() = 2`
                // leaves the property with two getters. A `get`/`set` call
                // on the next line is a statement instead: `var x = 1;
                // get\nget(1)` keeps the getter and the `get(1)` call. On
                // the same line `get get(1)` is one infix expression, so
                // the `;` stays a separator either way.
                if ((accessor_index == 0 && strcmp(member_word, "get") == 0) ||
                    (accessor_index == 1 && strcmp(member_word, "set") == 0)) {
                    if (!(member_newline && next == '(' && get_call_follows(lexer))) {
                        return;
                    }
                }
            } else if (member_word[0] && member_newline &&
                       (strcmp(member_word, "as") == 0 || strcmp(member_word, "is") == 0 ||
                        strcmp(member_word, "in") == 0)) {
                // `as`/`is`/`in` cannot start a statement — Kotlin ignores
                // the newline before them, so `get\nas Int` is the infix
                // cast `get as Int` and `get` is an identifier.
                return;
            } else if (member_word[0] && !member_newline) {
                // Only on the accessor's own line can a non-keyword word be
                // an infix callee (`get until 2`); across a newline the
                // complete bare accessor ends the property and the word —
                // even a literal like `42` — starts the next member or
                // statement (`set\n42.toString()`).
                skip_ws_comments(lexer);
                if (iswalnum(lexer->lookahead) || lexer->lookahead == '_' ||
                    lexer->lookahead == '(' || lexer->lookahead == '[' ||
                    lexer->lookahead == '{' || lexer->lookahead == '"' ||
                    lexer->lookahead == '\'' || lexer->lookahead == '`' ||
                    lexer->lookahead == '!' || lexer->lookahead == '-' ||
                    lexer->lookahead == '+' || lexer->lookahead == '.' ||
                    lexer->lookahead == '?' || lexer->lookahead == ':' ||
                    lexer->lookahead == '@' || lexer->lookahead == '$') {
                    return;
                }
            }
        }
    } else if (accessor_index == 2) {
        // `field` binds a backing field only before `=` (or `: type =`),
        // possibly across comments and a line break; a call like `field(x)`,
        // a comparison (`field == x`), or a callable reference
        // (`field::class`) is a statement.
        skip_ws_comments(lexer);
        if (lexer->lookahead == '=' || lexer->lookahead == ':') {
            wchar_t op = lexer->lookahead;
            skip(lexer);
            if (lexer->lookahead == op) {
                return;
            }
        } else {
            return;
        }
        // A backing field can only open the property tail, so it gets a
        // distinct token: `_field_semi` is only valid at the tail start.
        // After a completed accessor `; field = n` is a new statement, so
        // leave the already-marked `_semi` in place when the token is not
        // valid here.
        if (!valid_symbols[FIELD_SEMI]) {
            return;
        }
        lexer->result_symbol = FIELD_SEMI;
        return;
    }
    lexer->result_symbol = ACCESSOR_SEMI;
}

// Skips whitespace and comments. Returns whether at least one comment was
// skipped, so callers can tell comment-crossed tokens from adjacent ones.

void *tree_sitter_kotlin_external_scanner_create() { return NULL; }

void tree_sitter_kotlin_external_scanner_destroy(void *payload) {}

unsigned tree_sitter_kotlin_external_scanner_serialize(void *payload, char *buffer) { return 0; }

void tree_sitter_kotlin_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {}

bool tree_sitter_kotlin_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
    if (valid_symbols[MULTILINE_STRING_CONTENT]) {
        bool did_advance = false;
        lexer->result_symbol = MULTILINE_STRING_CONTENT;
        while (!lexer->eof(lexer)) {
            switch (lexer->lookahead) {
                case '$':
                    lexer->mark_end(lexer);
                    advance(lexer);
                    if (iswalpha(lexer->lookahead) || lexer->lookahead == '{') {
                        return did_advance;
                    }
                    did_advance = true;
                    break;
                case '"':
                    lexer->mark_end(lexer);
                    // 3 or 4 quotes means we're done
                    advance(lexer);
                    if (lexer->lookahead == '"') {
                        advance(lexer);
                        if (lexer->lookahead == '"') {
                            advance(lexer);
                            if (lexer->lookahead == '"') {
                                advance(lexer);
                            }
                            return did_advance;
                        }
                    }
                    did_advance = true;
                    break;
                default:
                    advance(lexer);
                    did_advance = true;
                    break;
            }
        }
    }

    if (valid_symbols[SEMI] || valid_symbols[CLASS_MEMBER_SEMI]) {
        lexer->result_symbol = valid_symbols[SEMI] ? SEMI : CLASS_MEMBER_SEMI;
        lexer->mark_end(lexer);
        bool saw_newline = false;
        for (;;) {
            if (lexer->eof(lexer)) {
                return true;
            }

            if (lexer->lookahead == ';') {
                advance(lexer);
                lexer->mark_end(lexer);
                if (valid_symbols[ACCESSOR_SEMI] || valid_symbols[FIELD_SEMI]) {
                    accessor_after_semi(lexer, valid_symbols);
                }
                return true;
            }

            if (!iswspace(lexer->lookahead)) {
                break;
            }

            if (lexer->lookahead == '\n') {
                skip(lexer);
                saw_newline = true;
                break;
            }

            if (lexer->lookahead == '\r') {
                skip(lexer);

                if (lexer->lookahead == '\n') {
                    skip(lexer);
                }

                saw_newline = true;
                break;
            }

            skip(lexer);
        }

        // Skip whitespace and comments
        while (iswspace(lexer->lookahead)) {
            skip(lexer);
        }
        if (lexer->lookahead == '/') {
            goto comment;
        }

        if (!saw_newline) {
            switch (lexer->lookahead) {
                case '!':
                    skip(lexer);
                    goto continue_not_is_from_semi;
                case '?':
                    if (valid_symbols[Q_DOT]) {
                        goto q_dot_from_semi;
                    }
                    return false;
                case 'i':
                    return scan_word(lexer, "import");
                case '}':
                    // `SEMI` can be valid at a member boundary now (the member
                    // separator accepts it), so gate on the member symbol alone.
                    return valid_symbols[CLASS_MEMBER_SEMI];
                case ';':
                    advance(lexer);
                    lexer->mark_end(lexer);
                    return true;
                default:
                    return false;
            }
        }

        char scanned_word[16] = {0};
    _switch:
        switch (lexer->lookahead) {
            case ':':
                advance(lexer);
                return lexer->lookahead == ':';
            case ',':
            case '.':
            case '*':
            case '%':
            case '>':
            case '<':
            case '=':
            case '{':
            case '[':
            case '|':
            case '&':
            case '/':
                return false;
            // Insert a semicolon before `--` and `++`, but not before binary `+` or `-`.
            // Insert before +/-{float}
            case '+':
                skip(lexer);
                if (lexer->lookahead == '+') {
                    return true;
                }
                return iswdigit(lexer->lookahead);
            case '-':
                skip(lexer);
                if (lexer->lookahead == '-') {
                    return true;
                }
                return iswdigit(lexer->lookahead);
            // Don't insert a semicolon before `!=`, but do insert one before a unary `!`.
            case '!':
                skip(lexer);
                if (lexer->lookahead == 'i' && valid_symbols[NOT_IS]) {
                    skip(lexer);
                    if (lexer->lookahead == 's') {
                        skip(lexer);
                        if (!iswalnum(lexer->lookahead)) {
                            return true;
                        }
                    }
                }
                return lexer->lookahead != '=';
            case '?':
                if (valid_symbols[Q_DOT]) {
                    goto q_dot_from_semi;
                }
                return true;
            // `class C\n(params)` — a `(` continuing a declaration head must
            // not take a semicolon. The CONSTRUCTOR keyword is valid exactly in
            // that head position (`class C constructor(...)`); after a complete
            // statement (`fun g() {}`, `do { } while (c)`, `val x: Int`) it is
            // not, so the semi must be emitted or `(a as B).c()` merges into
            // the previous construct. In expression position `x\n(y)` stays two
            // statements; nullable-chain (`?.`) and `!is` continuations also
            // mark an ended expression, so the semi is kept wherever either is
            // valid.
            case '(':
                // `Type\n(` mid-delegation: the separator is part of the
                // constructor invocation, not the declaration end. Emitting
                // the dedicated token keeps `_semi` free to terminate a
                // bodyless declaration (`class T : M<Int>` at EOF).
                if (valid_symbols[DELEGATION_SEMI]) {
                    lexer->result_symbol = DELEGATION_SEMI;
                    return true;
                }
                return valid_symbols[Q_DOT] || valid_symbols[NOT_IS] ||
                       !valid_symbols[CONSTRUCTOR];
            case 'e':
            case 'i':
            case 'g':
            case 's':
            case 'p':
            case 'a':
            case 'f':
            case 'o':
            case 'l':
            case 'v':
            case 'n':
            case 'c':
            case 'b':
            case 'w':
                bool crossed_comment = false;
                bool crossed_modifier = false;
                while (scan_words(lexer,
                                  (const char[MAX_WORDS][MAX_WORD_SIZE]){"public", "private", "protected", "internal", "abstract",
                                                       "final", "open", "override", "lateinit", "vararg", "noinline",
                                                       "crossinline", "external", "suspend", "inline", "actual",
                                                       "expect"},
                                  scanned_word, NULL)) {
                    memset(scanned_word, 0, MAX_WORD_SIZE);
                    crossed_modifier = true;
                    crossed_comment |= skip_ws_comments(lexer);
                }

                uint8_t index = -1;
                bool res = scan_words(
                    lexer,
                    (const char[MAX_WORDS][MAX_WORD_SIZE]){"else", "in", "instanceof", "get", "set", "constructor", "by", "as",
                                        "where", "field"},
                    scanned_word, &index);

                // A continuation keyword (else/in/instanceof/by/as/where) after a comment crossed in the
                // modifier loop means the preceding word was an identifier, not a member modifier, so
                // this line is a new statement rather than a continuation of the previous expression.
                // A consumed modifier word means the same: `actual as T` reads `actual`
                // as an identifier (a modifier is never followed by a continuation
                // keyword in a declaration head), so the statement separates.
                if ((crossed_comment || crossed_modifier) && (index <= 2 || (index >= 6 && index <= 8))) {
                    return true;
                }

                // If `CLASS_MEMBER_SEMI` is valid, we found a secondary constructor and so we want to insert a semi, OR
                // we found a variable named constructor whose field is being accessed
                if (index == 5) {
                    while (iswspace(lexer->lookahead)) {
                        skip(lexer);
                    }
                    if (valid_symbols[CLASS_MEMBER_SEMI] || lexer->lookahead == '.' || lexer->lookahead == '=') {
                        return true;
                    }
                }
                // Ordinarily, we should not insert a semicolon if there is an `else` on the next line,
                // except for when it's a 'when entry', which has a `->` after the `else`.
                else if (index == 0) {
                    while (iswspace(lexer->lookahead)) {
                        skip(lexer);
                    }
                    if (lexer->lookahead == '-') {
                        skip(lexer);
                        if (lexer->lookahead == '>') {
                            return true;
                        }
                    }
                }
                // If `get` was found and the keyword is not valid, return a semi since it's being used as an identifier
                else if (index == 3 && (!valid_symbols[GET] || lexer->lookahead == '[' || get_call_follows(lexer))) {
                    return true;
                }
                // If `set` was found and the keyword is not valid, return a semi since it's being used as an identifier
                else if (index == 4 && (!valid_symbols[SET] || lexer->lookahead == '[' || lexer->lookahead == '(' ||
                                        lexer->lookahead == '.')) {
                    if (lexer->lookahead == '(' && valid_symbols[SET]) {
                        unsigned depth = 0;
                        do {
                            if (lexer->lookahead == '(') {
                                depth++;
                            } else if (lexer->lookahead == ')') {
                                depth--;
                            }
                            skip(lexer);
                        } while (depth > 0 && !lexer->eof(lexer));

                        while (iswspace(lexer->lookahead)) {
                            if (lexer->lookahead == '\n') {
                                return true;
                            }
                            skip(lexer);
                        }
                        return false;
                    }
                    return true;
                }
                // If `in` was found and this specific external keyword is valid,
                // return a semi since it's being used in a range test
                else if (index == 1 && valid_symbols[IN]) {
                    return true;
                }
                // If `field` was found at class member position and a `:` or `=`
                // follows (comments permitted between), this is a Kotlin 2.x explicit
                // backing field continuing the property declaration — no semi.
                else if (index == 9) {
                    // A `field: T = expr` tail can only be a Kotlin 2.x explicit
                    // backing field — a `field:` statement is never valid — so the
                    // semi is suppressed wherever a statement separator is allowed.
                    // A `field = expr` tail is ambiguous with a plain assignment to
                    // an identifier named `field`, so it suppresses only at class
                    // member position, where a bare identifier cannot start a member.
                    if (!valid_symbols[SEMI] && !valid_symbols[CLASS_MEMBER_SEMI]) {
                        return true;
                    }
                    skip_ws_comments(lexer);
                    if (lexer->lookahead == ':') {
                        skip(lexer);
                        return lexer->lookahead == ':';
                    }
                    if (lexer->lookahead == '=') {
                        if (!valid_symbols[CLASS_MEMBER_SEMI]) {
                            return true;
                        }
                        skip(lexer);
                        return lexer->lookahead == '=';
                    }
                    return true;
                }
                return !res;
            case ';':
                advance(lexer);
                lexer->mark_end(lexer);
                return true;
            case '@':
                if (valid_symbols[CONSTRUCTOR]) {
                    // `@A(x) @B private actual constructor` — a secondary
                    // constructor head may be split across lines with any
                    // number of annotations and modifiers between them. Skip
                    // each annotation (name plus a balanced-paren argument
                    // list, so `@Anno(name = "x")` does not stop at the
                    // inner space), then the modifier words, then require
                    // the `constructor` keyword itself.
                    for (;;) {
                        while (!iswspace(lexer->lookahead) && lexer->lookahead != '(' &&
                               !lexer->eof(lexer)) {
                            skip(lexer);
                        }
                        if (lexer->lookahead == '(') {
                            unsigned depth = 0;
                            do {
                                wchar_t ch = lexer->lookahead;
                                if (ch == '"' || ch == '\'') {
                                    // String/char literals inside the argument list
                                    // must not feed the paren counter (`@Anno(")")`).
                                    skip_string(lexer, ch);
                                    continue;
                                }
                                if (ch == '/') {
                                    // `//` and `/*` comments can carry
                                    // unbalanced parens (`@A(1 /*)*/ 2)`).
                                    skip(lexer);
                                    if (lexer->lookahead == '/') {
                                        while (lexer->lookahead != '\n' && !lexer->eof(lexer)) {
                                            skip(lexer);
                                        }
                                        continue;
                                    }
                                    if (lexer->lookahead == '*') {
                                        skip(lexer);
                                        unsigned comment_depth = 1;
                                        while (comment_depth > 0 && !lexer->eof(lexer)) {
                                            wchar_t c = lexer->lookahead;
                                            skip(lexer);
                                            if (c == '/' && lexer->lookahead == '*') {
                                                comment_depth++;
                                                skip(lexer);
                                            } else if (c == '*' && lexer->lookahead == '/') {
                                                comment_depth--;
                                                skip(lexer);
                                            }
                                        }
                                        continue;
                                    }
                                    continue;
                                }
                                if (ch == '(') {
                                    depth++;
                                } else if (ch == ')') {
                                    depth--;
                                }
                                skip(lexer);
                            } while (depth > 0 && !lexer->eof(lexer));
                        }
                        skip_ws_comments(lexer);
                        if (lexer->lookahead != '@') {
                            break;
                        }
                    }
                    while (scan_words(lexer,
                                      (const char[MAX_WORDS][MAX_WORD_SIZE]){"public", "private", "protected", "internal",
                                                           "abstract", "final", "open", "override", "lateinit",
                                                           "vararg", "noinline", "crossinline", "external",
                                                           "suspend", "inline", "actual", "expect"},
                                      scanned_word, NULL)) {
                        memset(scanned_word, 0, MAX_WORD_SIZE);
                        skip_ws_comments(lexer);
                    }
                    // `scan_words` buffers the word it scanned even on a
                    // miss, so `constructor` may already sit in
                    // `scanned_word` here — match it through the same
                    // helper instead of re-reading the lexer.
                    if (scan_words(lexer,
                                   (const char[MAX_WORDS][MAX_WORD_SIZE]){"constructor"},
                                   scanned_word, NULL)) {
                        return false;
                    }
                    return true;
                }
                if (valid_symbols[GET] || valid_symbols[SET]) {
                    bool saw_paren = false;
                    while ((saw_paren ? lexer->lookahead != '\n' : !iswspace(lexer->lookahead))) {
                        skip(lexer);
                        if (lexer->lookahead == '(') {
                            saw_paren = true;
                        }
                        if (lexer->lookahead == ')') {
                            saw_paren = false;
                        }
                    }
                    while (iswspace(lexer->lookahead)) {
                        skip(lexer);
                    }
                    if (lexer->lookahead == '/') {
                        return true;
                    }
                    goto _switch;
                }
                return true;

            default:
                return true;
        }
    }

    while (iswspace(lexer->lookahead)) {
        skip(lexer);
    }

    if (valid_symbols[NOT_IS]) {
        if (lexer->lookahead == '!') {
            advance(lexer);
        continue_not_is_from_semi:
            if (lexer->lookahead == 'i') {
                advance(lexer);
                if (lexer->lookahead == 's') {
                    advance(lexer);
                    lexer->result_symbol = NOT_IS;
                    lexer->mark_end(lexer);
                    return !iswalnum(lexer->lookahead);
                }
            }
        }
    }

    if (valid_symbols[IN]) {
        if (lexer->lookahead == 'i') {
            advance(lexer);
            if (lexer->lookahead == 'n') {
                advance(lexer);
                lexer->result_symbol = IN;
                lexer->mark_end(lexer);
                return !iswalnum(lexer->lookahead);
            }
        }
    }

q_dot_from_semi:
    if (valid_symbols[Q_DOT]) {
        while (iswspace(lexer->lookahead)) {
            skip(lexer);
        }
        if (lexer->lookahead == '?') {
            advance(lexer);
            while (iswspace(lexer->lookahead)) {
                skip(lexer);
            }
            if (lexer->lookahead == '.') {
                advance(lexer);
                lexer->result_symbol = Q_DOT;
                lexer->mark_end(lexer);
                return true;
            }
        }
    }

comment:
    if (valid_symbols[DOLLAR]) {
        return false;
    }

    if (lexer->lookahead == '/') {
        advance(lexer);
        if (lexer->lookahead != '*') {
            return false;
        }
        advance(lexer);

        bool after_star = false;
        unsigned nesting_depth = 1;
        for (;;) {
            switch (lexer->lookahead) {
                case '\0':
                    return false;
                case '*':
                    advance(lexer);
                    after_star = true;
                    break;
                case '/':
                    if (after_star) {
                        advance(lexer);
                        after_star = false;
                        nesting_depth--;
                        if (nesting_depth == 0) {
                            lexer->result_symbol = BLOCK_COMMENT;
                            lexer->mark_end(lexer);
                            return true;
                        }
                    } else {
                        advance(lexer);
                        after_star = false;
                        if (lexer->lookahead == '*') {
                            nesting_depth++;
                            advance(lexer);
                        }
                    }
                    break;
                default:
                    advance(lexer);
                    after_star = false;
                    break;
            }
        }
    }

    return false;
}
