//! Three-argument equality assertions are ambiguous by arity: JUnit 4 puts the message
//! first (`assertEquals("why", 2, 3)`), JUnit 5 puts it last, and the delta overloads take
//! a numeric third argument without any message. A string literal or concatenation in the
//! first position can only be a message, because every delta overload starts with a
//! floating-point value.

use crate::support::with_facts;
use sloplint_core::lang::Lang;

const SOURCE: &str = "class T {
    void m(double expected, double actual, int count, String name) {
        assertEquals(\"count\", 2, 3);
        assertNotEquals(\"count\", 2, 3);
        assertEquals(\"count of \" + name, 2, 3);
        assertEquals((\"count\"), 2, 3);
        assertEquals(1.0, 2.0, 0.5);
        assertEquals(expected, actual, 0.5);
        assertEquals(2, 3);
        assertEquals(\"count\", count, name);
        assertEquals(expected, actual, \"within tolerance\");
        assertEquals(1.0, 2.0, 0.5, \"within tolerance\");
    }
}
";

/// `(line, has_message)` of every assertion in `SOURCE`, in source order.
fn messages() -> Vec<(u32, bool)> {
    with_facts(Lang::Java, "T.java", SOURCE, |facts| {
        facts
            .assertions
            .iter()
            .map(|a| (a.pos.line, a.has_message))
            .collect()
    })
}

fn has_message(line: u32) -> bool {
    messages()
        .into_iter()
        .find(|m| m.0 == line)
        .unwrap_or_else(|| panic!("no assertion on line {line}"))
        .1
}

#[test]
fn junit4_message_first_calls_with_numeric_values_carry_a_message() {
    assert!(has_message(3), "assertEquals(\"count\", 2, 3)");
    assert!(has_message(4), "assertNotEquals(\"count\", 2, 3)");
    assert!(has_message(5), "string concatenation first");
    assert!(has_message(6), "parenthesized string first");
}

#[test]
fn numeric_delta_and_bare_calls_still_have_no_message() {
    assert!(
        !has_message(7),
        "assertEquals(1.0, 2.0, 0.5) is a delta call"
    );
    assert!(
        !has_message(8),
        "assertEquals(expected, actual, 0.5) is a delta call"
    );
    assert!(!has_message(9), "two arguments never carry a message");
}

#[test]
fn string_valued_messages_and_four_argument_forms_still_carry_one() {
    assert!(has_message(10), "message-first with non-numeric values");
    assert!(has_message(11), "JUnit 5 message last");
    assert!(has_message(12), "delta call plus message");
}
