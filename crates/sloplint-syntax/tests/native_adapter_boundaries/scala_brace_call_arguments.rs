//! A Scala call written with braces (`Future { ... }`, `xs.map { case ... }`) has one
//! argument: the whole block. Its statements and case clauses are not separate arguments.

use crate::support::with_facts;
use sloplint_core::facts::ArgShape;
use sloplint_core::lang::Lang;

const LINES: [&str; 18] = [
    "object A {",
    "  def run(xs: List[Int]): Unit = {",
    "    Future {",
    "      step1()",
    "      step2()",
    "      step3()",
    "      step4()",
    "      step5()",
    "      step6()",
    "    }",
    "    xs.map {",
    "      case 1 => \"one\"",
    "      case _ => \"other\"",
    "    }",
    "    foo(a, \"lit\", b)",
    "    Future(step1())",
    "  }",
    "}",
];

fn describe(shape: &ArgShape<'_>) -> String {
    match shape {
        ArgShape::Str { longest_static } => format!("str:{longest_static}"),
        ArgShape::Concat => "concat".to_string(),
        ArgShape::Ident(name) => format!("ident:{name}"),
        ArgShape::Other => "other".to_string(),
    }
}

/// `(shape, start_line, end_line)` of every argument of the call that starts on `line`.
fn args_of_call_on(line: u32) -> Vec<(String, u32, u32)> {
    let source = LINES.join("\n") + "\n";
    with_facts(Lang::Scala, "A.scala", &source, |facts| {
        let call = facts
            .calls
            .iter()
            .find(|c| c.pos.line == line)
            .unwrap_or_else(|| panic!("no call starts on line {line}"));
        call.args
            .iter()
            .map(|a| (describe(&a.shape), a.start_line, a.end_line))
            .collect()
    })
}

#[test]
fn a_brace_block_is_one_argument_spanning_the_whole_block() {
    assert_eq!(
        args_of_call_on(3),
        [("other".to_string(), 3u32, 10u32)],
        "`Future {{ ... }}` is one argument, not one per statement"
    );
}

#[test]
fn a_case_block_is_one_argument_spanning_every_clause() {
    assert_eq!(
        args_of_call_on(11),
        [("other".to_string(), 11u32, 14u32)],
        "`xs.map {{ case ... }}` is one argument, not one per clause"
    );
}

#[test]
fn parenthesized_arguments_are_listed_one_by_one() {
    let found = args_of_call_on(15);
    assert_eq!(
        found,
        [
            ("ident:a".to_string(), 15u32, 15u32),
            ("str:3".to_string(), 15u32, 15u32),
            ("ident:b".to_string(), 15u32, 15u32),
        ],
        "parenthesized arguments are listed one by one; observed {found:?}"
    );
    assert_eq!(
        args_of_call_on(16),
        [("other".to_string(), 16u32, 16u32)],
        "a single parenthesized call argument"
    );
}
