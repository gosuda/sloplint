## Behavior

This update recognizes literal Cmd Echo forms such as `echo(` without treating their opening parenthesis as an unclosed command group. Nested `cmd /c` and `cmd /k` screening now reads the original payload bytes, preserving equals signs, commas, semicolons, tabs, redirection and trailing spaces.

The repair keeps real command groups and FOR input sets distinct. Bodyless or malformed loops, unmatched command groups, computed programs and refused executables retain their review or blocking paths. The Windows permission regression executes harmless Echo forms and verifies that a computed command after Echo is still denied without exact approval.

The existing #30/#12/#3 follow-up, imported #20 work, and external feature/test changes through `38d586ae` remain. The earlier behavior changes also remain part of this PR:

- **Cmd FOR:** recognize headers in command position, distinguish input data from the body delimiter, and require a body. FOR /F file/literal inputs remain separate from command sources; observed attached `usebackq` spellings that cannot be parsed reliably retain Review.
- **POSIX parameters and wrappers:** literal `{@` text inside a scalar fallback stays one field, while actual multi-field expansions retain their controls. Grouped `command` lookup switches are recognized. A lone `env -` before assignments is an option; after assignments it is the executable.
- **fd permissions:** attached literal data filters such as `-tf` and `--type=f` can remain read-only after screening returns Clear. Later execution flags, assignment prefixes and redirection retain their separate effect/approval checks.

The POSIX parser, `tools_shell.ml`, public interface and workflow files are unchanged by this two-file follow-up. The full earlier description is retained in the [f879 receipt](https://github.com/gosuda/sloplint/blob/d718be40e2d96f061f52998b5ef11e9eb4e7b101/ci-records/charamel32-f879-body.md).

## Current verification

Verified additive source: __HEAD__, tree __TREE__, on actual main `84e5ff153f781c7e2d02804a5c24997858f618ab`.

- [Native Echo observations](https://github.com/gosuda/sloplint/actions/runs/45967234894) and [trailing-space controls](https://github.com/gosuda/sloplint/actions/runs/45969232238) freeze 46 actual Windows results. They include valid Echo forms, native-invalid group syntax, and output distinctions that would be lost by rejoining words.
- [Source preparation](https://github.com/gosuda/sloplint/actions/runs/__PREPARE_RUN__) checks 177 classifier cases and 24 actual recursive-input traces, preserving all 124 earlier cases. The original source, payload-only stage, full candidate, reverted Echo stage and restored candidate are recorded separately with actual exits. Trace instrumentation exists only in temporary compilation copies.
- [Fresh Linux and Windows gates](https://github.com/gosuda/sloplint/actions/runs/__GATES_RUN__) authenticate that preparation and apply a SHA-256-pinned patch, then require the exact candidate Git tree and clean source identity. Linux runs `dune build @fmt`, `dune build --profile release` and `dune runtest --profile release` in the required `rd` switch. Windows runs formatting, the full release build, the shell/permission suites and all 101 frozen native Cmd observations. __ACTUAL_COUNTS__

The proof uses OCaml 5.5.1 and ocamlformat 0.29.0. No full Windows-suite or complete shell-parity claim is made. Two already conservative nested FOR forms continue to require review.

Exact source hashes, artifact digests, source restoration and all historical failed attempts are preserved in __CHECKPOINT__. The earlier f879/e004 result remains historical evidence for that revision; the current production change has its own Linux and Windows proof.

## Review and ordinary CI

__THREAD_STATE__

__ORDINARY_CI__
