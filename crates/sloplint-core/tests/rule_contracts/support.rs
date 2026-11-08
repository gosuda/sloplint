//! Hand-built facts: the rules and the line index only read these public fields.

use sloplint_core::facts::{FileFacts, Function, Pos};
use sloplint_core::finding::line_starts;
use sloplint_core::lang::Lang;

/// Facts for `source` with no functions, calls, or other sites.
pub fn facts_for(lang: Lang, path: &'static str, source: &'static str) -> FileFacts<'static> {
    FileFacts {
        lang,
        path,
        source,
        line_starts: line_starts(source),
        is_test_file: false,
        is_entry_file: false,
        line_count: 0,
        functions: Vec::new(),
        calls: Vec::new(),
        bindings: Vec::new(),
        imports: Vec::new(),
        types: Vec::new(),
        impls: Vec::new(),
        wildcard_arms: Vec::new(),
        handlers: Vec::new(),
        assertions: Vec::new(),
        field_copies: Vec::new(),
        dynamic_symbols: Vec::new(),
        ambient: Vec::new(),
        barrel: None,
        has_inline_tests: false,
    }
}

/// A public, plain function at `pos`.
pub fn function(name: &'static str, pos: Pos) -> Function<'static> {
    Function {
        name,
        pos,
        doc: Vec::new(),
        doc_pos: None,
        public: true,
        interface_bound: false,
        in_test: false,
        params: Vec::new(),
        return_type: None,
        untyped_throws: false,
        missing_annotations: Vec::new(),
        cognitive: 0,
        max_depth: 0,
        deepest_line: 0,
        sole_call: None,
    }
}

/// The position where `needle` first appears in `source`.
pub fn pos_of(source: &str, needle: &str) -> Pos {
    let Some(byte) = source.find(needle) else {
        panic!("{needle:?} is not in the source");
    };
    let line = source[..byte].matches('\n').count() + 1;
    Pos {
        line: u32::try_from(line).expect("small source"),
        byte: u32::try_from(byte).expect("small source"),
    }
}
