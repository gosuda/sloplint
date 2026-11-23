# Practices adopted from JustGoodPractices

sloplint adopts two checks from
[JustGoodPractices at `7b80f75`](https://github.com/gosuda/JustGoodPractices/tree/7b80f75de6c626aa4f1466269468935a9b0ef444):
unnecessary `else` blocks and wall-clock sleeps in tests. These are independently
written implementations using sloplint's language adapters and structural facts.
The source project is [MIT licensed](https://github.com/gosuda/JustGoodPractices/blob/7b80f75de6c626aa4f1466269468935a9b0ef444/LICENSE),
copyright 2026 GoSuda. No source code or skill text is copied.

The practices share rule IDs, configuration, roles, and output across languages.
Each language adapter supplies only the facts its syntax can establish.

| Rule | Supported languages |
|---|---|
| `slop/redundant-else` | C, C++, C#, Go, Java, JavaScript/JSX, Python, Rust, TypeScript/TSX |
| `agent/no-sleep-in-tests` | C#, Go, Java, Python, Rust |

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
It detects a nonempty alternative after a branch that exits unconditionally.

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

Java constructor guards use the same scope checks as method guards.

The adapter requires a direct terminal statement at the end of the preceding
branch: `return`, `break`, or `continue`, plus `throw` in C++, Java, C#,
JavaScript/TypeScript and `raise` in Python. It does not guess that calls named `panic`, `fatal`, or
`exit` terminate. A return inside a nested closure or a conditional branch
does not prove that the outer branch exits.

The check skips else-if chains, empty alternatives, and declarations or labels
whose scope would change. Nested blocks must keep their scope when applying
the suggested change. Additional restrictions follow each language's semantics:

| Language | Cases left unchanged |
|---|---|
| Go | Initializer scopes, including variables introduced by `if value := read(); ...` |
| JavaScript/TypeScript | Conditionals directly under unbraced control statements; branch-local declarations; statement boundaries that rely on automatic semicolon insertion |
| C/C++ | Preprocessor regions, condition declarations and initializers, and `if constexpr` |
| Java/C# | Pattern bindings, declaration expressions, and branches outside a containing block |
| Rust | Value-producing or tail `if` expressions, pattern bindings, non-identifier conditions, outer attributes, and alternatives with direct declarations, macros, or implicit tail values |

Rust coverage deliberately requires a discarded statement, a plain identifier
condition, and an alternative made of explicitly terminated expression
statements. This avoids changing result types, binding visibility, or drop
scope. A syntactically terminating branch alone is insufficient in a language
where an `if` can produce a value.

## Wait for completion in tests

`agent/no-sleep-in-tests` follows the deterministic-synchronization principle
from the [source testing analyzer](https://github.com/gosuda/JustGoodPractices/blob/7b80f75de6c626aa4f1466269468935a9b0ef444/internal/analyzer/rules_testing.go).
It reports identified standard-library sleep calls in effective test or
test-support roles, including inline test regions.

| Language | Recognized API and aliases |
|---|---|
| Go | `time.Sleep` through an explicit package import or alias |
| Python | `time.sleep`, module aliases, and module-scope `from time import sleep` aliases |
| Rust | `std::thread::sleep`, leading `::std` paths, and explicit source-file-level `use` aliases or grouped imports |
| Java | `java.lang.Thread.sleep`, unambiguous `Thread.sleep`, and static imports from that exact type |
| C# | `System.Threading.Thread.Sleep`, `global::` paths, file-scope namespace imports, direct type aliases, and static imports from that exact type |

```go
go rebuildIndex()
time.Sleep(100 * time.Millisecond)
checkIndex(t)
```

A duration cannot establish that the rebuild finished. Wait for a completion
signal or an observable condition. A bounded timeout can explain a failure;
it should not be the evidence that the operation succeeded. Use virtual time
when the behavior under test is a timer.

The adapters omit ambiguous bindings instead of treating arbitrary methods named
`sleep` as standard-library calls. A shadowing declaration or rebinding anywhere
in a file conservatively disables recognition of that imported name, including
when the shadow is in a separate scope. This can miss real sleeps, but avoids
accusing a fake clock or unrelated object of using wall-clock time.

Go sleeps inside recognized inline `testing/synctest.Test` or `Run` callbacks
are excluded. A dot-imported or shadowed synctest binding conservatively disables
the check for the file. The adapter
does not follow calls into separate helper functions to discover their clock
context. Python assignments to `time.sleep`, simple literal patch targets, and
common `setattr` or `patch.object` shapes also disable recognition for the file.
Qualified `builtins.setattr` and `delattr` require an explicit, unshadowed
module import or alias. These built-ins take object targets; only recognized
patch helpers accept a dotted string such as `"time.sleep"` as a module target.
The mutation veto also covers a local object that shadows an imported module's
name. Without object identity, the adapter cannot prove that the target is a
separate fake; this can hide real sleeps through independently imported aliases.

Java static wildcard imports remain a source of ambiguity even beside an
explicit `Thread.sleep` import. An imported `sleep(int)` can be selected over
`Thread.sleep(long)`: single static imports shadow on-demand methods only for
matching signatures, as specified in
[JLS 6.4.1](https://docs.oracle.com/javase/specs/jls/se26/html/jls-6.html#jls-6.4.1).
The adapter keeps that veto instead of assuming the members of an external
class. A fully qualified `java.lang.Thread.sleep` remains recognizable.

Delay exemptions follow the runtime. Go and Python omit literal nonpositive
delays. Rust omits identity-proven standard `Duration::ZERO` and zero-valued
standard duration constructors. Java and C# omit zero numeric delays; Java's
two-argument overload is checked for both milliseconds and nanoseconds. A
nonzero nanosecond argument still waits. Literal Java delays that immediately
throw, such as negative milliseconds or nanoseconds outside `0..999999`, are
omitted. C#'s `Sleep(-1)` is an infinite wait and is reported; numeric literals
below `-1` are omitted because they throw. These are bounded literal checks,
not general constant evaluation. Zero-delay exemptions are intentional even
on runtimes where a zero sleep can yield the thread.

Production code, benchmarks, examples, timeout APIs, and asynchronous sleep APIs
are outside this rule. Local-only imports, ambiguous wildcard imports, indirect
aliases, cross-file type resolution, and arbitrary runtime monkey-patching are
not resolved. Rust root aliases are not assumed to be visible in nested
modules. Canonical `::std` calls and zero durations survive nested module globs,
which cannot redirect the absolute root. Relative canonical paths remain
recognizable outside the scopes of nested globs. A glob in an enclosing block
or the current module keeps those relative paths ambiguous; imports from an
outer module do not enter a child module under Rust's
[item scope rules](https://doc.rust-lang.org/reference/names/scopes.html#item-scopes).
File-scope globs and explicit `std` shadowing still veto recognition throughout
the file. Leading `::` on a local import alias does not prove standard-library
identity because absolute path resolution differs by
[Rust edition](https://doc.rust-lang.org/reference/paths.html#path-qualifiers).
C# namespace aliases and nested using directives remain unclassified.
The rule does not prove
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
resulting facts and apply code roles. Facts remain independent of the selected
profile; import and binding resolution is cached per file so repeated calls
reuse it. Each supported language has a positive
fixture and a near-miss fixture; fact tests exercise scope, terminal statements,
aliases, and runtime exceptions. This preserves the distinction between a
useful engineering principle and a predicate that syntax can support.
