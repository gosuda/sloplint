# Gosuda CI and PR reconciliation

Observed through 2026-11-26T19:04:06.302Z. This is a finite snapshot, not an atomic organization-wide transaction.

## Runner result

Every audited maintained PR head selects Depot runners or contains no runner jobs. All declared macOS selections on those heads are exactly `depot-macos-latest`. No Ubuntu 26.04 runner selection was found.

| Scope | Count |
| --- | ---: |
| Repositories | 115 |
| Open PRs | 149 |
| Exact source commits inspected | 288 |
| Unique workflow blobs | 245 |
| Unique job definitions | 789 |
| Open PR heads with non-Depot or forbidden runner labels | 0 |
| Unresolved review threads | 31 |
| Confirmed merge conflicts | 7 |
| Unknown mergeability states | 0 |

## Default branches still awaiting rollout

| Repository | Disposition |
| --- | --- |
| [gosuda/bitcoin-wallet](https://github.com/gosuda/bitcoin-wallet) | Migration is published in [PR #76](https://github.com/gosuda/bitcoin-wallet/pull/76); the PR remains unmerged. |
| [gosuda/BiBeam](https://github.com/gosuda/BiBeam) | Migration is published in [PR #24](https://github.com/gosuda/BiBeam/pull/24); the PR remains unmerged. |
| [gosuda/gstime](https://github.com/gosuda/gstime) | Migration is published in [PR #10](https://github.com/gosuda/gstime/pull/10); the PR remains unmerged. |

PR publication does not change an unmerged default branch. No PR was merged, force-pushed, amended or configured for auto-merge.

## Concrete CI reductions

| Change | Reduction and retained coverage |
| --- | --- |
| [BiBeam #24](https://github.com/gosuda/BiBeam/pull/24) | Routine PR execution drops from eight CI jobs plus a release-plan job to two Linux jobs. Full nextest and the 50% coverage floor remain. Native macOS and Windows tests move to weekly/manual runs. All five release targets remain; Intel macOS artifacts cross-build on Apple Silicon. |
| [gstime #10](https://github.com/gosuda/gstime/pull/10) | Routine execution drops from three jobs to two by sharing Linux setup for static checks, race tests and integration tests. Linux/macOS tests and parser fuzz coverage remain. |
| [bitcoin-wallet #76](https://github.com/gosuda/bitcoin-wallet/pull/76) | Core tests and release linking run once on Linux, frontend/WASM setup is consolidated, and CodeQL runs weekly/manually. All ten required check names, native Clippy checks and release artifact sets remain. Native Linux, Windows, macOS, Android debug and iOS Simulator packaging were verified; signed-device packaging was skipped when keys were absent. |

These are changes to repository-controlled workflows. No billing saving or universal CI success is inferred from job-count reductions.

## Completed follow-up repairs

| PR and exact head | Result | Native proof | Ordinary PR workflow |
| --- | --- | --- | --- |
| [gosuda/flats #266](https://github.com/gosuda/flats/pull/266) at [`066c32dcf2e0`](https://github.com/gosuda/flats/commit/066c32dcf2e0d2a9d9ea0d41e7aacf68524c1518) | Measure settled notebook heap after the third actual read, excluding the test-owned response without subtracting response capacity. Independent design, implementation and final testing reviews accepted; both follow-up threads resolved. | [success](https://github.com/gosuda/sloplint/actions/runs/45861924362) | [success](https://github.com/gosuda/flats/actions/runs/45868856946) |
| [gosuda/flats #273](https://github.com/gosuda/flats/pull/273) at [`17b44ec73e98`](https://github.com/gosuda/flats/commit/17b44ec73e9826c8d09ccc438f81fd543cb9701f) | Exercise backward deletion with a rendered Mermaid widget in both keyboard and native-input paths; all 20 browser cases retained. Independent implementation carry-forward and final testing reviews accepted; the follow-up thread resolved. | [success](https://github.com/gosuda/sloplint/actions/runs/45865269020) | [success](https://github.com/gosuda/flats/actions/runs/45869522998) |
| [gosuda/bamTiScript #253](https://github.com/gosuda/bamTiScript/pull/253) at [`c5c3ec93ec4b`](https://github.com/gosuda/bamTiScript/commit/c5c3ec93ec4b6c355054503f368b2ae874a2bfa0) | Verify both invalid source-alias union orders and compatible controls through the actual binder, compared with TypeScript 7.0.2. The follow-up thread resolved; all 25 threads are resolved at the snapshot. | [success](https://github.com/gosuda/sloplint/actions/runs/45865746729) | [success](https://github.com/gosuda/bamTiScript/actions/runs/45867667858) |

Each completed repair has an immutable checkpoint linking its source tree, native run and probe retirement. Earlier failed or expected-failure controls remain in that evidence; they are not presented as passing full runs.

## Remaining review and conflict work

| PR | Unresolved threads | Merge conflict |
| --- | ---: | --- |
| [gosuda/ssh-chat #1](https://github.com/gosuda/ssh-chat/pull/1) | 0 | Confirmed |
| [gosuda/ruk #20](https://github.com/gosuda/ruk/pull/20) | 2 | No confirmed conflict |
| [gosuda/sloplint #137](https://github.com/gosuda/sloplint/pull/137) | 3 | No confirmed conflict |
| [gosuda/LoGOs #343](https://github.com/gosuda/LoGOs/pull/343) | 3 | No confirmed conflict |
| [gosuda/LoGOs #342](https://github.com/gosuda/LoGOs/pull/342) | 4 | No confirmed conflict |
| [gosuda/LoGOs #339](https://github.com/gosuda/LoGOs/pull/339) | 1 | Confirmed |
| [gosuda/LoGOs #336](https://github.com/gosuda/LoGOs/pull/336) | 0 | Confirmed |
| [gosuda/LoGOs #334](https://github.com/gosuda/LoGOs/pull/334) | 0 | Confirmed |
| [gosuda/LoGOs #328](https://github.com/gosuda/LoGOs/pull/328) | 13 | Confirmed |
| [gosuda/LoGOs #322](https://github.com/gosuda/LoGOs/pull/322) | 0 | Confirmed |
| [gosuda/LoGOs #306](https://github.com/gosuda/LoGOs/pull/306) | 0 | Confirmed |
| [gosuda/gglint #33](https://github.com/gosuda/gglint/pull/33) | 5 | No confirmed conflict |

## Audit method and limits

Both paginated repository catalogs agree. Canonical per-repository open-PR lists are authoritative; search indexing is not used as the completeness boundary. Thread state comes from `is_resolved`, and inline comment counts agree with REST metadata. Actual base commits come from raw target refs.

Workflow YAML is parsed, finite matrices and conditional literal matrices are enumerated, local reusable workflows are resolved, and generated cargo-dist matrices are checked against the exact configuration and pinned generator source. Unchanged source is reused only by immutable commit or blob identity; original source observation times remain in the JSON.

The audit covers current defaults, actual PR bases and maintained open PR heads. Retired diagnostic branches and historical commits are outside that scope. Runner selection does not prove runtime availability or that all application checks pass. Known baseline failures and uncompleted source repairs remain explicit in their PR evidence.
