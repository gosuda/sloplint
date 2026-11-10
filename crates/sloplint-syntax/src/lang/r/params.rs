use sloplint_core::facts::{Param, PrimitiveKind};
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, named_children, trimmed};

pub(crate) fn param_name<'s>(cx: &Cx<'_, 's>, parameter: Node<'_>) -> &'s str {
    child_by_field(parameter, "name")
        .map_or_else(|| trimmed(cx, parameter), |name| trimmed(cx, name))
}

fn r_param<'s>(cx: &Cx<'_, 's>, parameter: Node<'_>) -> Param<'s> {
    let name = param_name(cx, parameter);
    let flag = child_by_field(parameter, "default")
        .is_some_and(|value| matches!(trimmed(cx, value), "TRUE" | "FALSE"));
    Param {
        name,
        ty: None,
        primitive_kind: if flag {
            PrimitiveKind::Bool
        } else {
            PrimitiveKind::Other
        },
        has_default: child_by_field(parameter, "default").is_some(),
        bool_default: flag,
        keyword_only: false,
        is_variadic: name == "...",
    }
}

pub(crate) fn params<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Vec<Param<'s>> {
    let mut out: Vec<Param<'s>> =
        child_by_field(func, "parameters").map_or_else(Vec::new, |list| {
            named_children(list)
                .into_iter()
                .filter(|p| p.kind() == "parameter")
                .map(|parameter| r_param(cx, parameter))
                .collect()
        });
    let mut after_dots = false;
    for param in &mut out {
        param.keyword_only = after_dots;
        after_dots |= param.is_variadic;
    }
    out
}
