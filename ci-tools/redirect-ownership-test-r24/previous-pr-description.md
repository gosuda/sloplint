## Problem

Block comments must preserve the boundary between a PowerShell command and its arguments. The standalone comment element introduced in the preceding revision made a valid adjacent word fail parsing and could finish a redirection before its numeric target:

- `Write-Output <# note #>hello`
- `Write-Output hello > <# note #> 1`

Both defects change the facts available to lint rules. Custom rules also depend on the existing comment parents and separator spans.

## Changes

Published source: [d972b52788bb](https://github.com/gosuda/sloplint/commit/d972b52788bbdf88265e50df1a2502663e597df7), tree `a026522088e19dd38731146c90956650506be913`.

- Keep comments as trivia while selecting the next argument. Hidden zero-width guards preserve the historical direct comment parent before named parameters and words containing dash/format-mark prefixes.
- Retain the earlier separator parent for comment groups separated by whitespace. Ordinary word-facing separators keep their existing captures.
- Keep attached parameter-shaped filenames inside the existing redirection path. Adjacent words and numeric redirect targets retain their argument boundaries.
- Preserve the externally authored leading-format-mark word alternative from [e7ee97e8](https://github.com/gosuda/sloplint/commit/e7ee97e84246863766ec7465777a035134533feb).
- Add eight permanent public-API regressions, regenerate the parser and upstream-relative patch, and document the bounded behavior. Generated metadata is derived from the final files: ABI 15, 2,065 states, 2,621 patch lines and 44 hunks.

The existing conditional-branch guards, Unicode separators, continuation rules, interpolation and literal measurement repairs remain covered.

## Verification

[Native Depot run 45915713157](https://github.com/gosuda/sloplint/actions/runs/45915713157) passed preparation, x64, ARM64 and lint on the exact final application tree.

- All 116 permanent focused PowerShell regressions pass.
- The ownership proof repeats 805 native observations, 801 parser-tree comparisons and 21 real public queries across 803 inputs. It preserves accepted and rejected observations, recovery state, byte positions and captured text.
- The 18 comma/quote/escape argument cases pass an isolated test invocation with every case ID and native count checked. Both extra-space redirection witnesses retain their target extents.
- All 653 retained parser cases, 432 context comparisons, 96 supported indented-continuation argv counts, 52 literal outputs and six unchanged near-control trees pass.
- Five legacy queries across 62 redirection inputs and all 24 earlier plain-call query controls retain their original capture ranges.
- Rust 1.100.0: full Clippy, workspace tests with `--no-fail-fast` and release builds pass on native Linux x64 and ARM64. The x64 release CLI passes self-lint.
- Formatting, grammar inventory, release-script self-tests, pinned cargo-deny, authoritative patch reapplication, all 269 corpus cases, fresh generation and whitespace checks pass.
- Every one of the 4,907 tracked blobs is verified before and after the gates. Independent GitHub reconstruction matches the final tree. Public node types match the pre-regression metadata.

The native single-dash filename split (`-output` plus `.log`), attached-CRLF command split and 16 no-indent bare-dash cases remain explicitly recorded historical limitations.

## Evidence and validation history

The [current-head baseline](https://github.com/gosuda/sloplint/actions/runs/45908737910) records the two new regressions on the unmodified source, while retaining the older query contracts. Earlier rejected candidates and the diagnostic output-reader failure remain recorded as failures. [Artifact-only inspection](https://github.com/gosuda/sloplint/actions/runs/45915194176) found the intact eighteenth count record immediately after libtest's `ok` text; the final source run authenticates that log and independently repeats all counts.

| Artifact | ID | ZIP SHA-256 |
|---|---:|---|
| Candidate | 15161574716 | `c5f1b4a0ef22e691426b82d18bb2ba2c7055549b01b650a877a8b485df508e06` |
| x64 | 15162223640 | `8f447ee6d0f1230ea5d64b882e05b7d8591a10c4ba4a3d71593e8e25df813f84` |
| ARM64 | 15162726535 | `00314946b5ccd22a7c2cd29024c611c6f46162a673c789f8c0ff77d6f6f8f6e2` |
| Lint | 15162770393 | `46755c24b7ea4795ab26bb1cd1f9376ee092252ceb0b2f365061fc2a9119e759` |

Source payload SHA-256: `6acebcae9d772ebd692f1387f59a66ffaf78849cf1541321a8f30d7a9aaee353`.
Source-index SHA-256: `0998a8f0606f2df45d28976c5a607f3e3401fae6a22b5253f16fa96405537283`.

[Immutable final checkpoint](https://github.com/gosuda/sloplint/blob/4653b2d295d848994b03f5a9f901f17f34e2b60a/ci-tools/powershell-separator-ownership-r23/checkpoint.json) records the exact source, gate exits, artifact digests, rejected experiments and observer correction. [04bbfba1f2fc](https://github.com/gosuda/sloplint/commit/04bbfba1f2fcba1d8df8b9e77e90794c41b13f32) removes the owned r23 proof workflow. The checkpoint also preserves the previous PR description.

[Ordinary PR CI 45917020535](https://github.com/gosuda/sloplint/actions/runs/45917020535) completed successfully on the published head: x64, ARM64 and lint passed. Its merge commit `b7f13dd9cfaa58c9b5f6a2853c4e0d158a408c32` has the same final source tree. The scheduled native-platform matrix is skipped for this ordinary PR event, as configured.

[Earlier source and verification history](https://github.com/gosuda/sloplint/blob/03a275c9b8b72820d5bcb0925e43da127d0e25b9/ci-tools/powershell-context-boundaries-r22/checkpoint.json) preserves r22 and the preceding repairs. The r20/r21/r22 proof workflows remain retired.
