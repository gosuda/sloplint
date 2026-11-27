## Current follow-up

Cmd screening now removes redirections owned by the outer command before stripping the command string's outer quotes. Inner quoted filenames remain literal arguments, so `cmd /d /s /c "type "draft (for review).txt""` no longer turns the filename into a false loop-review request. Source spelling, separators and redirections inside the quoted payload remain intact.

This revision retains every external verdict and Windows Echo assertion from `d19fcb385bf404c2fbc1628d8c090c63a63cdcd1`. It adds a documented `Shell_screen.For_test.screen_cmd` observation surface, exact recursive-input regressions, and owned-file Windows permission tests. The trace records the inputs actually consumed by recursive screening; it is not a claim that the trace equals the native process command line.

### Exact source and verification

- Published revision: **FINAL_HEAD**, tree **FINAL_TREE**, additive child of `d19fcb385bf404c2fbc1628d8c090c63a63cdcd1`.
- Three changed paths in this follow-up: `bin/crush/lib/shell_screen.ml`, its documented `.mli`, and `test/crush/test_tools_shell.ml`. The other 1,091 tracked entries and their modes are unchanged.
- [Native quoted-filename observations](https://github.com/gosuda/sloplint/actions/runs/45997335988): all 20 frozen observations retained, with 16 native exits 0 and four native-invalid controls exiting 1. The reported file read and owned-child argument check pass.
- [Exact parser comparison](https://github.com/gosuda/sloplint/actions/runs/45999925649): current `d19f` reproduces the false Review and changed filename bytes; prior `c009` and the span-aware implementation preserve the filename. Observation adapters preserve the original classifiers.
- [Maintained regression preparation](https://github.com/gosuda/sloplint/actions/runs/PREP_RUN): all 214 classifier cases and 57 exact-input cases pass on the candidate and restored candidate. Reinstating only the first-subsequent-quote implementation preserves the older 203 classifier / 48 input controls but fails the new quoted-filename regression.
- [Fresh Depot Linux and Windows source gates](https://github.com/gosuda/sloplint/actions/runs/FULL_RUN): FINAL_GATE_SUMMARY. Both lanes use the real `rd` switch, OCaml 5.5.1 and ocamlformat 0.29.0. The final tracked source and index match the published tree.
- [Durable terminal receipt](TERMINAL_URL) includes source hashes, actual command exits, artifact digests, the two setup-only observation failures, and the earlier successful but unpublished `ecea` proof superseded by external head movement.

### Preserved scope

Earlier repairs to POSIX parameter/wrapper handling, `fd` read-only recognition, and Cmd FOR/Echo approval boundaries remain in the branch. Their previous exact-tree evidence is preserved in [the earlier receipt](https://github.com/gosuda/sloplint/blob/97893a8905fecfc6a3dbbc1e7c94b18622d92409/ci-records/charamel32-echo-terminal.json). This follow-up does not change permission policy, `tools_shell.ml`, the POSIX parser prefix, dependencies or repository workflows.

The proof covers the stated Cmd contracts, not all Cmd grammar. Native-valid quoted filenames with unmatched parentheses remain conservatively reviewed in all three observed parser versions. The four native-invalid ampersand/grouped observations are preserved separately. Windows verification covers formatting, the release build, and the selected shell/permission tests; it does not claim the unrelated full Windows suite is green.

ORDINARY_CI_STATUS


---

Addressed in [`FINAL_HEAD`](https://github.com/gosuda/Charamel/commit/FINAL_HEAD), tree `FINAL_TREE`.

The screen now records and removes only outer redirection spans, then strips the first and last quotes of the remaining command string. The inner filename quotes therefore reach the recursive parser intact: `type "draft (for review).txt"`. Redirections inside the quoted payload remain part of that payload, and the existing quoted outer-target controls are preserved.

The [native observation](https://github.com/gosuda/sloplint/actions/runs/45997335988) proves the reported TYPE command reads the owned file and a harmless child receives one intact filename argument. The [source comparison](https://github.com/gosuda/sloplint/actions/runs/45999925649) reproduces the current `d19f` failure. Nine maintained exact-input cases cover /s and non-/s invocation, literal/unwrapped controls, inner/outer/prefix redirects and the FOR body. A Windows permission test reads an owned temporary filename and compares the captured bytes exactly.

[Preparation](https://github.com/gosuda/sloplint/actions/runs/PREP_RUN) also reintroduces only the first-quote shortcut: the new maintained test fails while the previous 203 classifier and 48 input controls remain green. Candidate and restored source pass all 214 classifier / 57 input cases. [Fresh Linux/Windows gates](https://github.com/gosuda/sloplint/actions/runs/FULL_RUN) pass on the exact published tree; actual exits and artifacts are in [the terminal receipt](TERMINAL_URL). This retains the older unmatched-parenthesis filename limitation explicitly and makes no full Cmd-grammar claim.
