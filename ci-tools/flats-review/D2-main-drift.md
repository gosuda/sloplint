# D2 impact assessment: main advanced through PR271

The accepted design remains D2, SHA-256 9c6b81ac08581ceff96254731ad158ca7d362d6d0ecbf6a8d525eecdf84d4162. This assessment requests renewed design acceptance for D2 plus this exact delta. No maintained Flats head or source object has been changed for this task.

## Identity delta

Actual main advanced from 81235c28ba7d51ea12aad4740da198d54b8a59b5 to 427f1afc69ff671bd9d159f4de463ee2e00d677a in one commit, merging PR271. The six PR heads remain:
- 272 bc4b4cdb4d78fb2b70aaf7fbdb6ae66a8d143815
- 265 f8badac07f5745bcd71b55de30778c0585408336
- 248 863801718fce4cb188b072cd29e0e170a3096af2
- 236 bc89bb1912422cb7d73b6864fdf4570978fd4438
- 242 fc50dec92555539a029b3c6040c221b28cec319a
- 243 2a51d466e3d4dc848d82ed99954d4f1ab2dd81de

The new commit changes exactly three paths:
- .github/workflows/ci.yml: TMP and TEMP point to runner.temp on the configuration, open-file core, backup-junction and running-host backup steps. The site path regression keeps its system-drive environment. There is no job-wide TMP/TEMP override.
- internal/app/backupcmd_test.go: the existing running-host backup assertion compares the path emitted by the command with filepath.EvalSymlinks(out), propagating a failed canonicalization through t.Fatal.
- scripts/test_ci_shards.py: verifies those environment placements.

Repository instructions, protocol, specifications, source behavior and dependency inputs do not change in this main delta.

## Implementation impact

Use 427f1afc69ff671bd9d159f4de463ee2e00d677a as the actual main integration and comparison target. Preserve all three changes. The native Windows design already selected main's targeted checks; their precise environment now comes from this corrected main. Preserve the system-drive site fixture, put each of the other four named native checks on runner.temp, and place the additional PR248 Sheet native test on runner.temp without a job-wide override. Update the shard test to count that additional Sheet step only on248 and verify its placement.

Carry only the canonical-path assertion hunk into other older heads that contain the existing backup test, with exact context checks. Do not replace the whole test file: PR248 has additional Sheet coverage. Reads confirm the four independent source roots 236,248,265,272 contain the old assertion;242 and243 inherit236 and will be checked against their final intended parents. This is a test oracle repair to an existing platform contract, not a production change.

The ordinary three-way merge on248 now includes the canonical backup assertion in addition to the previous main changes. Preserve all feature additions. Final candidates, parents, changed-file manifests and stage reviews bind this updated target and the exact stacked parent commits.

## Verification and evidence carry-forward

D2's runner labels, event matrix, cancellation, selector fail-open rule, routine shards, sequential race lanes, cold bounds, lifecycle controls, Backspace witness, stack order and publication conditions are unchanged. There is no prior implementation or testing acceptance to carry forward. The accepted design receipt may be carried only after the eligible reviewer accepts this impact assessment.

Run the final native Windows steps and the actual workflow/shard tests on each applicable candidate. Record canonical-path behavior and the distinct site versus temporary-directory environments. Preserve all previous required gates and cold-run evidence requirements. Fresh actual refs are checked again before writes; later relevant drift needs another explicit impact disposition.

The full compare response is the primary source:
https://github.com/gosuda/flats/compare/81235c28ba7d51ea12aad4740da198d54b8a59b5...427f1afc69ff671bd9d159f4de463ee2e00d677a

## Review state and budgets

Material author remains gpt-6.2-astra. The requested reviewer remains the fixed different-model gpt-6.2-sol session /root/bitcoin_round7/flats_design_review and has authored no adopted implementation. This is design correction/impact round2 of3; round1 produced D2. Invocation retries remain1 of2 used. The new impact review is pending. Implementation and final testing are not accepted.
