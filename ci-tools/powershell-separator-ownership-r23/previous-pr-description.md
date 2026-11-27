## Problem

PowerShell command boundaries can change the facts Sloplint reports. This PR corrects parsing and literal measurement around interpolation, comments, escaped newlines, Unicode separators and dynamic expressions. Native PowerShell controls distinguish real language behavior from parser acceptance.

The latest review found that continued bare-dash arguments became unreachable, branch lookahead could swallow a separate command, and redirected dash words lost legacy separator captures. The vendor recovery notes also described older behavior.

## Latest changes

Published source: [57adc9ff](https://github.com/gosuda/sloplint/commit/57adc9fff126a9826e0fb0be921c28e6cdb08391), tree `f518b3943c6b86af53c334175309303c3d79f38b`.

- [1f020557](https://github.com/gosuda/sloplint/commit/1f0205578757a91e3e089f022cd48d7282715bbd) gives real conditional continuations explicit hidden grammar guards. Native complete-token boundaries keep `else.exe` and ordinary commands named `else` separate. Existing public clause fields, wrappers and keyword ranges remain intact.
- [29796722](https://github.com/gosuda/sloplint/commit/29796722ab92bd53b03bab52e3648ebd19f425d7) gives whitespace delimiters priority over competing words. This restores indented bare dashes after LF/CRLF continuations and the original custom-query captures before redirected dash words, while preserving legacy token names and mixed native whitespace.
- [57adc9ff](https://github.com/gosuda/sloplint/commit/57adc9fff126a9826e0fb0be921c28e6cdb08391) documents partial parse recovery and the precise older limitations. Generated metadata is derived from the final files: ABI 15, 2,039 states, 2,473 patch lines and 44 hunks.

Earlier repairs for here-string dollar boundaries, interpolation, leading pipes, attached CR, literal Unicode escapes, complete format-mark words, comma boundaries, quotes and dynamic properties remain covered.

## Verification

[Native Depot run 45893490495](https://github.com/gosuda/sloplint/actions/runs/45893490495) completed successfully in preparation, x64, ARM64 and lint jobs on the exact final tree.

- All 108 focused PowerShell tests pass. The isolated conditional stage passes its 104-test suite and 258 native context comparisons.
- All 1,085 required parser comparisons pass: 653 retained cases and 432 context comparisons. Exact command extents, branch ownership, mixed case and malformed owned branches are checked.
- All 96 supported indented dash cases match actual native argv counts. The 52 prior native literal outputs and six unchanged near-control trees are retained.
- Five real public queries across 62 legacy-spacing redirection inputs reproduce the original capture ranges. All 24 earlier plain-call query cases remain unchanged.
- Rust 1.100.0: full Clippy, workspace tests with `--no-fail-fast` and release builds pass on native Linux x64 and ARM64. The x64 release CLI passes self-lint.
- Formatting, grammar inventory, release-script self-tests, pinned cargo-deny, authoritative upstream patch reapplication, all 269 corpus cases, fresh generation and whitespace checks pass.
- Every one of the 4,906 final tracked blobs is verified before and after the gates. Independent GitHub reconstruction matches all three staged trees. Public node types are unchanged.

The [corrected unchanged-source baseline](https://github.com/gosuda/sloplint/actions/runs/45888591536) preserves native observations and the keyword/query failures. The [narrowed causal baseline and rejected separator experiment](https://github.com/gosuda/sloplint/actions/runs/45892216991) preserve current-source exit 101 versus previous-source exit 0 for the 96 indented cases; that experiment later failed three candidate tests and was not published. The final run verifies the reused baseline ZIP, logs, frozen test bytes and restored source identities before applying the successful candidate.

Sixteen no-indent bare-dash observations remain older unsupported cases and are not used as the causal regression. Native PowerShell's separate single-dash redirect split (`-output` plus `.log`) is also an older limitation; the redirection repair restores the observable legacy query gap and target start.

| Evidence | Artifact | ZIP SHA-256 |
| --- | --- | --- |
| candidate | `15152068841` | `c25d14053a9d863047a5fe92f3c274ace82f843f628ca8578dce00408533f43f` |
| x64 | `15153156434` | `a653075e3a4ec2eff4af58f5e6022b764baec0917966c38e92e6f42f671eb15c` |
| arm64 | `15153271736` | `2de1cefb83d00063821ce374f5cad8b4f21f01e2c3ac29359745b6942a22b3b2` |
| lint | `15152238730` | `4b980144a985d3dbf43ec00afe6fc8ad5e5b41e0591b2ad7cf09fe9b4ebae4f0` |

The source payload SHA-256 is `e30dd5bed8dc97f6c0f9361e23a6d2c9644b28f178f3b68f4218995a80fc3c89`; the final source-index SHA-256 is `73b2ffef65c2b6087de35a3fcc1ef31b3e32968bf1696b4ce519537a9f54ed5f`.

[Durable source and evidence checkpoint](https://github.com/gosuda/sloplint/blob/03a275c9b8b72820d5bcb0925e43da127d0e25b9/ci-tools/powershell-context-boundaries-r22/checkpoint.json). The owned r22 proof workflow was removed additively in [a10db690](https://github.com/gosuda/sloplint/commit/a10db6900c5ed16b4be9530ca240d2624b1c98b5); the older r20/r21 proof workflows remain retired.

All 22 threads present at the r22 publication were resolved, and the [ordinary PR workflow](https://github.com/gosuda/sloplint/actions/runs/45895428017) completed successfully on this source. Three later findings arrived: the source-identity analysis thread has been reconciled and resolved; the two source findings about format-mark redirect words and inline comment parenting remain open. Their [unchanged-source native/query baseline](https://github.com/gosuda/sloplint/actions/runs/45897555856) is running before any further repair.

[Earlier source and verification history](https://github.com/gosuda/sloplint/blob/a10db6900c5ed16b4be9530ca240d2624b1c98b5/ci-tools/powershell-context-boundaries-r22/previous-pr-description.md) retains the previous descriptions and receipts.
