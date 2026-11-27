# Gosuda public CI and PR audit

Snapshot R32: canonical repository, PR and review reads from 2026-11-27 05:12:04.090 to 05:16:46.411 UTC. Individual source records preserve their observation times. This public report omits private repository identities, findings and evidence. PR titles, branch names and review bodies are omitted, including private-origin metadata in the public landing mirror.

## Current result

All runner selections on the **159 open public PR heads** use Depot, or the head contains no runner jobs. Every declared macOS runner on those heads is exactly `depot-macos-latest`. No Ubuntu 26.04 selection was found.

The public scope contains **115 repositories, 159 PRs across 52 repositories, 468 review threads and 1,012 inline comments**. Canonical open-PR lists were read for every repository. Inline-comment counts reconcile with GitHub's REST counts, and no mergeability value is unknown. R32 had no unexpected failed reads; expected empty-repository and absent-workflow responses are recorded separately.

The runner audit covers **299 immutable source commits, 245 unique YAML blobs and 778 unique job definitions**, across 570 workflow occurrences. Current defaults, actual base refs and maintained PR heads are included. Static labels, finite matrices, local reusable workflows and cargo-dist runner mappings are resolved. This verifies declared selection, not universal runtime success or billing savings.

Eight new PRs appeared since R29 and contribute 28 unresolved threads. Several existing heads also changed externally. The new workflow blob in elegant-mermaid #28 contains six weekly/manual jobs on Depot Ubuntu or Windows. The other new inventories reused earlier definitions only where immutable workflow blobs matched. The public landing mirror still has 26 open PRs, with no additions after #120 or closures during this refresh.

## Default branches still awaiting published migrations

| Repository | Published change |
| --- | --- |
| [bitcoin-wallet](https://github.com/gosuda/bitcoin-wallet) | [PR #76](https://github.com/gosuda/bitcoin-wallet/pull/76) |
| [BiBeam](https://github.com/gosuda/BiBeam) | [PR #24](https://github.com/gosuda/BiBeam/pull/24) |
| [gstime](https://github.com/gosuda/gstime) | [PR #10](https://github.com/gosuda/gstime/pull/10) |

These active public default branches still contain legacy runner labels. Their audited open PR heads already carry Depot selections. No PR has been merged by this audit.

## Published CI reductions

| Change | Reduction and retained coverage |
| --- | --- |
| [BiBeam #24](https://github.com/gosuda/BiBeam/pull/24) | Routine PR execution drops from eight CI jobs plus a release-plan job to two Linux jobs. Full nextest and the 50% coverage floor remain. Native macOS and Windows tests move to weekly/manual runs. All five release targets remain; Intel macOS artifacts cross-build on Apple Silicon. |
| [gstime #10](https://github.com/gosuda/gstime/pull/10) | Routine execution drops from three jobs to two by sharing Linux setup for static checks, race tests and integration tests. Linux/macOS tests and parser fuzz coverage remain. Validation for this reduction is static; a universal green native gate is not claimed. |
| [bitcoin-wallet #76](https://github.com/gosuda/bitcoin-wallet/pull/76) | Core tests and release linking run once on Linux, frontend/WASM setup is consolidated, and CodeQL runs weekly/manually. All ten required check names, native Clippy checks and release artifact sets remain. Native Linux, Windows, macOS, Android debug and iOS Simulator packaging were verified; signed-device packaging was skipped when keys were absent. Existing advisory findings are not waived. |

## Recent verified source publications

| PR | Exact published head | Verification |
| --- | --- | --- |
| [gosuda/flats #266](https://github.com/gosuda/flats/pull/266) | `066c32dcf2e0` | [Native 45861924362](https://github.com/gosuda/sloplint/actions/runs/45861924362) / [ordinary 45868856946](https://github.com/gosuda/flats/actions/runs/45868856946) |
| [gosuda/flats #273](https://github.com/gosuda/flats/pull/273) | `17b44ec73e98` | [Native 45865269020](https://github.com/gosuda/sloplint/actions/runs/45865269020) / [ordinary 45869522998](https://github.com/gosuda/flats/actions/runs/45869522998) |
| [gosuda/bamTiScript #253](https://github.com/gosuda/bamTiScript/pull/253) | `c5c3ec93ec4b` | [Native 45865746729](https://github.com/gosuda/sloplint/actions/runs/45865746729) / [ordinary 45867667858](https://github.com/gosuda/bamTiScript/actions/runs/45867667858) |
| [gosuda/sloplint #137](https://github.com/gosuda/sloplint/pull/137) | `530519c0040c` | [Native 45920800138](https://github.com/gosuda/sloplint/actions/runs/45920800138) / [ordinary 45921682807](https://github.com/gosuda/sloplint/actions/runs/45921682807) |

The published changes have native evidence and separate successful ordinary CI. Their owned temporary proof workflows were removed additively. The evidence preserves failed experiments and actual command exits; no failed run was relabeled as a successful run.

For sloplint #137, the published repair verifies complete PowerShell redirection ownership through immutable parser, CLI and Lean tests. Its first test gate failed because of stale Cargo target reuse; that receipt remains preserved. The passing gate compiled the exact candidate into a fresh isolated target. The report does not claim that a native PowerShell API supplied a complete redirection-end extent.

## Remaining review threads

There are **49 unresolved public review threads on 14 PRs** in this snapshot. The active repair queue includes the seven new PRs outside the preserved pause. Source changes require their applicable repository gates before publication and thread resolution.

| PR | Threads | Exact current head | State |
| --- | ---: | --- | --- |
| [gosuda/bamTiScript #260](https://github.com/gosuda/bamTiScript/pull/260) | 2 | `97b2682378c9` | Active repair queue |
| [gosuda/Charamel #32](https://github.com/gosuda/Charamel/pull/32) | 4 | `53cdbc3dd32e` | Active repair queue |
| [gosuda/dstack-skills #34](https://github.com/gosuda/dstack-skills/pull/34) | 1 | `3fd028e70964` | Active repair queue |
| [gosuda/elegant-mermaid #28](https://github.com/gosuda/elegant-mermaid/pull/28) | 5 | `2b61fca73a31` | Active repair queue |
| [gosuda/gglint #33](https://github.com/gosuda/gglint/pull/33) | 2 | `dcb7e6be40f9` | Preserved pause |
| [gosuda/LoGOs #336](https://github.com/gosuda/LoGOs/pull/336) | 3 | `8d676fe1e595` | Preserved pause |
| [gosuda/LoGOs #339](https://github.com/gosuda/LoGOs/pull/339) | 3 | `1cccd8960b7a` | Preserved pause |
| [gosuda/LoGOs #342](https://github.com/gosuda/LoGOs/pull/342) | 5 | `1cd2591d1fa9` | Preserved pause |
| [gosuda/LoGOs #343](https://github.com/gosuda/LoGOs/pull/343) | 5 | `1ea43146195b` | Preserved pause |
| [gosuda/LoGOs #344](https://github.com/gosuda/LoGOs/pull/344) | 3 | `6cd2829ee92e` | Preserved pause |
| [gosuda/portal-tunnel #851](https://github.com/gosuda/portal-tunnel/pull/851) | 2 | `ac6ade92d901` | Active repair queue |
| [gosuda/ruk #20](https://github.com/gosuda/ruk/pull/20) | 3 | `adbb6ff975bb` | Preserved pause |
| [gosuda/sloplint #138](https://github.com/gosuda/sloplint/pull/138) | 2 | `e2608e57b277` | Active repair queue |
| [gosuda/spira #47](https://github.com/gosuda/spira/pull/47) | 9 | `76ec82d9307d` | Active repair queue |

LoGOs source changes, gglint repairs, BiBeam source/dependency-gate investigation and Ruk20 publication remain within a preserved execution pause. The agent registry reports a possible-cybersecurity-risk flag on the root worker, without identifying a repository, action, tool call or PR. No separate action-specific rejection record has been recovered. No unsupported repository-specific reason is assigned, and those actions have not been retried.

One actual conflict remains: [gosuda/ssh-chat #1](https://github.com/gosuda/ssh-chat/pull/1). Its head belongs to `fatheradvisor/ssh-chat`, outside the authorized write boundary. A blocked check status is not classified as a merge conflict.

## Evidence and limits

The complete public snapshot is `ci-tools/gosuda-pr-audit-20261126-public.json`, SHA-256 `55f6073e20a2b8902e45cf9d33e4a3e8154ddf9c9b084ad4ce0e0ac4626060de`. It records exact heads, actual bases, review resolution, runner declarations and observation times.

This is a finite, non-atomic read window. Later pushes, reviews and merges can change the result. Review resolution uses `is_resolved`, including outdated threads. The newer active source work will be recorded separately after validation and publication; this snapshot does not predict its outcome.

All publication remains additive. No force push, commit amendment, PR merge, automatic merge, archive change or repository-setting change was performed.
