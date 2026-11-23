use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn sleep_lines(lang: Lang, source: &str) -> Vec<u32> {
    let path = match lang {
        Lang::Java => "Clock.java",
        Lang::CSharp => "Clock.cs",
        _ => panic!("managed sleep tests require Java or C#"),
    };
    run_clean(lang, path, source)
        .facts
        .blocking_sleeps
        .into_iter()
        .map(|pos| pos.line)
        .collect()
}

#[test]
fn java_thread_sleep_qualifiers_and_nanos_are_classified() {
    let source = r#"class Clock {
    void waitForIndex(long millis, int nanos) throws InterruptedException {
        Thread.sleep(1);
        java . lang . Thread.sleep(millis);
        Thread.sleep(0, nanos);
        Thread.sleep(0, 1);
        Thread.sleep(0, 0);
        Thread.sleep(/* yield */ (0L));
    }
}
"#;
    assert_eq!(sleep_lines(Lang::Java, source), [3, 4, 5, 6]);
}

#[test]
fn java_explicit_and_static_imports_are_resolved() {
    let source = r#"import java.lang.Thread;
import static java.lang.Thread.sleep;
import application.*;
class Clock {
    void waitForIndex() throws InterruptedException {
        Thread.sleep(1);
        sleep(0, 1);
        sleep(0, 0);
    }
}
"#;
    assert_eq!(sleep_lines(Lang::Java, source), [6, 7]);
    let wildcard =
        "import static java.lang.Thread.*; class Clock { void waitForIndex() { sleep(1); } }";
    assert_eq!(sleep_lines(Lang::Java, wildcard), [1]);
}

#[test]
fn java_zero_integer_forms_are_nonblocking() {
    for value in [
        "0",
        "(0)",
        "+0",
        "-0",
        "0L",
        "0x0L",
        "0b0",
        "00_0",
        "/* a */ (/* b */ 0)",
    ] {
        let source = format!("class Clock {{ void waitForIndex() {{ Thread.sleep({value}); }} }}");
        assert!(sleep_lines(Lang::Java, &source).is_empty(), "{source}");
    }
    let zero_pair =
        "class Clock { void waitForIndex() { Thread.sleep(/* millis */ 0, /* nanos */ 0); } }";
    assert!(sleep_lines(Lang::Java, zero_pair).is_empty());
}

#[test]
fn java_invalid_literal_arguments_throw_before_blocking() {
    for arguments in [
        "-1",
        "-1L",
        "(-0x1)",
        "0xffff_ffff",
        "0xffff_ffff_ffff_ffffL",
        "-(-2147483648)",
        "-(-9223372036854775808L)",
        "0, -1",
        "0, 1_000_000",
        "millis, 0xf4240",
        "0, (-2147483648)",
    ] {
        let source = format!(
            "class Clock {{ void waitForIndex(long millis) {{ Thread.sleep({arguments}); }} }}"
        );
        assert!(sleep_lines(Lang::Java, &source).is_empty(), "{source}");
    }
    let maximum_nanos = "class Clock { void waitForIndex() { Thread.sleep(0, 999_999); } }";
    assert_eq!(sleep_lines(Lang::Java, maximum_nanos), [1]);
}

#[test]
fn java_visible_bindings_veto_imported_thread() {
    for body in [
        "class Clock { void waitForIndex(Sleeper Thread) { Thread.sleep(1); } }",
        "class Clock { void waitForIndex() { Sleeper Thread = fake(); Thread.sleep(1); } }",
        "class Clock { Sleeper Thread; void waitForIndex() { Thread.sleep(1); } }",
        "class Clock<Thread> { void waitForIndex() { Thread.sleep(1); } }",
        "class Thread { static void sleep(int millis) {} } class Clock { void waitForIndex() { Thread.sleep(1); } }",
        "class Clock { void waitForIndex() { for (Sleeper Thread : clocks) { Thread.sleep(1); } } }",
        "class Clock { void waitForIndex() { try { work(); } catch (Failure Thread) { Thread.sleep(1); } } }",
        "class Clock { void waitForIndex() { var action = (Thread) -> Thread.sleep(1); } }",
        "class Clock { void waitForIndex(Object value) { if (value instanceof Sleeper Thread) { Thread.sleep(1); } } }",
    ] {
        let source = format!("import java.lang.Thread;\n{body}\n");
        assert!(sleep_lines(Lang::Java, &source).is_empty(), "{source}");
    }
}

#[test]
fn java_qualified_calls_survive_a_shadowed_short_name() {
    let source = r#"class Clock {
    void waitForIndex(Sleeper Thread) {
        Thread.sleep(1);
        java.lang.Thread.sleep(1);
    }
}
"#;
    assert_eq!(sleep_lines(Lang::Java, source), [4]);
    let shadowed_package =
        "class Clock { void waitForIndex(Fake java) { java.lang.Thread.sleep(1); } }";
    assert!(sleep_lines(Lang::Java, shadowed_package).is_empty());
}

#[test]
fn java_import_and_inheritance_ambiguity_is_not_guessed() {
    for source in [
        "import application.Thread; class Clock { void waitForIndex() { Thread.sleep(1); } }",
        "import application.*; class Clock { void waitForIndex() { Thread.sleep(1); } }",
        "import static application.Clocks.*; class Clock { void waitForIndex() { Thread.sleep(1); } }",
        "class Clock extends Base { void waitForIndex() { Thread.sleep(1); } }",
        "import static java.lang.Thread.sleep; class Clock { void sleep(int millis) {} void waitForIndex() { sleep(1); } }",
        "import static java.lang.Thread.sleep; class Clock extends Base { void waitForIndex() { sleep(1); } }",
        "class Clock { void waitForIndex() { clock.sleep(1); sleep(1); } }",
    ] {
        assert!(sleep_lines(Lang::Java, source).is_empty(), "{source}");
    }
}

#[test]
fn csharp_thread_imports_aliases_and_static_calls_are_classified() {
    let source = r#"using System.Threading;
using Worker = global::System.Threading.Thread;
using static System.Threading.Thread;
class Clock {
    void WaitForIndex(int millis) {
        Thread.Sleep(1);
        Worker.Sleep(millis);
        System . Threading . Thread.Sleep(1);
        global::System.Threading.Thread.Sleep(1);
        Sleep(1);
        @Worker.@Sleep(millisecondsTimeout: 1);
    }
}
"#;
    assert_eq!(sleep_lines(Lang::CSharp, source), [6, 7, 8, 9, 10, 11]);
}

#[test]
fn csharp_infinite_sleeps_are_not_mistaken_for_zero_delays() {
    let source = r#"using System.Threading;
class Clock {
    void WaitForIndex() {
        Thread.Sleep(-1);
        Thread.Sleep(Timeout.Infinite);
        Thread.Sleep(Timeout.InfiniteTimeSpan);
        Thread.Sleep(0);
        Thread.Sleep(-0);
    }
}
"#;
    assert_eq!(sleep_lines(Lang::CSharp, source), [4, 5, 6]);
}

#[test]
fn csharp_zero_integer_forms_are_nonblocking() {
    for value in [
        "0",
        "(0)",
        "+0",
        "-0",
        "0x0",
        "0b0",
        "0_0",
        "millisecondsTimeout: /* yield */ (0)",
    ] {
        let source = format!(
            "using System.Threading; class Clock {{ void WaitForIndex() {{ Thread.Sleep({value}); }} }}"
        );
        assert!(sleep_lines(Lang::CSharp, &source).is_empty(), "{source}");
    }
}

#[test]
fn csharp_invalid_negative_literals_throw_before_blocking() {
    for value in [
        "-2",
        "(-2)",
        "-0x2",
        "-0b10",
        "-2147483648",
        "millisecondsTimeout: -2",
    ] {
        let source = format!(
            "using System.Threading; class Clock {{ void WaitForIndex() {{ Thread.Sleep({value}); }} }}"
        );
        assert!(sleep_lines(Lang::CSharp, &source).is_empty(), "{source}");
    }
    let infinite_hex =
        "using System.Threading; class Clock { void WaitForIndex() { Thread.Sleep(-0x1); } }";
    assert_eq!(sleep_lines(Lang::CSharp, infinite_hex), [1]);
}

#[test]
fn csharp_visible_bindings_veto_imported_thread() {
    for body in [
        "class Clock { void WaitForIndex(Sleeper Thread) { Thread.Sleep(1); } }",
        "class Clock { void WaitForIndex() { var Thread = fake; Thread.Sleep(1); } }",
        "class Clock { Sleeper @Thread; void WaitForIndex() { Thread.Sleep(1); } }",
        "class Clock<Thread> { void WaitForIndex() { Thread.Sleep(1); } }",
        "class Thread { public static void Sleep(int millis) {} } class Clock { void WaitForIndex() { Thread.Sleep(1); } }",
        "class Clock { void WaitForIndex() { foreach (var Thread in clocks) { Thread.Sleep(1); } } }",
        "class Clock { void WaitForIndex() { try { Work(); } catch (Failure Thread) { Thread.Sleep(1); } } }",
        "class Clock { void WaitForIndex() { var action = Thread => Thread.Sleep(1); } }",
        "class Clock { void WaitForIndex(object value) { if (value is Sleeper Thread) { Thread.Sleep(1); } } }",
        "class Clock { void WaitForIndex() { var (Thread, other) = pair; Thread.Sleep(1); } }",
        r"class Clock { void WaitForIndex(Sleeper \u0054hread) { Thread.Sleep(1); } }",
    ] {
        let source = format!("using System.Threading;\n{body}\n");
        assert!(sleep_lines(Lang::CSharp, &source).is_empty(), "{source}");
    }
}

#[test]
fn csharp_aliases_and_static_imports_respect_visible_shadowing() {
    for source in [
        "using Worker = System.Threading.Thread; class Clock { void WaitForIndex(Sleeper Worker) { Worker.Sleep(1); } }",
        "using static System.Threading.Thread; class Clock { void Sleep(int millis) {} void WaitForIndex() { Sleep(1); } }",
        "using static System.Threading.Thread; class Clock { void WaitForIndex() { void Sleep(int millis) {} Sleep(1); } }",
        "using static System.Threading.Thread; using static Application.Clock; class Clock { void WaitForIndex() { Sleep(1); } }",
        "using System.Threading; class Clock : Base { void WaitForIndex() { Thread.Sleep(1); } }",
        "using static System.Threading.Thread; class Clock : Base { void WaitForIndex() { Sleep(1); } }",
    ] {
        assert!(sleep_lines(Lang::CSharp, source).is_empty(), "{source}");
    }
}

#[test]
fn csharp_global_qualification_bypasses_a_local_system_binding() {
    let source = r#"class Clock {
    void WaitForIndex(Fake System) {
        System.Threading.Thread.Sleep(1);
        global::System.Threading.Thread.Sleep(1);
    }
}
"#;
    assert_eq!(sleep_lines(Lang::CSharp, source), [4]);
}

#[test]
fn csharp_lookalikes_async_delays_and_out_of_scope_usings_remain_unclassified() {
    for source in [
        "using Thread = Application.Thread; class Clock { void WaitForIndex() { Thread.Sleep(1); } }",
        "using System.Threading.Tasks; class Clock { void WaitForIndex() { Task.Delay(1); clock.Sleep(1); } }",
        "class Clock { void WaitForIndex() { Thread.Sleep(1); Sleep(1); } }",
        "namespace A { using System.Threading; class Clock { void WaitForIndex() { Thread.Sleep(1); } } } namespace B { class Clock { void WaitForIndex() { Thread.Sleep(1); } } }",
        "using Threads = System.Threading; class Clock { void WaitForIndex() { Threads.Thread.Sleep(1); } }",
    ] {
        assert!(sleep_lines(Lang::CSharp, source).is_empty(), "{source}");
    }
}
