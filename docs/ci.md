# Continuous integration

GitHub Actions owns all workflow events and every job uses Depot hardware.
The duplicate `.depot/workflows/ci.yml` pipeline has been removed.

## Pull requests and merge queues

The `lint` job runs formatting, grammar synchronization, release-script
self-tests, and the pinned cargo-deny license and advisory check with one
checkout. Two Linux jobs run complete Clippy and test suites, release
builds, and filesystem-capability checks on x86-64 and ARM64. The x86-64
release binary also lints the repository. Main pushes do not repeat the
pull request or merge queue pipeline.

## Extended platform coverage

The native matrix runs each Monday at 03:23 UTC, on manual dispatch, and
when the release workflow calls CI for a tag. Each target shares its
Clippy and platform tests in one job. macOS uses `depot-macos-latest`.
Windows x64 uses `depot-windows-2025-4`.

macOS x64 is cross-compiled on Apple silicon and its recursion tests run
through Rosetta. This is translated execution, not physical Intel host
coverage. The job fails if Rosetta is unavailable. Windows ARM64 receives
a cross-target Clippy check from the x64 Windows job. Depot does not offer
a Windows ARM64 host, so this matrix no longer certifies ARM64 Windows
runtime behavior.

## Release artifacts and provenance

The release workflow retains archives and SBOMs for all six target triples.
Windows ARM64 is cross-compiled and packaged without executing or
self-linting its binary. The other five artifacts retain version smoke
tests and binary self-lint. macOS x64 executes through Rosetta.

Attestation creation and publishing run on Depot Linux. Verification still
requires the correct artifact signature, repository, signer workflow,
source tag, and source commit. Invalid or missing attestations still block
publishing. The GitHub-hosted-only `--deny-self-hosted-runners` restriction
has been removed to admit the requested Depot runner provenance. This
changes the accepted execution environment, not the artifact identity
requirements. Release builds do not restore shared CI caches.

Runner inventory: <https://depot.dev/docs/github-actions/runner-types>.
Attestation policy: <https://cli.github.com/manual/gh_attestation_verify>.
