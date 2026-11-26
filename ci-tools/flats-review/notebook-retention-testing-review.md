## Final testing review receipt

**ACCEPT.** Execution state **completed**. Candidate verdict **accepted** for the retained-memory correction in Flats PR 266. There are no blocking findings or outstanding resolution conditions within this testing scope.

**Snapshot.** Candidate `066c32dcf2e0d2a9d9ea0d41e7aacf68524c1518`, native-tested tree `86fb795695e992b5835bdffe5f1fbaade7427b36`, parent and live PR head `86c7849c2f608fd751e9a478c2574c49e817f44d`, actual `main` `c7cd9d07c9e841f4430772f1193b2c09e757411f`, merge base `81235c28ba7d51ea12aad4740da198d54b8a59b5`. Before and after review, I recomputed and matched all five source SHA-256 hashes against the testing manifest and verified comparison membership. The correction changes three of those files. I independently matched the native job log SHA-256, proof recipe and input hashes, and the design and implementation receipt hashes. [Candidate commit](https://github.com/gosuda/flats/commit/066c32dcf2e0d2a9d9ea0d41e7aacf68524c1518)

| Causal control | Original exit | Candidate exit |
|---|---:|---:|
| 2 MiB retained only on the third read | 0 | 1 |
| 400 KiB retained on each read | 0 | 1 |
| 2 MiB retained only on the first read | 1 | 1 |
| 2 MiB retained only on the middle read | 1 | 1 |
| Valid response with a test-owned 2 MiB backing allocation | 1 | 0 |

The log shows all three actual reads for each control. Intentional failures reached the post-GC 1 MiB retention assertion. The response control preserved the response bytes and introduced no global reference to the oversized backing allocation. The candidate samples settled heap after the sampler and the read helper have returned, then reloads the actual response selected by the lowest sampled peak for continuation checks.

**Native execution.** The same two-CPU Depot Ubuntu 24.04 job paired the original and candidate controls under Go 1.27.4. Both complete `option_schema` runs exited zero and included both live sizes and `bounded_growth`. Candidate notebook vet, the full notebook package, both complementary notebook race shards, documentation audit and verification, formatting, and whitespace checks also exited zero. The job verified source restoration and printed the exact candidate tree after the required gates. The large-memory opt-in lane was outside this correction’s default live-read scope. [Native job and artifact metadata](https://github.com/gosuda/sloplint/actions/runs/45861924362/job/140054864954)

**Evidence reuse.** I accept reuse of the completed integrated execution because it covers this candidate’s exact tree and required checks, with no intervening source or test change. This review performed no new native run. Settled heap is a process-wide Go metric subject to unrelated heap movement. I matched the artifact digest from GitHub metadata against the upload log. ZIP bytes were outside the independent checksum verification.

**Review identity and budget.** Fresh session `/root/bitcoin_round7/flats_retention_testing_review`, fixed non-aliased `gpt-6.2-sol` collaboration binding. The material design, implementation, and testing-result author is `/root/bitcoin_round7`, `gpt-6.2-astra`. This receipt consumes testing invocation **2/3**, with **0/2** corrections used. Design and implementation each remain accepted at **3/3** invocations and **1/2** corrections. The verdict covers this immutable candidate. Publication and any later source snapshot require their own current-state checks.
