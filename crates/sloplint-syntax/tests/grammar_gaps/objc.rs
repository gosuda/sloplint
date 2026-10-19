//! Valid Objective-C that the pinned tree-sitter-objc 3.0.2 grammar rejects.
//!
//! Every repro here ships in AFNetworking or SDWebImage headers, which are
//! plain non-preprocessed sources: `NS_ENUM`/`NS_OPTIONS`/`NS_ERROR_ENUM`
//! typedefs, trailing `NS_STRING_ENUM`-family annotations, bare
//! `NS_ASSUME_NONNULL_BEGIN`/`END` and `NS_SWIFT_*` annotations, stacked and
//! mid-enumerator availability attributes, `__has_warning` with a string
//! operand, `__has_include` with an angle-bracket operand, the
//! `NS_ENUM`/`NS_OPTIONS` fallback `#define`s, and `#ifdef` around an
//! `@property` or a method definition. The vendored grammar under
//! `vendor/tree-sitter-objc` accepts each one; the published crate reported a
//! parse error for each one (proven by swapping the dependency to the registry
//! version: 14 of these tests fail there). The guard tests pin inputs that
//! already parsed (a lone trailing availability attribute, the `NS_ENUM`
//! fallback defines, forward declarations, generics), and the control tests
//! pin inputs that stay errors, so the grammar cannot drift toward accepting
//! invalid code or regress existing behavior. The control tests read facts
//! from the parsed tree, so a node-shape regression shows up as a changed
//! fact and not only as a new error.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::ObjC,
        path: "gap.m",
        source,
        is_test_file: false,
        has_interface: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn run_clean(source: &str) -> Analysis<'_> {
    // Raw-tree verdict first: recovery can insert MISSING nodes that are invisible to
    // cursor walks, so an `analyze`-only check would pass them as clean (false green).
    // An error-to-hidden-MISSING swap is not a fix.
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_objc::LANGUAGE.into())
        .expect("objc language loads");
    let tree = parser.parse(source, None).expect("source parses to a tree");
    assert!(
        !tree.root_node().has_error(),
        "valid ObjC must parse cleanly on the raw tree in:\n{source}"
    );
    assert!(
        !tree.root_node().to_sexp().contains("MISSING"),
        "valid ObjC raw tree must not hide recovery nodes in:\n{source}"
    );
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_none(),
        "valid ObjC must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

fn type_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.types.iter().map(|t| t.name).collect()
}

#[test]
fn ns_enum_typedef_parses() {
    let source = concat!(
        "typedef NS_ENUM(NSUInteger, SDCallbackPolicy) {\n",
        "    SDCallbackPolicyFireAndForget = 0,\n",
        "    SDCallbackPolicyEnsureExecute = 1,\n",
        "};\n",
    );
    let analysis = run_clean(source);
    assert!(
        type_names(&analysis).contains(&"SDCallbackPolicy"),
        "the enum name is still a type: {:?}",
        analysis.facts.types
    );
}

#[test]
fn ns_options_typedef_parses() {
    let source = concat!(
        "typedef NS_OPTIONS(NSUInteger, SDImageCacheOptions) {\n",
        "    SDImageCacheRetryFailed = 1 << 0,\n",
        "    SDImageCacheLowPriority = 1 << 1,\n",
        "};\n",
    );
    let analysis = run_clean(source);
    assert!(
        type_names(&analysis).contains(&"SDImageCacheOptions"),
        "the options name is still a type: {:?}",
        analysis.facts.types
    );
}

#[test]
fn ns_error_enum_typedef_parses() {
    let source = concat!(
        "typedef NS_ERROR_ENUM(SDWebImageErrorDomain, SDWebImageError) {\n",
        "    SDWebImageErrorBadImageData = 0,\n",
        "};\n",
    );
    let analysis = run_clean(source);
    assert!(
        type_names(&analysis).contains(&"SDWebImageError"),
        "the error name is still a type: {:?}",
        analysis.facts.types
    );
}

#[test]
fn enumerator_with_availability_before_value_parses() {
    let source = concat!(
        "typedef NS_OPTIONS(NSUInteger, SDWebImageOptions) {\n",
        "    SDWebImageAvoidDecodeImage API_DEPRECATED(\"Use X instead\", macos(10.10, 10.10), ios(8.0, 8.0)) = 1 << 18,\n",
        "    SDWebImageScaleDownLargeImages = 1 << 20,\n",
        "};\n",
    );
    let analysis = run_clean(source);
    assert!(
        type_names(&analysis).contains(&"SDWebImageOptions"),
        "the options name is still a type: {:?}",
        analysis.facts.types
    );
}

#[test]
fn string_enum_annotations_parse() {
    let source = concat!(
        "typedef NSString * SDImageCoderOption NS_STRING_ENUM;\n",
        "typedef NSString * SDWebImageContextOption NS_EXTENSIBLE_STRING_ENUM;\n",
        "typedef NSInteger SDImageFormat NS_TYPED_EXTENSIBLE_ENUM;\n",
        "typedef NSInteger SDScalePolicy NS_TYPED_ENUM;\n",
    );
    let analysis = run_clean(source);
    for name in [
        "SDImageCoderOption",
        "SDWebImageContextOption",
        "SDImageFormat",
        "SDScalePolicy",
    ] {
        assert!(
            type_names(&analysis).contains(&name),
            "typedef `{name}` is still recorded: {:?}",
            analysis.facts.types
        );
    }
}

#[test]
fn plain_typedefs_still_parse() {
    let analysis = run_clean("typedef NSString * SDPlainOption;\ntypedef NSInteger SDPlainCount;\n");
    for name in ["SDPlainOption", "SDPlainCount"] {
        assert!(
            type_names(&analysis).contains(&name),
            "typedef `{name}` is still recorded: {:?}",
            analysis.facts.types
        );
    }
}

#[test]
fn assume_nonnull_region_parses() {
    let source = concat!(
        "NS_ASSUME_NONNULL_BEGIN\n",
        "\n",
        "@interface WindowController : NSWindowController\n",
        "\n",
        "@end\n",
        "\n",
        "NS_ASSUME_NONNULL_END\n",
    );
    let analysis = run_clean(source);
    assert!(
        type_names(&analysis).contains(&"WindowController"),
        "the class is still a type: {:?}",
        analysis.facts.types
    );
}

#[test]
fn assume_nonnull_end_inside_interface_parses() {
    let source = concat!(
        "@interface SDImageFramePool : NSObject\n",
        "- (void)removeAllFrames;\n",
        "NS_ASSUME_NONNULL_END\n",
        "@end\n",
    );
    // Method declarations carry no body, so the adapter records no function
    // fact for them; the class type proves the shape survived.
    let analysis = run_clean(source);
    assert!(
        type_names(&analysis).contains(&"SDImageFramePool"),
        "the class is still a type: {:?}",
        analysis.facts.types
    );
}

#[test]
fn swift_actor_annotations_parse() {
    let source = concat!(
        "NS_SWIFT_NONISOLATED\n",
        "@interface SDAnimatedImage : UIImage\n",
        "@end\n",
        "NS_SWIFT_UI_ACTOR\n",
        "@interface SDAnimatedImageView : UIImageView\n",
        "@end\n",
    );
    let analysis = run_clean(source);
    for name in ["SDAnimatedImage", "SDAnimatedImageView"] {
        assert!(
            type_names(&analysis).contains(&name),
            "the class `{name}` is still a type: {:?}",
            analysis.facts.types
        );
    }
}

#[test]
fn stacked_trailing_availability_parses() {
    let source = concat!(
        "@interface SDWebImageTransition : NSObject\n",
        "@property (nonatomic, strong, nullable) CAMediaTimingFunction *timingFunction API_UNAVAILABLE(ios, tvos, watchos) API_DEPRECATED(\"Use X instead\", macos(10.10, 10.10));\n",
        "@end\n",
    );
    run_clean(source);
}

#[test]
fn deprecated_with_replacement_on_method_parses() {
    let source = concat!(
        "@interface UIView_WebCache : NSObject\n",
        "- (void)sd_cancelCurrentImageLoad API_DEPRECATED_WITH_REPLACEMENT(\"sd_cancelLatestImageLoad\", macos(10.10, 10.10), ios(8.0, 8.0), tvos(9.0, 9.0), watchos(2.0, 2.0));\n",
        "@end\n",
    );
    // A method declaration carries no body, so no function fact is recorded;
    // parsing cleanly is the regression pin.
    run_clean(source);
}

#[test]
fn available_with_version_groups_on_property_parses() {
    let source = concat!(
        "@interface SDWebImageDownloader : NSObject\n",
        "@property (nonatomic, strong, nullable) NSURLSessionTaskMetrics *metrics API_AVAILABLE(macos(10.12), ios(10.0), watchos(3.0), tvos(10.0));\n",
        "@end\n",
    );
    run_clean(source);
}

#[test]
fn has_warning_with_string_operand_parses() {
    let source = concat!(
        "#if __has_warning(\"-Wunguarded-availability-new\")\n",
        "    #define AF_CAN_USE_AT_AVAILABLE 1\n",
        "#else\n",
        "    #define AF_CAN_USE_AT_AVAILABLE 0\n",
        "#endif\n",
    );
    run_clean(source);
}

#[test]
fn has_include_with_angle_and_quoted_operands_parses() {
    let source = concat!(
        "#if __has_include(<UIKit/UITraitCollection.h>) && !SD_WATCH\n",
        "#import <UIKit/UITraitCollection.h>\n",
        "#endif\n",
        "#if __has_include(\"SDWebImageCompat.h\")\n",
        "#import \"SDWebImageCompat.h\"\n",
        "#endif\n",
    );
    run_clean(source);
}

#[test]
fn ns_enum_fallback_defines_parse() {
    // SDWebImageCompat.h defines these when the SDK does not; the names are
    // keywords now, so the macro-name positions must still accept them. This
    // parsed before the patch and must keep parsing.
    let source = concat!(
        "#ifndef NS_ENUM\n",
        "#define NS_ENUM(_type, _name) enum _name : _type _name; enum _name : _type\n",
        "#endif\n",
        "#ifndef NS_OPTIONS\n",
        "#define NS_OPTIONS(_type, _name) enum _name : _type _name; enum _name : _type\n",
        "#endif\n",
    );
    run_clean(source);
}

#[test]
fn ifdef_around_property_in_interface_parses() {
    let source = concat!(
        "@interface User : NSObject\n",
        "@property (readonly, nonatomic, assign) NSUInteger userID;\n",
        "#ifdef __MAC_OS_X_VERSION_MIN_REQUIRED\n",
        "@property (nonatomic, strong) NSImage *profileImage;\n",
        "#endif\n",
        "@end\n",
    );
    run_clean(source);
}

#[test]
fn ifdef_around_method_definition_in_implementation_parses() {
    let source = concat!(
        "@implementation Foo\n",
        "#ifdef FOO\n",
        "- (void)bar {\n",
        "}\n",
        "#endif\n",
        "@end\n",
    );
    let analysis = run_clean(source);
    assert!(
        function_names(&analysis).contains(&"bar"),
        "the method is still recorded: {:?}",
        function_names(&analysis)
    );
}

#[test]
fn forward_class_and_guarded_import_still_parse() {
    // These parsed before the patch and must keep parsing.
    // A function declaration carries no body, so no function fact is
    // recorded; parsing cleanly is the regression pin. The category interface
    // and the protocol still yield their types.
    let analysis = run_clean(concat!(
        "@class AFImageDownloader;\n",
        "#if TARGET_OS_IOS || TARGET_OS_TV\n",
        "#import <UIKit/UIKit.h>\n",
        "#endif\n",
        "FOUNDATION_EXPORT NSString * AFPercentEscapedStringFromString(NSString *string);\n",
        "@interface UIProgressView (AFNetworking)\n",
        "@end\n",
        "@protocol SDImageLoader <NSObject>\n",
        "@end\n",
    ));
    assert!(
        type_names(&analysis).contains(&"SDImageLoader"),
        "the protocol is still a type: {:?}",
        analysis.facts.types
    );
}

#[test]
fn generic_class_and_protocol_id_params_still_parse() {
    // Lightweight generics and protocol-qualified `id` parsed before the
    // patch and must keep parsing.
    let source = concat!(
        "@interface MyClass<ObjectType> : NSObject\n",
        "@property (nonatomic, strong) NSArray<NSString *> *items;\n",
        "- (void)add:(NSArray<NSNumber *> *)nums;\n",
        "- (void)setLoader:(id<SDImageLoader>)loader;\n",
        "@end\n",
    );
    let analysis = run_clean(source);
    assert!(
        type_names(&analysis).contains(&"MyClass"),
        "the class is still a type: {:?}",
        analysis.facts.types
    );
}

#[test]
fn an_unterminated_interface_still_reports_a_parse_error() {
    let analysis = run("@interface Foo : NSObject\n");
    assert!(
        analysis.parse_error.is_some(),
        "a missing @end must not parse"
    );
}

#[test]
fn a_broken_method_body_still_reports_a_parse_error() {
    let analysis = run("@implementation Foo\n- (void)bar {\n@end\n");
    assert!(
        analysis.parse_error.is_some(),
        "a missing method closing brace must not parse"
    );
}

#[test]
fn a_broken_declaration_still_reports_a_parse_error() {
    let analysis = run("int x = ;\n");
    assert!(analysis.parse_error.is_some(), "`int x = ;` must not parse");
}

#[test]
fn an_ns_enum_without_its_semicolon_still_reports_a_parse_error() {
    let analysis = run("typedef NS_ENUM(NSUInteger, Foo) {\n    FooA = 0,\n}\n");
    assert!(
        analysis.parse_error.is_some(),
        "an NS_ENUM typedef without `;` must not parse"
    );
}
