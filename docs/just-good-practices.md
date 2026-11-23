# Practices adopted from JustGoodPractices

sloplint adopts two checks from
[JustGoodPractices at `7b80f75`](https://github.com/gosuda/JustGoodPractices/tree/7b80f75de6c626aa4f1466269468935a9b0ef444):
unnecessary `else` blocks and wall-clock sleeps in tests. These are independently
written implementations using sloplint's language adapters and structural facts.
The source project is [MIT licensed](https://github.com/gosuda/JustGoodPractices/blob/7b80f75de6c626aa4f1466269468935a9b0ef444/LICENSE),
copyright 2026 GoSuda. No source code or skill text is copied.

Both rules start **off at minimal, basic, and standard**, and **info with low
confidence at strict**. Their precision has not yet been measured on the
project's real-code corpus. Positive fixtures prove that a predicate works;
they do not establish the proportion of useful findings in existing projects.
The existing 80 percent precision requirement still applies before either
rule can enter the standard profile.

To inspect only these checks:

```bash
sloplint check --strictness strict \
  --rule slop/redundant-else --rule agent/no-sleep-in-tests
```

The default `major` failure threshold will not fail on these advisory findings.
Add `--fail-on info` to gate on them, or set a rule's `level` explicitly in
`sloplint.toml` after reviewing its results on your code.

## Flatten a branch only when its exit is known

`slop/redundant-else` implements the flat-control-flow principle from the
[source control-flow analyzer](https://github.com/gosuda/JustGoodPractices/blob/7b80f75de6c626aa4f1466269468935a9b0ef444/internal/analyzer/rules_controlflow.go).
It supports Go, Python, JavaScript, TypeScript, and TSX.

```go
if failed {
    return err
} else {
    recordSuccess()
    return nil
}
```

The alternative can follow the guard:

```go
if failed {
    return err
}
recordSuccess()
return nil
```

The adapter requires a direct terminal statement at the end of the preceding
branch: `return`, `break`, or `continue`, plus `throw` in JavaScript/TypeScript
and `raise` in Python. It does not guess that calls named `panic`, `fatal`, or
`exit` terminate. A return inside a nested closure or a conditional branch
does not prove that the outer branch exits.

The check skips else-if chains, empty alternatives, Go initializer scopes,
and Go/JavaScript/TypeScript alternatives whose direct declarations or labels
need their existing scope. JavaScript/TypeScript conditionals under unbraced
control statements are also skipped. Nested blocks must keep their scope when
applying the suggested change. Expression-valued conditionals in other
languages remain outside this rule's scope.

## Wait for completion in tests

`agent/no-sleep-in-tests` follows the deterministic-synchronization principle
from the [source testing analyzer](https://github.com/gosuda/JustGoodPractices/blob/7b80f75de6c626aa4f1466269468935a9b0ef444/internal/analyzer/rules_testing.go).
It reports identified Go `time.Sleep` and Python `time.sleep` calls in effective
test or test-support roles, including inline test regions.

```go
go rebuildIndex()
time.Sleep(100 * time.Millisecond)
checkIndex(t)
```

A duration cannot establish that the rebuild finished. Wait for a completion
signal or an observable condition. A bounded timeout can explain a failure;
it should not be the evidence that the operation succeeded. Use virtual time
when the behavior under test is a timer.

The adapters recognize module-scope imports and their aliases. Python also
recognizes `from time import sleep` and an explicit alias for that function.
They omit ambiguous bindings instead of treating arbitrary methods named
`sleep` as standard-library calls. A shadowing declaration or rebinding anywhere
in a file conservatively disables recognition of that imported name, including
when the shadow is in a separate scope. This can miss real sleeps, but avoids
accusing a fake clock or unrelated object of using wall-clock time.

Go sleeps inside recognized inline `testing/synctest.Test` or `Run` callbacks
are excluded, as are literal nonpositive delays. A dot-imported or shadowed
synctest binding conservatively disables the check for the file. The adapter
does not follow calls into separate helper functions to discover their clock
context. Python assignments to `time.sleep`, simple literal patch targets, and
common `setattr` or `patch.object` shapes also disable recognition for the file.

Production code, benchmarks, examples, timeout APIs, and asynchronous sleep APIs
are outside this rule. Local-only imports, wildcard imports, indirect aliases,
and arbitrary runtime monkey-patching are not resolved. The rule does not prove
that a particular sleep is used for synchronization: tests of real clocks or
scheduling may need a suppression with a reason.

```go
// sloplint-allow(agent/no-sleep-in-tests): this integration test measures the operating system clock
time.Sleep(delay)
```

## Reuse existing coverage

The source pack contains useful review guidance that already has a counterpart
in sloplint. These practices do not need another overlapping rule.

| Practice | Existing coverage |
|---|---|
| Reduce nesting | `complexity/cognitive-depth`, `complexity/nesting-depth` |
| Explain intent instead of restating syntax | `slop/echo-comment` |
| Use precise names | `grep/generic-name` |
| Avoid ambiguous primitive arguments | `complexity/boolean-params`, `agent/stringly-api` |
| Preserve error information | `slop/zombie-catch`, `agent/typed-error-boundary` |
| Remove trivial indirection | `slop/passthrough-wrapper`, `slop/single-impl-abstraction` |
| Explain assertion failures | `agent/informative-assertions` |
| Control shared mutable state | `agent/ambient-global-state` |

These mappings describe the overlap; each existing rule retains its documented
predicate, exceptions, confidence, and profile settings.

## Keep semantic decisions in review

A repeated map lookup is a useful candidate only when the map and key are
stable and no intervening mutation changes the value. sloplint does not yet
have the facts to establish those conditions across languages, so it does not
import the source's broad rendered-expression matcher.

Negative boolean names, mock query call counts, constructor names, collaborator
injection, and whole-object assertions need knowledge of a domain or contract.
Their spelling alone does not establish a defect. Go-version-specific library
replacements also need toolchain and evaluation-order checks. Those practices
remain review guidance rather than new syntax rules.

## Maintain the boundary

Language adapters identify syntax and call identity. Core rules consume the
resulting facts and apply code roles. Each supported language has a positive
fixture and a near-miss fixture; fact tests exercise scope, terminal statements,
aliases, and runtime exceptions. This preserves the distinction between a
useful engineering principle and a predicate that syntax can support.
