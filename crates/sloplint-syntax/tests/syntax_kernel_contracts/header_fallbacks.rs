//! A `.h` file routes to C, but C++ and Objective-C headers share the extension. When the C
//! parse of a `.h` file has an error, the file is parsed again as C++ and then as
//! Objective-C; the first clean tree is analyzed with that grammar's adapter, and any other
//! outcome keeps the C parse error. The facts carry the language of the grammar that
//! produced them, so a clean fallback reads as C++ or Objective-C and everything else reads
//! as C.

use sloplint_core::facts::{FileFacts, Function};
use sloplint_core::lang::Lang;
use sloplint_core::{CustomRuleSpec, Pos, Severity};
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

use crate::support::{run, run_clean};

const CPP_HEADER: &str = "#pragma once
#include <string>
#include <vector>

namespace ui {

template <typename T>
class Stack {
public:
    void push(const T& value) { items_.push_back(value); }
    bool empty() const { return items_.empty(); }
private:
    std::vector<T> items_;
};

class Widget {
public:
    explicit Widget(std::string name) : name_(std::move(name)) {}
    virtual ~Widget() {}
    const std::string& name() const { return name_; }
private:
    std::string name_;
};

}  // namespace ui
";

const C_HEADER: &str = "#include <string.h>

unsigned long get_length(const char *s) {
    return strlen(s);
}

static int twice(int n) {
    if (n > 0) {
        return n * 2;
    }
    return 0;
}
";

/// The `template` on line 1 fails the C parse; `= ;` on line 3 fails the C++ parse.
const INVALID_IN_BOTH: &str = "template <typename T>
struct Box { T value; };
int x = ;
";

fn function<'a, 'f>(facts: &'a FileFacts<'f>, name: &str) -> &'a Function<'f> {
    facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no function {name}"))
}

#[test]
fn a_cpp_header_in_a_dot_h_file_is_analyzed_with_the_cpp_adapter() {
    let analysis = run_clean(Lang::C, "include/widget.h", CPP_HEADER);
    let facts = &analysis.facts;
    assert_eq!(
        facts.lang,
        Lang::Cpp,
        "the facts carry the language of the grammar that produced them"
    );
    assert!(
        function(facts, "~Widget").interface_bound,
        "a virtual destructor is a C++ adapter fact"
    );
    assert!(
        !function(facts, "name").interface_bound,
        "a plain member is not interface-bound"
    );
    for member in ["push", "empty"] {
        function(facts, member);
    }
    let types: Vec<&str> = facts.types.iter().map(|t| t.name).collect();
    for class in ["Stack", "Widget"] {
        assert!(types.contains(&class), "{class} is a type: {types:?}");
    }
}

/// `class_specifier` is a C++-only node kind, so these types exist only when the C++ role
/// table walked the tree, and `lang` must say so.
#[test]
fn a_fallback_header_reads_through_the_cpp_role_table_and_says_so() {
    let source = "#include <vector>
class Shape {
public:
    virtual ~Shape() {}
    virtual int area() const = 0;
};
class Square : public Shape {
public:
    int area() const override { return 4; }
};
";
    let as_c = run(Lang::C, "shapes.c", source);
    assert!(
        as_c.parse_error.is_some(),
        "the C grammar cannot parse `class`, so the same bytes prove the C++ table did the work"
    );
    let analysis = run_clean(Lang::C, "shapes.h", source);
    let facts = &analysis.facts;
    assert_eq!(facts.lang, Lang::Cpp, "the walk used the C++ role table");
    let types: Vec<&str> = facts.types.iter().map(|t| t.name).collect();
    assert_eq!(
        types,
        ["Shape", "Square"],
        "the C++ table maps `class_specifier` to a type"
    );
    assert!(
        function(facts, "area").interface_bound,
        "C++ virtual dispatch marks the member interface-bound"
    );
}

#[test]
fn a_clean_c_header_and_a_failed_header_keep_c_as_their_language() {
    let clean = run_clean(Lang::C, "get_length.h", C_HEADER);
    assert_eq!(clean.facts.lang, Lang::C, "a header that parses as C is C");
    let failed = run(Lang::C, "box.h", INVALID_IN_BOTH);
    assert!(
        failed.parse_error.is_some(),
        "the header is invalid in both"
    );
    assert_eq!(
        failed.facts.lang,
        Lang::C,
        "a failed parse has no C++ tree, so its empty facts keep the routed language"
    );
}

/// In C a `R"x(` raw string is an identifier and a string, so the text after the string is a
/// `//` comment. In C++ it is one raw string and holds no comment. Which tree supplied the
/// comments therefore shows in which `sloplint-allow` comments come back.
const RAW_STRING_COMMENT: &str =
    "const char* raw = R\"x(\" // sloplint-allow(grep/generic-name): inside a C++ raw string
)x\";
";

fn spans(analysis: &sloplint_syntax::Analysis<'_>) -> Vec<(u32, u32, String)> {
    analysis
        .comments
        .iter()
        .map(|c| (c.start_line, c.end_line, c.text.to_string()))
        .collect()
}

#[test]
fn a_fallback_header_reports_the_comments_of_the_cpp_tree() {
    let source = format!(
        "template <typename T>
struct Box {{ T value; }};
{RAW_STRING_COMMENT}// sloplint-allow(slop/passthrough-wrapper): a real comment
/* sloplint-allow(grep/*): block
   comment */
int twice(int n) {{ return n * 2; }}
"
    );
    let analysis = run_clean(Lang::C, "box.h", &source);
    assert_eq!(
        analysis.facts.lang,
        Lang::Cpp,
        "the C++ tree is the one used"
    );
    assert_eq!(
        spans(&analysis),
        [
            (
                5,
                5,
                "// sloplint-allow(slop/passthrough-wrapper): a real comment".to_string()
            ),
            (
                6,
                7,
                "/* sloplint-allow(grep/*): block\n   comment */".to_string()
            ),
        ],
        "the raw string hides a comment from the C++ tree, and the C tree's view is not mixed in"
    );
}

#[test]
fn a_header_invalid_in_both_grammars_reports_the_comments_of_the_c_tree() {
    let source = format!(
        "template <typename T>
struct Box {{ T value; }};
{RAW_STRING_COMMENT}// sloplint-allow(slop/passthrough-wrapper): a real comment
int x = ;
"
    );
    let header = run(Lang::C, "box.h", &source);
    let twin = run(Lang::C, "box.c", &source);
    assert_eq!(header.parse_error, Some(Pos { line: 1, byte: 0 }));
    assert_eq!(
        spans(&header),
        [
            (
                3,
                3,
                "// sloplint-allow(grep/generic-name): inside a C++ raw string".to_string()
            ),
            (
                5,
                5,
                "// sloplint-allow(slop/passthrough-wrapper): a real comment".to_string()
            ),
        ],
        "both failed, so the comments are the C tree's, raw-string 'comment' included"
    );
    assert_eq!(
        spans(&header),
        spans(&twin),
        "the same bytes at a `.c` path, which never retries, give the same comments"
    );
}

#[test]
fn a_cpp_source_in_a_dot_c_file_is_not_retried() {
    let analysis = run(Lang::C, "widget.c", CPP_HEADER);
    assert!(
        analysis.parse_error.is_some(),
        "only `.h` files fall back to C++"
    );
    assert!(
        analysis.facts.functions.is_empty(),
        "a failed parse yields empty facts"
    );
}

#[test]
fn a_dot_h_path_in_another_routed_language_is_not_retried() {
    let analysis = run(Lang::ObjC, "widget.h", CPP_HEADER);
    assert!(
        analysis.parse_error.is_some(),
        "the fallback belongs to the C route alone"
    );
}

#[test]
fn a_plain_c_header_keeps_its_c_facts() {
    let header = run_clean(Lang::C, "get_length.h", C_HEADER);
    let source = run_clean(Lang::C, "get_length.c", C_HEADER);
    assert_eq!(
        header.facts.functions, source.facts.functions,
        "a `.h` file that parses as C must read exactly as the same bytes in a `.c` file"
    );
    assert_eq!(header.facts.calls, source.facts.calls);
    assert_eq!(header.facts.types, source.facts.types);
    let twice = function(&header.facts, "twice");
    assert_eq!(twice.cognitive, 1, "one `if` at depth zero scores one");
    let length = function(&header.facts, "get_length");
    let forwarded = length.sole_call.expect("get_length forwards a single call");
    let forwarded = usize::try_from(forwarded).expect("call index fits usize");
    assert_eq!(
        header.facts.calls[forwarded].callee, "strlen",
        "the forwarded call is `strlen`"
    );
}

#[test]
fn a_header_invalid_in_both_grammars_reports_the_c_parse_error() {
    let analysis = run(Lang::C, "box.h", INVALID_IN_BOTH);
    assert_eq!(
        analysis.parse_error,
        Some(Pos { line: 1, byte: 0 }),
        "the position is the C parse's first error, not the C++ parse's"
    );
    let as_c = run(Lang::C, "box.c", INVALID_IN_BOTH);
    assert_eq!(
        analysis.parse_error, as_c.parse_error,
        "the outcome equals the behavior without a fallback"
    );
    assert!(
        analysis.facts.functions.is_empty() && analysis.custom_hits.is_empty(),
        "a failed parse yields empty facts"
    );
}

#[test]
fn a_failed_header_keeps_the_comments_of_the_c_parse() {
    let source = "// sloplint-allow(grep/generic-name): keep\ntemplate <typename T>\nint x = ;\n";
    let analysis = run(Lang::C, "box.h", source);
    assert!(analysis.parse_error.is_some(), "the file stays invalid");
    assert_eq!(
        analysis.comments.len(),
        1,
        "the suppression comment survives a failed parse"
    );
}

fn query_rule(language: &str, query: &str) -> CustomRuleSpec {
    CustomRuleSpec {
        id: format!("custom/{language}-hit"),
        language: language.to_string(),
        severity: Severity::Warn,
        message: "hit".to_string(),
        why: "test why".to_string(),
        fix: "test fix".to_string(),
        query: query.to_string(),
        include: Vec::new(),
        exclude: Vec::new(),
    }
}

/// Plain Objective-C: `@protocol`, `@interface` and `@property` fail the C grammar and the C++
/// grammar alike, and only the Objective-C grammar parses them.
const OBJC_HEADER: &str = "#import <Foundation/Foundation.h>

@protocol Greeter <NSObject>
- (NSString *)greet;
@end

@interface Counter : NSObject
@property (nonatomic, assign) NSInteger count;
- (void)increment;
@end

static inline void LogCount(NSString *name) {
    NSLog(name);
}
";

/// A C++ header that the Objective-C grammar also accepts, because it reads `HttpRouter<...>
/// router;` as an expression statement. The C grammar rejects the `namespace`.
const CPP_AND_OBJC_HEADER: &str = "#include \"HttpRouter.h\"

namespace uWS {

struct Http3Response;

struct Http3ContextData {
    struct RouterData {
        Http3Response *httpResponse;
    };
    HttpRouter<RouterData> router;
};

inline void load(const char *name) {
    Probe(name);
}

}
";

/// `@interface` fails as C, fails as C++, and parses as Objective-C, so the facts must come
/// from the Objective-C adapter and say so.
#[test]
fn an_objc_header_in_a_dot_h_file_is_analyzed_with_the_objc_adapter() {
    for (lang, path) in [(Lang::C, "Counter.c"), (Lang::Cpp, "Counter.cpp")] {
        assert!(
            run(lang, path, OBJC_HEADER).parse_error.is_some(),
            "{path}: the same bytes must fail the other grammars, or this proves nothing"
        );
    }
    let analysis = run_clean(Lang::C, "include/Counter.h", OBJC_HEADER);
    let facts = &analysis.facts;
    assert_eq!(
        facts.lang,
        Lang::ObjC,
        "the facts carry the language of the grammar that produced them"
    );
    let types: Vec<&str> = facts.types.iter().map(|t| t.name).collect();
    assert_eq!(
        types,
        ["Greeter", "Counter"],
        "`@protocol` and `@interface` are the Objective-C table's types"
    );
    function(facts, "LogCount");
    assert!(
        facts.calls.iter().any(|c| c.callee == "NSLog"),
        "the C function body is walked: {:?}",
        facts.calls
    );
    assert!(
        analysis.parse_error.is_none() && analysis.facts.path == "include/Counter.h",
        "the file keeps its own path"
    );
}

/// The Objective-C grammar parses this header too, so only the order decides: C++ comes first.
#[test]
fn a_header_valid_as_cpp_and_as_objc_resolves_to_cpp() {
    for (lang, path) in [(Lang::Cpp, "Router.cpp"), (Lang::ObjC, "Router.m")] {
        assert!(
            run(lang, path, CPP_AND_OBJC_HEADER).parse_error.is_none(),
            "{path}: both grammars must accept the sample, or the order is not tested"
        );
    }
    assert!(
        run(Lang::C, "Router.c", CPP_AND_OBJC_HEADER)
            .parse_error
            .is_some(),
        "the C grammar rejects the sample, so the header reaches the fallbacks"
    );
    for _ in 0..3 {
        let analysis = run_clean(Lang::C, "Router.h", CPP_AND_OBJC_HEADER);
        assert_eq!(
            analysis.facts.lang,
            Lang::Cpp,
            "C++ is tried before Objective-C"
        );
        function(&analysis.facts, "load");
    }
}

#[test]
fn a_header_invalid_in_all_three_grammars_reports_the_c_parse_error() {
    let source = "@interface Counter : NSObject
@end
int x = ;
";
    let header = run(Lang::C, "Counter.h", source);
    let as_c = run(Lang::C, "Counter.c", source);
    let as_objc = run(Lang::ObjC, "Counter.m", source);
    assert!(
        run(Lang::Cpp, "Counter.cpp", source).parse_error.is_some()
            && as_objc.parse_error.is_some(),
        "the sample fails C++ and Objective-C"
    );
    assert_ne!(
        as_c.parse_error, as_objc.parse_error,
        "the C and Objective-C errors differ, so the reported position tells them apart"
    );
    assert_eq!(
        header.parse_error, as_c.parse_error,
        "the position is the C parse's first error, not the last fallback's"
    );
    assert_eq!(
        header.facts.lang,
        Lang::C,
        "a failed parse keeps the routed language"
    );
    assert!(
        header.facts.functions.is_empty() && header.custom_hits.is_empty(),
        "failed header yields no facts: {:?} functions, {} custom hits",
        header.facts.functions,
        header.custom_hits.len()
    );
}

#[test]
fn an_objc_header_keeps_its_suppression_comments() {
    let source = format!("// sloplint-allow(grep/generic-name): keep\n{OBJC_HEADER}");
    let analysis = run_clean(Lang::C, "Counter.h", &source);
    assert_eq!(analysis.facts.lang, Lang::ObjC);
    assert_eq!(spans(&analysis).len(), 1, "{:?}", spans(&analysis));
    assert_eq!(spans(&analysis)[0].0, 1);
}

/// A query matches the tree that was parsed, so a header analyzed as C++ runs the `cpp` rules,
/// one analyzed as Objective-C runs the `objc` rules, and one analyzed as C runs the `c` rules,
/// never the other way around.
#[test]
fn custom_queries_run_against_the_tree_that_was_parsed() {
    let rules = CustomRules::compile(&[
        query_rule("cpp", "(class_specifier) @match"),
        query_rule("c", "(function_definition) @match"),
        query_rule("objc", "(class_interface) @match"),
    ])
    .expect("rules compile");
    let hits = |path: &str, source: &str| {
        let input = FileInput {
            lang: Lang::C,
            path,
            source,
            is_test_file: false,
        };
        let analysis = analyze(&mut Parsers::new(), &input, &rules);
        assert!(analysis.parse_error.is_none(), "{path} parses cleanly");
        analysis
            .custom_hits
            .iter()
            .map(|hit| hit.rule)
            .collect::<Vec<u32>>()
    };
    assert_eq!(
        hits("widget.h", CPP_HEADER),
        [0, 0],
        "the C++ class query matches both classes and the C query is skipped"
    );
    assert_eq!(
        hits("get_length.h", C_HEADER),
        [1, 1],
        "the C function query matches both functions and the C++ query is skipped"
    );
    assert_eq!(
        hits("Counter.h", OBJC_HEADER),
        [2],
        "the Objective-C interface query matches the one interface, and neither the C nor the C++ query runs"
    );
}
