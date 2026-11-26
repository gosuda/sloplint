# Notebook retained-heap test correction

## Scope and state

This design addresses the two remaining review findings on Flats PR 266. The source snapshot is `86c7849c2f608fd751e9a478c2574c49e817f44d`; actual main is `c7cd9d07c9e841f4430772f1193b2c09e757411f`. No implementation has been applied for this follow-up.

The original request authorizes resolving PR feedback and publishing additive commits. It does not authorize merging. This work changes test accounting and its explanation. The notebook implementation, memory limits, fixture sizes, CI resources and test selection stay within their existing contracts.

The complete current PR changes five paths: `docs/gates.md`, `markdown_read_memory_test.go`, `reader_entry_test.go`, `markdown_read_scaling_test.go`, and `markdown_read_capture_release_test.go` under `internal/notebook`. The proposed follow-up changes only the first three. The review covers the current five-path result and the proposed delta.

## Defects and required behavior

The collected-memory loop makes three reads per payload size. Its first baseline precedes read one, but its last baseline precedes read three. Retention created by the final read therefore escapes the per-size check. The test also keeps one response body in Go memory for later continuation checks. That body is absent from the first baseline and present in later ones, so the shift includes test-owned storage.

The per-size gate must compare settled Go heap immediately before the first measured read with settled heap after all three measured reads. It must include retained allocations created by the first, middle and final read. It must exclude the selected response kept only for subsequent assertions. The positive difference must remain at most 1 MiB.

The separate peak-growth comparison keeps its three-sample minimum and 1 MiB cross-size budget. The existing whole-read tests keep the 4 MiB ceiling. Every output, reference, warning, snapshot and continuation assertion still runs on an actual response from a measured read. Spill files and quota remain governed by the existing release test.

## Chosen accounting

Keep the selected response in a temporary file during the measurement sequence. Create the path before the first baseline. When a sample becomes the selected minimum, write its response bytes to that file and drop the in-memory response reference before the next measurement. Do not keep a response slice in the loop's outer scope.

After read three, settle the heap with two collections, matching the existing baseline's treatment of pool victims. Read HeapAlloc only after the sampler has stopped and the measured calls and response-write scope have returned. Compare that value with the first measured baseline. Reload the selected response only after taking the final heap sample, then run the existing continuation assertions.

This preserves the response from the measured read and removes its storage through reachability. It avoids subtracting slice capacity from a whole-heap measurement, which would depend on backing-allocation ownership and size. It also avoids an extra application read that would replace the selected measured response.

The response is bounded by the existing cursor-page contract. Temporary-file errors fail the test with context. The file is owned by the test's temporary directory and removed by its cleanup. The I/O occurs between measured reads and does not alter the capture sampler. No new production hook or persistent fixture is introduced.

Comments must describe sampled Go heap rather than claim that collection eliminates all measurement noise. Settling does not measure allocations outside the Go runtime, and periodic sampling can miss allocations released between samples. Those limitations remain explicit.

## Verification before implementation acceptance

Freeze the fault scenarios before correcting the test. In a disposable native checkout, use the same source, Go version and fixed CPU count for the original and corrected test.

1. Retain more than 1 MiB only on the final measured read. The original per-size retention gate must miss this fault; the corrected gate must fail in the named retention test with an actual nonzero test exit.
2. Retain 400 KiB on every measured read. The corrected gate must count all three reads and reject the cumulative growth. Preserve the original observation, whether it passes or fails on that host.
3. Retain more than 1 MiB on the first measured read and on the middle measured read in separate processes. The corrected gate must reject both.
4. Return the same response bytes in a deliberately oversized backing allocation owned only by the test. After its bytes are saved and its slice released, the corrected retention gate must pass. This control must not disable the real-retention faults or bypass response assertions.
5. Run the unchanged clean option-schema parent with all sizes and its cross-size comparison. Preserve the logs for every sample, baseline, final heap and test duration. A failure is a defect to diagnose, not a reason to raise a budget.

All fault changes remain outside the candidate. Prove exact restoration before the clean gates. Run gofmt, affected-package go vet, the complete notebook tests, both notebook race shards, the existing capture-release test, and the documentation checks required by the changed explanation. Preserve actual command exits and the exact candidate tree. Keep CI resources and timeouts unchanged.

Before implementation review, run the repository's code and prose cleanup steps. The reviewer must inspect the complete five-path PR scope, the follow-up diff, source-bound fault results and remaining limitations.

## Final testing and publication

After implementation acceptance, verify the exact approved tree with the integrated notebook and documentation gates. A separate eligible reviewer assesses the testing result. Any test-only correction requires an accepted implementation impact assessment. Publish only after the stages accept the final snapshot and fresh reads confirm the maintained head and actual main.

Reply to both outstanding threads with the source commit and native evidence. Resolve them only after the source correction and required verification are published. Do not treat bot resolution as proof of correctness.

## Review and budgets

The material author of this design is the current GPT-6 Astra session. An eligible reviewer must run in a fresh session using a fixed, different model ID. A requested fixed `gpt-6.2-sol` session satisfies that identity distinction.

The PR's published report records design and implementation at two of three review invocations and one of two corrections; testing at one of three invocations and zero of two corrections. Preserve those limits and usage. This follow-up does not reset them. Exact prior receipts are not linked in that report, so no prior approval is claimed for the current head. The next design review is invocation three of three and must assess this complete current design. Persistent review failure or an exhausted stage budget produces a blocker report.

The reported human analysis in issue comment 7195105179 is useful evidence to reproduce, not completed native verification for this executor. Its proposed capacity subtraction is not adopted. The source snapshots, this design, exact reviewer verdict and native evidence are recorded outside the Flats source tree.
