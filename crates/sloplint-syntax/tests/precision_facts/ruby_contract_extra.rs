use sloplint_core::facts::ArgShape;

use super::ruby::with_facts;

const CONCAT_MSG: &str = "logger.error(\"Failed to save order \" + order.id.to_s + suffix)\nlogger.error(order.id.to_s + suffix)\n";

#[test]
fn ruby_left_deep_concat_carries_longest_static() {
    with_facts(CONCAT_MSG, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses concat calls");
        let calls: Vec<_> = analysis
            .facts
            .calls
            .iter()
            .filter(|c| c.callee == "logger.error")
            .collect();
        assert_eq!(calls.len(), 2, "finds both error calls");
        assert_eq!(
            calls[0].args[0].shape,
            ArgShape::Concat { longest_static: 21 },
            "left-deep literal measures 21"
        );
        assert_eq!(
            calls[1].args[0].shape,
            ArgShape::Other,
            "dynamic-only plus stays Other"
        );
    });
}

const INTERPOLATED: &str = "greet(\"Hi #{name}\")\ngreet(\"#{code}\")\ngreet(\"Failed\")\n";

#[test]
fn ruby_interpolated_string_carries_short_static() {
    with_facts(INTERPOLATED, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses greetings");
        let calls: Vec<_> = analysis
            .facts
            .calls
            .iter()
            .filter(|c| c.callee == "greet")
            .collect();
        assert_eq!(calls.len(), 3, "finds all calls");
        assert_eq!(
            calls[0].args[0].shape,
            ArgShape::Concat { longest_static: 3 },
            "interpolated greeting measures 3"
        );
        assert_eq!(
            calls[1].args[0].shape,
            ArgShape::Concat { longest_static: 0 },
            "static-less interpolation measures 0"
        );
        assert_eq!(
            calls[2].args[0].shape,
            ArgShape::Str { longest_static: 6 },
            "plain literal stays Str"
        );
    });
}

const PAREN_CONCAT: &str =
    "logger.error(prefix + ('Failed to save order ' + id))\nlogger.error(prefix + (suffix))\n";

#[test]
fn ruby_parenthesized_operand_keeps_its_static_run() {
    with_facts(PAREN_CONCAT, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses paren concats");
        let calls: Vec<_> = analysis
            .facts
            .calls
            .iter()
            .filter(|c| c.callee == "logger.error")
            .collect();
        assert_eq!(calls.len(), 2, "finds both calls");
        assert_eq!(
            calls[0].args[0].shape,
            ArgShape::Concat { longest_static: 21 },
            "literal inside parens measures 21"
        );
        assert_eq!(
            calls[1].args[0].shape,
            ArgShape::Other,
            "parens without a string stay Other"
        );
    });
}

const LAMBDA_ARG: &str = "register(->(req) { handle(req) })\nregister(handler)\n";

#[test]
fn ruby_lambda_argument_is_a_code_body() {
    with_facts(LAMBDA_ARG, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses lambda calls");
        let calls: Vec<_> = analysis
            .facts
            .calls
            .iter()
            .filter(|c| c.callee == "register")
            .collect();
        assert_eq!(calls.len(), 2, "finds both calls");
        assert!(
            matches!(calls[0].args[0].shape, ArgShape::CodeBody),
            "lambda is a code body"
        );
        assert!(
            matches!(calls[1].args[0].shape, ArgShape::Ident(_)),
            "identifier stays Ident"
        );
    });
}

const UPDATE_COPY: &str = "order.update(\"total\" => src.total, \"tax\" => src.tax, \"fee\" => src.fee, \"net\" => src.net, \"gross\" => src.gross, \"tip\" => src.tip)\n";

#[test]
fn ruby_method_update_is_not_a_field_copy() {
    with_facts(UPDATE_COPY, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses update");
        assert!(
            analysis.facts.field_copies.is_empty(),
            "method update is not a type construction"
        );
    });
}

const KEYED_ARG: &str = "submit({a: 1, b: 2})\nsubmit([1, 2])\n";

#[test]
fn ruby_keyed_literal_argument_is_other() {
    with_facts(KEYED_ARG, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses submits");
        let calls: Vec<_> = analysis
            .facts
            .calls
            .iter()
            .filter(|c| c.callee == "submit")
            .collect();
        assert_eq!(calls.len(), 2, "finds both calls");
        assert_eq!(
            calls[0].args[0].shape,
            ArgShape::Other,
            "keyed literal is not a collection"
        );
        assert_eq!(
            calls[1].args[0].shape,
            ArgShape::Collection,
            "plain array stays a collection"
        );
    });
}

const CLOSURE_ELEM: &str = "submit([->(x) { x }])\n";

#[test]
fn ruby_closure_literal_argument_is_code_body() {
    with_facts(CLOSURE_ELEM, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses submit");
        let call = analysis
            .facts
            .calls
            .iter()
            .find(|c| c.callee == "submit")
            .expect("has submit call");
        assert_eq!(
            call.args[0].shape,
            ArgShape::CodeBody,
            "array holding a closure is a code body"
        );
    });
}

const BARE_COPY: &str =
    "Point(\"x\" => s.x, \"y\" => s.y, \"z\" => s.z, \"w\" => s.w, \"v\" => s.v, \"u\" => s.u)\n";

#[test]
fn ruby_bare_constructor_guard_stays_a_field_copy() {
    with_facts(BARE_COPY, |analysis| {
        assert!(analysis.parse_error.is_none(), "parses constructor");
        assert_eq!(
            analysis.facts.field_copies.len(),
            1,
            "guard: bare constructor still copies"
        );
    });
}
