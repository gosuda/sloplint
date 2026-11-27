# Gosuda public CI and PR audit

Snapshot R28: 2026-11-27T02:51:15.733Z to 2026-11-27T02:57:57.472Z. This public report omits private repository identities, source revisions and findings. PR titles, branch names and review bodies are omitted from the data projection, including private-origin metadata in the public landing mirror.

## Current result

All runner selections on the **150 open public PR heads** use Depot, or the head contains no runner jobs. Every declared macOS runner on those heads is exactly `depot-macos-latest`. No Ubuntu 26.04 runner selection was found.

The public scope contains **115 repositories, 150 PRs across 52 repositories, 430 review threads and 958 inline comments**. Canonical open-PR lists were read for every repository. All inline-comment counts agree with GitHub's REST counts. No read failed and no mergeability value is unknown.

The runner audit covers **289 immutable source commits, 244 unique YAML blobs and 772 unique job definitions**. It includes current defaults, actual base refs and maintained PR heads. Static labels, finite matrix variants, local reusable workflows and cargo-dist runner mappings are resolved. The report verifies declared selection; it does not claim that every CI run passed or estimate billing savings.

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

There are **26 unresolved public review threads on seven PRs**.

| PR | Threads | Exact current head | State |
| --- | ---: | --- | --- |
| [gosuda/gglint #33](https://github.com/gosuda/gglint/pull/33) | 5 | `b7f2808eb6b9` | Preserved pause |
| [gosuda/LoGOs #334](https://github.com/gosuda/LoGOs/pull/334) | 1 | `682c21d440d5` | Preserved pause |
| [gosuda/LoGOs #336](https://github.com/gosuda/LoGOs/pull/336) | 5 | `aaabdc702edc` | Preserved pause |
| [gosuda/LoGOs #339](https://github.com/gosuda/LoGOs/pull/339) | 3 | `1cccd8960b7a` | Preserved pause |
| [gosuda/LoGOs #342](https://github.com/gosuda/LoGOs/pull/342) | 4 | `9cb0ef15ac9a` | Preserved pause |
| [gosuda/LoGOs #343](https://github.com/gosuda/LoGOs/pull/343) | 5 | `1ea43146195b` | Preserved pause |
| [gosuda/ruk #20](https://github.com/gosuda/ruk/pull/20) | 3 | `adbb6ff975bb` | Preserved pause |

LoGOs source changes, gglint repairs, BiBeam source/dependency-gate investigation and Ruk20 publication remain within a preserved execution pause. The agent registry reports a possible-cybersecurity-risk flag on the root worker, but it does not identify a repository, action, tool call or PR. No separate action-specific rejection record has been recovered. This report does not assign an unsupported repository-specific reason or retry those actions.

One actual public conflict remains: [gosuda/ssh-chat #1](https://github.com/gosuda/ssh-chat/pull/1). Its PR head is in `fatheradvisor/ssh-chat`, outside the authorized write boundary. The branch was not changed.

## Evidence and limits

The complete public snapshot is `ci-tools/gosuda-pr-audit-20261126-public.json`, SHA-256 `ae37fef792f99741665670790c9df42e3dd8193d8a2cadb34afd82ce274a6127`. It records source heads, actual bases, review resolution, runner declarations and observation times. Files cached from earlier reads are reused only by identical immutable source commit and Git blob SHA.

This is a finite, non-atomic read window. Later pushes, comments, closures and merges can change the result. An unstable check state is not treated as a merge conflict. Review resolution is determined by `is_resolved`, including outdated threads.

All publishing was additive. No force push, commit amendment, PR merge, automatic merge, archive change or repository-setting change was performed.
