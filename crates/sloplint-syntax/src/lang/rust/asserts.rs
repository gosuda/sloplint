use sloplint_core::facts::{AssertShape, Assertion};
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, first_named_child_matching, last_segment, pos_of, text};

fn is_assert_macro(name: &str) -> bool {
    matches!(last_segment(name, &["::", "."]), "assert" | "debug_assert")
}

struct CondTally {
    named: u32,
    sole_ident: bool,
    extra: bool,
    seen_eq: bool,
    seen_logic: bool,
}

impl CondTally {
    fn shape(&self) -> AssertShape {
        if self.seen_logic {
            AssertShape::Other
        } else if self.seen_eq {
            AssertShape::TopEqNe
        } else if self.named == 1 && self.sole_ident && !self.extra {
            AssertShape::BareName
        } else {
            AssertShape::Other
        }
    }
}

struct CondShape {
    shape: AssertShape,
    has_message: bool,
    comparison: bool,
}

fn finish(tally: &CondTally, has_message: bool) -> CondShape {
    CondShape {
        shape: tally.shape(),
        has_message,
        comparison: tally.seen_eq,
    }
}

fn matching_close(open: &str) -> &str {
    match open {
        "(" => ")",
        "[" => "]",
        _ => "}",
    }
}

fn strip_outer<'a, 'b>(cx: &Cx<'_, 'b>, kids: &'a [Node<'b>]) -> &'a [Node<'b>] {
    let (Some(first), Some(last)) = (kids.first(), kids.last()) else {
        return kids;
    };
    if kids.len() >= 2 && matches!(text(cx, *first), "(" | "[" | "{") {
        let open = text(cx, *first);
        if text(cx, *last) == matching_close(open) {
            return &kids[1..kids.len() - 1];
        }
    }
    kids
}

fn scan_condition<'s>(cx: &Cx<'_, 's>, tt: Node<'_>) -> CondShape {
    let mut tally = CondTally {
        named: 0,
        sole_ident: false,
        extra: false,
        seen_eq: false,
        seen_logic: false,
    };
    let mut cursor = tt.walk();
    let kids: Vec<Node<'_>> = tt.children(&mut cursor).collect();
    let inner = strip_outer(cx, &kids);
    let mut depth = 0u32;
    for child in inner {
        if child.is_named() {
            if depth == 0 {
                tally.named += 1;
                tally.sole_ident = child.kind() == "identifier";
            }
            continue;
        }
        match text(cx, *child) {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth = depth.saturating_sub(1),
            "," if depth == 0 => return finish(&tally, true),
            "==" | "!=" if depth == 0 => tally.seen_eq = true,
            "||" | "&&" if depth == 0 => tally.seen_logic = true,
            _ => {
                if depth == 0 {
                    tally.extra = true;
                }
            }
        }
    }
    finish(&tally, false)
}

pub(super) fn assertion(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    if n.kind() != "macro_invocation" {
        return None;
    }
    let mac = child_by_field(n, "macro").map_or("", |m| text(cx, m));
    if !is_assert_macro(mac) {
        return None;
    }
    let tt = first_named_child_matching(n, &["token_tree"])?;
    let cond = scan_condition(cx, tt);
    Some(Assertion {
        pos: pos_of(n),
        has_message: cond.has_message,
        comparison: cond.comparison,
        shape: cond.shape,
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}
