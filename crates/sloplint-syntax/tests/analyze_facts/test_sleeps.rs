use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn sleep_lines(lang: Lang, source: &str) -> Vec<u32> {
    let path = if lang == Lang::Go {
        "clock.go"
    } else {
        "clock.py"
    };
    run_clean(lang, path, source)
        .facts
        .blocking_sleeps
        .into_iter()
        .map(|pos| pos.line)
        .collect()
}

#[test]
fn go_import_aliases_are_classified_outside_test_paths() {
    let source = r#"package p
import clock "time"
func pause(delay clock.Duration) {
    clock.Sleep(1)
    clock . Sleep(delay)
}
"#;
    assert_eq!(sleep_lines(Lang::Go, source), [4, 5]);
}

#[test]
fn python_module_and_function_aliases_are_classified_outside_test_paths() {
    let source = r#"import time as clock
from time import sleep as pause
def wait_for_index(delay):
    clock.sleep(0.1)
    pause(delay)
"#;
    assert_eq!(sleep_lines(Lang::Python, source), [4, 5]);
}

#[test]
fn python_unaliased_imports_are_classified() {
    let source = "import time\nfrom time import sleep\ntime.sleep(1)\nsleep(2)\n";
    assert_eq!(sleep_lines(Lang::Python, source), [3, 4]);
}

#[test]
fn lookalike_modules_and_unimported_names_are_not_classified() {
    let go = r#"package p
import time "example.test/time"
func wait() { time.Sleep(1) }
"#;
    assert!(sleep_lines(Lang::Go, go).is_empty());
    for python in [
        "import clocks as time\ntime.sleep(1)\n",
        "from clocks import sleep\nsleep(1)\n",
        "from .time import sleep\nsleep(1)\n",
        "import asyncio\nasyncio.sleep(1)\n",
        "def wait(clock):\n    clock.sleep(1)\n",
        "sleep(1)\ntime.sleep(1)\n",
    ] {
        assert!(sleep_lines(Lang::Python, python).is_empty(), "{python}");
    }
}

#[test]
fn go_zero_and_negative_literals_do_not_sleep() {
    let source = r#"package p
import "time"
func wait(delay time.Duration) {
    time.Sleep(0)
    time.Sleep((0))
    time.Sleep(0.0)
    time.Sleep(+0)
    time.Sleep(-1)
    time.Sleep(0x0)
    time.Sleep(0b0)
    time.Sleep(0o0)
    time.Sleep(0e9)
    time.Sleep(0x0p9)
    time.Sleep(1)
    time.Sleep(delay)
}
"#;
    assert_eq!(sleep_lines(Lang::Go, source), [14, 15]);
}

#[test]
fn python_zero_and_negative_literals_do_not_block() {
    let source = r#"import time
def wait(delay):
    time.sleep(0)
    time.sleep((0.0))
    time.sleep(-0.0)
    time.sleep(+0)
    time.sleep(-1)
    time.sleep(0x0)
    time.sleep(0b0)
    time.sleep(0o0)
    time.sleep(0e9)
    time.sleep(False)
    time.sleep(0.25)
    time.sleep(delay)
"#;
    assert_eq!(sleep_lines(Lang::Python, source), [13, 14]);
}

#[test]
fn go_synctest_callbacks_and_nested_goroutines_use_virtual_time() {
    let source = r#"package p
import "time"
import "testing"
import bubble "testing/synctest"
func TestClock(t *testing.T) {
    time.Sleep(1)
    bubble.Test(t, func(t *testing.T) {
        time.Sleep(2)
        go func() { time.Sleep(3) }()
    })
    bubble.Run(func() { time.Sleep(4) })
    time.Sleep(5)
}
"#;
    assert_eq!(sleep_lines(Lang::Go, source), [6, 12]);
}

#[test]
fn a_lookalike_synctest_package_does_not_hide_real_sleeps() {
    let source = r#"package p
import "time"
import synctest "example.test/synctest"
func wait() {
    synctest.Run(func() { time.Sleep(1) })
}
"#;
    assert_eq!(sleep_lines(Lang::Go, source), [5]);
}

#[test]
fn parenthesized_synctest_callees_still_use_virtual_time() {
    let source = r#"package p
import "time"
import "testing/synctest"
func wait() {
    (synctest.Run)(func() { time.Sleep(1) })
    (time.Sleep)(2)
}
"#;
    assert_eq!(sleep_lines(Lang::Go, source), [6]);
}

#[test]
fn dot_imported_synctest_conservatively_vetoes_the_file() {
    let source = r#"package p
import "time"
import . "testing/synctest"
func wait() { Run(func() { time.Sleep(1) }) }
"#;
    assert!(sleep_lines(Lang::Go, source).is_empty());
}

#[test]
fn go_local_bindings_veto_the_imported_name() {
    for body in [
        "func wait(time sleeper) { time.Sleep(1) }",
        "func wait() { time := sleeper{}; time.Sleep(1) }",
        "func wait() { var time sleeper; time.Sleep(1) }",
        "func wait() { for _, time := range clocks { time.Sleep(1) } }",
        "func wait() { func(time sleeper) { time.Sleep(1) }(sleeper{}) }",
        "func (time sleeper) wait() { time.Sleep(1) }",
    ] {
        let source = format!("package p\nimport \"time\"\n{body}\n");
        assert!(sleep_lines(Lang::Go, &source).is_empty(), "{source}");
    }
}

#[test]
fn python_binding_forms_veto_the_imported_name() {
    for body in [
        "def wait(time):\n    time.sleep(1)",
        "def wait():\n    time = clock\n    time.sleep(1)",
        "def wait():\n    for time in clocks:\n        time.sleep(1)",
        "with clock() as time:\n    time.sleep(1)",
        "try:\n    work()\nexcept Failure as time:\n    time.sleep(1)",
        "def wait():\n    (time := clock)\n    time.sleep(1)",
        "class time:\n    pass\ntime.sleep(1)",
        "time.sleep = fake_sleep\ntime.sleep(1)",
        "[time.sleep(1) for time in clocks]",
        "def wait():\n    import clocks as time\n    time.sleep(1)",
        "from clocks import *\ntime.sleep(1)",
        "del time\ntime.sleep(1)",
        "wait = lambda time: time.sleep(1)",
    ] {
        let source = format!("import time\n{body}\n");
        assert!(sleep_lines(Lang::Python, &source).is_empty(), "{source}");
    }
}

#[test]
fn python_local_and_wildcard_imports_remain_unclassified() {
    for source in [
        "def wait():\n    import time\n    time.sleep(1)\n",
        "def wait():\n    from time import sleep\n    sleep(1)\n",
        "from time import *\nsleep(1)\n",
        "from time import sleep\ndef wait(sleep):\n    sleep(1)\n",
    ] {
        assert!(sleep_lines(Lang::Python, source).is_empty(), "{source}");
    }
}

#[test]
fn python_sleep_patching_vetoes_from_import_aliases_too() {
    for patch in [
        "time.sleep = fake_sleep",
        "setattr(time, 'sleep', fake_sleep)",
        "patch.object(time, 'sleep')",
        "monkeypatch.setattr(time, 'sleep', fake_sleep)",
        "patch('time.sleep')",
        "replace('time.sleep')",
    ] {
        let source = format!(
            "import time\nfrom time import sleep as pause\n{patch}\ndef wait():\n    time.sleep(1)\n    pause(1)\n"
        );
        assert!(sleep_lines(Lang::Python, &source).is_empty(), "{source}");
    }
}

#[test]
fn separate_scope_shadows_conservatively_veto_the_whole_file() {
    let go = r#"package p
import "time"
func real() { time.Sleep(1) }
func other(time sleeper) { time.Sleep(2) }
"#;
    let python = r#"import time
def real():
    time.sleep(1)
def other(time):
    time.sleep(2)
"#;
    assert!(sleep_lines(Lang::Go, go).is_empty());
    assert!(sleep_lines(Lang::Python, python).is_empty());
}

#[test]
fn an_ambiguous_synctest_binding_does_not_turn_virtual_time_into_real_time() {
    let source = r#"package p
import "time"
import "testing/synctest"
func real() { synctest.Run(func() { time.Sleep(1) }) }
func other(synctest runner) { synctest.Run(func() { time.Sleep(2) }) }
"#;
    assert!(sleep_lines(Lang::Go, source).is_empty());
}

#[test]
fn repeated_sleep_calls_share_import_analysis() {
    let go = format!(
        "package p\nimport \"time\"\nfunc wait() {{\n{}}}\n",
        "time.Sleep(1)\n".repeat(2000)
    );
    let python = format!("import time\n{}", "time.sleep(1)\n".repeat(2000));
    assert_eq!(sleep_lines(Lang::Go, &go).len(), 2000);
    assert_eq!(sleep_lines(Lang::Python, &python).len(), 2000);
}
