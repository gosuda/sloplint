# tree-sitter-bash, vendored for sloplint

Date: 2026-10-20.

## Origin

- Upstream repository: https://github.com/tree-sitter/tree-sitter-bash
- Upstream tag: v0.25.1
- Published crate: tree-sitter-bash 0.25.1 (crates.io). Name and version are unchanged.
- This directory is the published crate plus the patch below. `README.md`, `LICENSE`,
  `queries/`, `bindings/rust/` and `Cargo.toml` are the published files.
- `sloplint-grammar.patch` is the diff of `grammar.js` and `src/scanner.c` against the
  upstream tag. It applies with `git apply` in a clone of tag v0.25.1.

## Why

The published grammar reports a parse error on valid Bash. sloplint skips a whole file on
any error node. These patches remove the errors that were reproduced on real scripts.

## Regenerating src/

- Tool: tree-sitter-cli 0.27.1, JavaScript runtime node v26.12.0 (the default).
- Command, run in this directory: `tree-sitter generate`
- It rewrites `src/parser.c`, `src/grammar.json`, `src/node-types.json` and
  `src/tree_sitter/*.h`. `src/scanner.c` is edited by hand and is not generated.
- Generation reports no conflicts beyond the existing supertype notes. The named kinds and
  fields that the sloplint Bash adapter reads are the same in `src/node-types.json` as in
  the published crate. Two hidden supertype entries differ: `_statement` and
  `_primary_expression` are not listed, because the 0.27.1 CLI expands supertypes. One
  anonymous token, `<>`, is added by the redirect change. Neither difference is a named
  kind or a field.
- Run `tree-sitter test` in an upstream clone with the patch applied and the
  `test/corpus/sloplint_*.txt` files copied in. Upstream does not ship them.

## Changes to grammar.js

- Read-write redirect: `file_redirect` accepts the `<>` operator (`exec 0<> file`,
  `echo a<>b`).
- Here-string after a file redirect: `redirected_statement` takes one `file_redirect` or
  `heredoc_redirect` first. More redirects of any kind, here-strings included, may follow.
- Lone assignment with a redirect: a new `redirected_statement` branch has the
  `variable_assignment` as body, with higher precedence than the generic branch. One
  conflict is declared for this: `[redirected_statement, list, command]`.
- Base-prefixed number with a variable: `number` accepts `simple_expansion` after `N#`.
- Case pattern with several quoted chunks: `_extglob_blob` repeats the
  string/expansion/command-substitution chunk with an optional extglob part after each.
  A single-quoted chunk (`raw_string`) is accepted in the same position.
- `for x do` without an in-list: `for_statement` accepts `do` directly after the variable.
  With an in-list, the terminator before `do` is still required.
- Heredoc operator at the end of the opener line: `cat <<EOF &&` and `cat <<EOF ||` take
  their right side after the heredoc body, as the next statement. The right side is not a
  child of the heredoc. A right side on the same line still parses as before. The
  operator form is an alternative to the same-line right side, not an addition to it.

## Changes to src/scanner.c

- Plain regex scan: outside `${...}`, a regex scan that meets an unnested newline declines.
  The check runs only when the plain `REGEX` symbol is valid and `CLOSING_BRACE` is not.
  The `[[ =~ ]]` right-hand side uses `REGEX_NO_SPACE` and does not reach it.
- Closing paren in a parameter alternate: inside `${...}`, a `)` after an expansion
  starts a concatenated word. `CLOSING_BRACE` is valid only while the expansion can end.
- Extglob pattern: an opening single quote ends the pattern token, as a double quote
  already did. An escaped single quote is consumed as an ordinary pattern character.

No backtick change. An earlier version of this patch changed how a backtick starts a
concatenated substitution. It was removed because it changed the tree of valid input.

## Constructs verified fixed

Each was a parse error with the published crate and is valid for `bash -n`.

- `f() {` newline `tee -a $c >/dev/null <<<"x $y"` newline `}`
- `exec 0<> "${o}/fake-stdin"`, `exec 3<>file`, `exec 8<>V.input`, `echo a<>b`
- `case "$(echo x)" in *"Copyright (c) "*"Uber"*) ;; esac` (zap `checklicense.sh`)
- `Y="a" && Y+=" b" 2>/dev/null`
- `h=ff; x=$((16#$h))` and `echo 16#$h`
- `echo == Loading $f` followed by a line that holds `)` inside a string (rocksdb
  `generate_random_db.sh`).
- `for arg do` with no in-list, including inside an `if` branch (git `emacsclient.sh`
  under oh-my-zsh, and `ZeroMQ` `android_build_helper.sh`)
- `"$1${2:+ (from $2)}"` (git `t/lib-pack.sh`) and `"$PUSH_REMOTE${push_repo:+ ($push_repo)}"`
  (ktor `switch-base-branch.sh`)
- `case x in ?*' '?*)` (git `git-merge-octopus.sh`, `git-merge-resolve.sh`)
- `cat >f <<\EOF &&` and `cat >f <<\EOF ||` with the right side after the body (git
  `contrib/vscode/init.sh` and `t/t9500-gitweb-standalone-no-errors.sh`)

Sweep totals, after the latest change, on the Main corpora (1630 Bash files):
7 error-to-clean files. On the combined set with the frozen snapshot (3560 files): 16.

## Constructs known still failing

1. Two backtick substitutions in one file, the second opening across a newline (rocksdb
   `coverage_test.sh`, 2 files). Example: `G=\`a\`` newline `L=\`` newline `  b\``. The
   closing backtick of the first substitution and the opening backtick of the second have
   the same neighbouring characters, and the scanner sees the same valid-symbol set at
   both. Backticks inside the grammar are internal literals, so the scanner never sees the
   pairing. A fix needs the parity of the backticks, and the scanner does not track it.
   Tried and not kept: a guard on the character after the backtick (changed valid trees);
   gating on `OPENING_PAREN` (no effect); an optional `_concat` before the closing backtick
   (still failed); a backtick external token with a depth counter (18 of 27 calibration
   inputs changed).
2. `$((time -p "$@" >/dev/null) 2>&1)` (four golang.org/x/exp `shootout/timing.sh` files).
   Bash reads `$((` as arithmetic first and falls back to a command substitution. One
   attempt added a `$((` alternative. It regressed `echo $(( (a+b) * 2 ))` and was
   reverted.
3. A heredoc body whose first line starts with a backslash escape and contains `<`, `>`
   or `|` (dune `test/blackbox-tests/setup-script.sh`). Minimal repro:
   `cat > source <<'XX'`, `\_o< COIN`, `XX`. Mechanism: newlines are extras, so after the
   opener the body line is read as argument words on the same statement, and the `<` then
   fails. `\_o COIN` parses only because it is read as arguments. Not fixed: the fix changes
   how heredoc arguments may cross a newline.
4. A nested subshell opened with `((` (dune `bench/run-synthetic-dune-watch.sh`). Minimal
   repro: `((${p} build) || (echo x)) &`. `( (x) || (y) )` with a space parses. Cause: the
   lexer commits `((` to arithmetic before the group is known to be arithmetic. Not fixed.
5. `f() { :; } >f <<<x` (a function body followed by two redirects). Valid Bash. Still an
   error. A function with one redirect parses.
6. `exec 0> f` and `exec 0<> f`: the leading `0` is lexed as a command argument (`number`),
   not as a `file_descriptor`. The published grammar does the same for `0>`. `exec 3<> f`
   gives a `file_descriptor`.
7. A lone assignment with a redirect followed by a command on the next line
   (`Y=a >f` newline `other`) parses as one `command` whose name is on the next line. The
   published grammar gives the same tree. No parse error.

Inputs accepted that `bash -n` rejects. These are permissive, not clean-to-error changes:

- `cat >f <<\EOF &&` with nothing after the terminator. The right side is a following
  statement, and the grammar cannot require one.
- `?*' ' *)` (a quoted space followed by a space before the closing paren). The published
  grammar already accepts the double-quoted form, `?*" " *)`, and the single-quoted form
  now follows the same rule.

Inputs that are not Bash and are skipped, not analysed:

- Bats test files: `test/bats.bats` and `test/fixtures/bats/whitespace_no_shellcheck.bats`.
  `bash -n` rejects them (`@test "name" {` followed by `}` is bats syntax).
- Build templates: `git-filter-branch.sh` (`@SHELL_PATH@`) and `git-sh-setup.sh`
  (`@DIFF@`, line 283). These are substituted at build time.
- Zsh-dialect files in oh-my-zsh (`${(M)...}`, `${0:a:h}`, `#!/usr/bin/env zsh`).
- Invalid input for both grammars: `fbcode_config.sh` (`bash -n`: unexpected end of file).

## Corpus results

- Upstream `tree-sitter test` at tag v0.25.1: 100 of 100 before, 100 of 100 after.
- Vendored cases in `test/corpus/sloplint_*.txt`: 38. Redirects 9, case patterns 3,
  backtick concatenation 4, arithmetic 2, regex arguments 3, substitution 1, for loops 4,
  parameter alternates 4, case glob quotes 4, heredoc operators 4.
- The published grammar fails 18 of the 38 vendored cases. The patched grammar passes all 38.
- Total with the patch: 138 of 138.
- The expected trees were produced by `tree-sitter test --update` and reviewed by hand.
  No case contains an ERROR or MISSING node.

## Sweep result

Tree comparison between the published parser and this one. Each file is parsed by a C
harness linked against the published sources and against this directory's sources.

- Main corpora (nvm, bats-core, git-scripts, ohmyzsh, dune): 1630 Bash files. Error to
  clean: 7. Clean to error: 0. Clean files whose tree changed: 38. Error files whose tree
  changed and that stay in error: 2 (zsh files).
- Combined set with a frozen snapshot of registry, module-cache and compiler shell files
  (3560 files in total): error to clean 16, clean to error 0, clean trees changed 38.
  Error to error with a changed tree: 2.
- The 38 clean trees that changed are all heredoc deferred-operator openers in git test
  scripts. In each, the published grammar read the first body line as the operator's right
  side, so that line was counted as a command or assignment. The vendored grammar keeps it
  as heredoc text. Across the 38 files the number of `right:` operands under heredocs fell
  from 115 to 50. The remaining 50 are same-line right sides. The number of `heredoc_body`
  nodes is unchanged at 164. sloplint facts for these files change: calls and bindings that
  came from heredoc text are no longer counted.

sloplint `meta/parse-error` counts, release build, `check --no-cache`:

| Corpus | Published | Patched | Files removed | Files added |
|---|---|---|---|---|
| nvm | 0 | 0 | 0 | 0 |
| bats-core | 2 | 2 | 0 | 0 |
| git-scripts | 8 | 2 | 6 | 0 |
| ohmyzsh | 9 | 8 | 1 | 0 |
| dune | 55 | 55 | 0 | 0 |

The dune count includes C and header files, which this change does not touch.

## Size

`src/parser.c`: 10,054,410 bytes (published: 9,912,254; previous patch: 9,997,512).
States: 7681 (published: 7571; previous patch: 7597).
