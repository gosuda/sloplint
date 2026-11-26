# Historical PR137 description before r22

Captured from maintained head `83f73f04e7f20bf405dbf4c2de3bd5467c8010bb` before the contextual-boundary follow-up. Status statements below describe their earlier publication checkpoints; use checkpoint.json for the current state.

## Current review follow-up

Published source: [83f73f04](https://github.com/gosuda/sloplint/commit/83f73f04e7f20bf405dbf4c2de3bd5467c8010bb), tree `703b3a5a445883598bfb7ef05c263a76d6629235`.

The latest three comments are addressed in separate additive commits:

- [0ef12ac2](https://github.com/gosuda/sloplint/commit/0ef12ac28a807a2607606e669c85bf1d403eb602) preserves `else`/`elseif` ownership after escaped lines in assignment-valued conditionals, including hashtable values. Leading pipes still stop at the native boundary.
- [23b5b13e](https://github.com/gosuda/sloplint/commit/23b5b13e23e70ba519ed054f1703be06ca29c490) preserves complete single- and double-dash words containing literal format marks. Native unescaped comma lists retain their array boundary; quotes, escapes, variable tails and named attached options retain their tested behavior.
- [83f73f04](https://github.com/gosuda/sloplint/commit/83f73f04e7f20bf405dbf4c2de3bd5467c8010bb) restores the public custom-query contract for anonymous space, tab and NBSP tokens while retaining the full native whitespace set.

## Verification of this source tree

[Native Depot run 45883465823](https://github.com/gosuda/sloplint/actions/runs/45883465823) completed successfully in all four jobs.

- All 99 PowerShell regressions pass. Each staged concern also passed its focused suite before the full architecture jobs started.
- All 653 required native parser, branch-structure and argument-extent comparisons pass, including 96 conditional-context cases, 54 complete-word/protected cases and 18 comma cases.
- Actual PowerShell argv captures verify the new argument counts and values. Six native near-controls, including `--format=csv,noheader`, ordinary parameters and a negative number, retain their exact pre-change parser trees.
- All 24 public custom-query capture cases match the original byte spans for single, repeated and mixed legacy separators. The 52 earlier native literal-value controls are retained.
- Rust 1.100.0: complete Clippy, workspace tests with `--no-fail-fast`, and release builds pass on native x64 and ARM64. The x64 release binary passes self-lint.
- Formatting, grammar inventory, release-script self-tests, pinned cargo-deny, authoritative upstream patch reapplication, all 269 corpus cases, fresh generation and whitespace checks pass.
- Every one of the final 4,905 tracked blobs was verified before and after the gates. Independent GitHub reconstruction matched all three staged source trees.

The [unchanged-source baseline](https://github.com/gosuda/sloplint/actions/runs/45881856636) records all three current regressions with exit 101. The prior source passes conditional ownership and legacy-query controls; its dash-word extent defect is recorded separately. The [initial baseline harness](https://github.com/gosuda/sloplint/actions/runs/45881064468) stopped at an incorrect per-character whitespace-capture assumption. Actual old capture spans were then frozen before the compatibility repair.

| Evidence | Artifact | ZIP SHA-256 |
| --- | --- | --- |
| candidate | `15148621955` | `824551d41d705fa028521e5158f85fe1065be25eb56bff1405bb080e3e8b64c4` |
| x64 | `15147644535` | `e7f1d038a2857995e23f2be6669f261cc90aab0e05dc5c8ffe73cab5d92fc07e` |
| arm64 | `15148118585` | `384151c08380ef3b692d1838f14aa4e7c60ff8d71328a0ebf2a9d401ef7338b8` |
| lint | `15148767534` | `c54bd9e8709f3a42aa2e33d6bb9e75fb8f086f3ae3a446fad2be6ca22f841298` |

[Durable source and evidence checkpoint](https://github.com/gosuda/sloplint/blob/bd31bc6746d6cb73eba6d579aa10ad5a2074e684/ci-tools/powershell-assignment-followup-r21/checkpoint.json). The owned r21 proof workflow has been retired additively.

All 18 current review threads are resolved. The [ordinary PR workflow](https://github.com/gosuda/sloplint/actions/runs/45884920364) is running on the exact published commit.

<details>
<summary>Previous source and verification history</summary>

## Previous verified follow-up (3071f522)

Published source: [3071f522](https://github.com/gosuda/sloplint/commit/3071f522049cbe2d440bc68ca20e8887fdbdaa9a), tree `171bbe1c7137faf98fb7981f69288120cd18bdf6`.

This three-commit follow-up fixes the newly reported argument-separator, plain Unicode-escape, and leading-pipe findings:

- [1e5d66c0](https://github.com/gosuda/sloplint/commit/1e5d66c085fdc842d6ab4903c0c9ca7faafee112) applies PowerShell's complete horizontal whitespace set to argument separators and both sides of bare dashes. Protected spaces and literal format marks retain their argument boundaries.
- [4fda478b](https://github.com/gosuda/sloplint/commit/4fda478b09c1b07ba9cdfd6cc5ad79437e623bac) retains `Str` facts while measuring actual searchable runs in eligible plain words containing Unicode escapes. The short-message and eight-character near-miss CLI fixtures preserve the existing threshold.
- [3071f522](https://github.com/gosuda/sloplint/commit/3071f522049cbe2d440bc68ca20e8887fdbdaa9a) stops leading-pipe lookahead at a backtick after an ordinary newline and preserves an attached escaped CR in the command token. The original attached-CR input remains an exact-name regression; the intended continuation fixture now starts at a token boundary.

## Verification of the published source tree

[Final native Depot run 45875733057](https://github.com/gosuda/sloplint/actions/runs/45875733057) completed successfully in all four jobs.

- 485 native parser and branch-structure comparisons, 190 actual native argument captures, and 52 native runtime literal-value cases.
- All 94 PowerShell regressions, including mixed LF/CRLF input and 16 preserved `else`/`elseif` structures.
- Rust 1.100.0: complete Clippy, workspace tests and release builds on native x64 and ARM64; the x64 release CLI passed self-lint.
- Formatting, grammar inventory, release-script self-tests, pinned cargo-deny, authoritative upstream patch reapplication, all 269 corpus parses, fresh generation and whitespace checks.
- Every one of the final 4,902 tracked blobs was verified before and after the gates. Independent GitHub reconstruction matched all three staged trees exactly.

The [original unchanged-source probe](https://github.com/gosuda/sloplint/actions/runs/45870222096) recorded all three primary Rust regressions and the missing CLI finding with exit 101. Its final assertion also caught two mistaken CRLF acceptance hypotheses, which the [native-only refinement](https://github.com/gosuda/sloplint/actions/runs/45871471507) corrected without changing their input bytes. The [first candidate gate](https://github.com/gosuda/sloplint/actions/runs/45873249049) exposed the older attached-CR fixture assumption. [Native token controls](https://github.com/gosuda/sloplint/actions/runs/45874990715) established the precise command extent before the test correction above.

| Evidence | Artifact | ZIP SHA-256 |
| --- | --- | --- |
| candidate | `15144733307` | `48d9c71c051557b5d95084deb51830134abf334376b28909f7686890533ed8d3` |
| x64 | `15143879984` | `9d2f665e08b0bb3f4a4874a5edd899600f469b42e99a1a90f1ece4b6ba5728c9` |
| arm64 | `15144654689` | `c24e14ff20ed5b5d38f66c38679d6d4b677dd36cf4f70587136595e8670609f8` |
| lint | `15143974210` | `adc6eb68ae622673418c3c3786dfd7dbe3e172c1872394efa42e55668e49cd5d` |

[Durable source and evidence checkpoint](https://github.com/gosuda/sloplint/blob/3a2ede51f89777df54121ca9e65d572bb497826f/ci-tools/powershell-three-findings-r20/checkpoint.json). Both temporary proof workflows have been retired additively.

The [ordinary PR workflow](https://github.com/gosuda/sloplint/actions/runs/45877442939) completed successfully on 3071f522.

<details>
<summary>Earlier source and verification history</summary>

## Previous review follow-up

Published source: [61c88ec4](https://github.com/gosuda/sloplint/commit/61c88ec4d492dd1ab72f76b15b84fa9434cccfaa), tree `589aff2ec97f6e6f8179453ff6882fe19b3b65b2`.

This follow-up fixes the six current findings. It preserves statement boundaries around comments, recognizes native Unicode whitespace in both lexers, uses one quote/escape scan for generic words, and treats computed subexpressions as dynamic holes. It adds parser and CLI positive/near-miss controls while retaining the valid tests from the intervening commits.

[The frozen baseline](https://github.com/gosuda/sloplint/actions/runs/45857207108) recorded 27 native grammar outcomes and matched 38 runtime values. All four newly frozen Rust regressions failed against the unchanged prior source.

[Native x64 verification](https://github.com/gosuda/sloplint/actions/runs/45858799234) passed 178 grammar comparisons, 39 runtime-value cases, corpus and upstream patch verification, the regressions and CLI controls, full Clippy/workspace tests, release build and self-lint. Its final whitespace check rejected 75 blank context prefixes in the generated patch. Only those prefixes were normalized afterward.

[The final native run](https://github.com/gosuda/sloplint/actions/runs/45862209359) passed formatting, grammar inventory, 66 release-script self-tests, cargo-deny, authoritative patch reapplication, fresh regeneration with unchanged generated files, all 269 corpus parses, whitespace, and native ARM64 Clippy/workspace/release. The final tree was verified against every tracked blob. The published commit preserves the intervening formatting commit as its parent; its source tree is identical to the verified candidate.

## Problem

The PowerShell scanner rejected valid here-string dollar boundaries, accepted malformed interpolation and blank-line pipeline continuations, and mismeasured searchable literal text in interpolated paths. Those errors can produce both missed and incorrect lint findings.

## Changes

- Preserve here-string interpolation openers after a literal-dollar line, including LF and CRLF input. Empty and unclosed braced variables remain errors.
- Apply the statement-terminator rule to parenthesized pipelines as well as top-level pipelines.
- Measure command-word literal spans by quote mode. Exclude hidden variable prefixes and quote delimiters, preserve literal adjacency, and treat backticks as literal inside single quotes.
- Match PowerShell's UTF-16 variable-name categories, including the distinction between decimal digits and other Unicode numbers or astral letters.
- Keep the lexical scanner in a private module within the repository's file-size budget. Update the two older path assertions to verify one concatenated argument and its static suffix.
- Preserve the explicit escape-byte API from the original follow-up, and regenerate the vendored grammar, parser and pinned upstream patch together.

This additive revision is `82ad44d49ca3c33060f4585dc8582697f603d118`, tree `8c74e01941bab1b45ff77d1f83b6a898644ca236`, on main `dc99ed955cf78862a6f8811289f606de1fb50dfa`. The verification workflow remains outside this feature tree.

## Verification

[Native Depot run 45836719168](https://github.com/gosuda/sloplint/actions/runs/45836719168) passed all three jobs against that exact source.

- PowerShell 7.7.0 agreed with all 151 grammar cases and all 26 literal-value oracles.
- Tree-sitter 0.27.1 regeneration, the complete vendored corpus and pinned upstream regeneration passed. The parser has 2,017 states; the upstream patch has 2,355 lines and 49 hunks.
- The old literal adapter failed the frozen regressions and actual CLI fixtures (exit 101). The earlier broad Unicode predicate failed the four category-boundary cases (exit 101). Restored source passed all 70 focused PowerShell tests.
- The separately frozen grammar baseline failed 46 of the 151 cases; its repaired grammar files are byte-identical in this combined candidate. [Grammar control run](https://github.com/gosuda/sloplint/actions/runs/45827219552), artifact `15121005011`, ZIP SHA-256 `4212bb37e560ca49113d6ea0a727835fef3dc103fa773962e9754631ae585b62`.
- Rust 1.100.0: full workspace Clippy, workspace tests and release builds passed on native x64 and ARM Depot runners. The x64 CLI self-lint returned 0 with no findings.
- Formatting, grammar manifest checks, release-script self-tests, cargo-deny, whitespace and source identity checks all returned 0. Every temporary control was restored before the final source check.

| Evidence | Artifact | ZIP SHA-256 |
| --- | --- | --- |
| x64 | `15126582589` | `1ca429ce61cf579def70ee78b31ab9f8f092a8aab8f157d2e82297bfcc498f6c` |
| ARM | `15126707610` | `bad8afc9dd26b3a0a91776bc89967a1fade179a031eb6b6f95092938f1109400` |
| lint | `15125567791` | `5dffbf847d3f5eef879e6c0b0950b1b4e2669c9724470fc96172a729ef2b6ea7` |

Earlier candidate runs are retained as failure evidence. They exposed two stale literal-length assertions and the scanner file-size limit; the current gates pass after those corrections. The [original follow-up verification](https://github.com/gosuda/sloplint/actions/runs/45816400772) applies to the earlier tree, not this final revision.

</details>


</details>
