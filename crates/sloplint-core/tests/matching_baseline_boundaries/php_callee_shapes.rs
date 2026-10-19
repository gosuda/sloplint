use sloplint_core::CalleePattern;
use sloplint_core::facts::CallKind;

fn observed(pattern: &str, callee: &str, kind: CallKind) -> bool {
    CalleePattern::parse(pattern)
        .unwrap_or_else(|error| panic!("pattern {pattern:?} must parse: {error}"))
        .matches(callee, kind)
}

fn hit_as(pattern: &str, callee: &str, kind: CallKind) {
    assert!(
        observed(pattern, callee, kind),
        "pattern {pattern:?} must match callee {callee:?} as {kind:?}"
    );
}

fn miss_as(pattern: &str, callee: &str, kind: CallKind) {
    assert!(
        !observed(pattern, callee, kind),
        "pattern {pattern:?} must not match callee {callee:?} as {kind:?}"
    );
}

fn hit(pattern: &str, callee: &str) {
    hit_as(pattern, callee, CallKind::Plain);
}

fn miss(pattern: &str, callee: &str) {
    miss_as(pattern, callee, CallKind::Plain);
}

#[test]
fn member_and_static_callees_split_on_arrows_and_namespace_separators() {
    for callee in [
        "$this->logger->info",
        "$logger->info",
        "$this\n    ->logger\n    ->info",
        "$this->logger?->info",
        "$this ->logger ?-> info",
        "logger->info",
    ] {
        hit("logger.info", callee);
    }
    hit("Log::info", "\\App\\Log::info");
    hit("Log::info", "Log::info");
    hit("App.Log::info", "\\App\\Log::info");
    hit("error_log", "\\error_log");
    hit_as("*Exception", "\\RuntimeException", CallKind::New);
    hit_as("new *Exception", "\\RuntimeException", CallKind::New);
    hit_as(
        "new *Exception",
        "\\App\\Failures\\DomainException",
        CallKind::New,
    );
    miss_as("new *Exception", "\\RuntimeException", CallKind::Plain);
}

#[test]
fn near_misses_do_not_match() {
    miss("logger.info", "$this->mylogger->info");
    miss("logger.info", "$this->logger->infox");
    miss("logger.info", "$this->logger->info->more");
    miss("Log::info", "\\App\\Blog::info");
    miss("Log::info", "\\App\\Log::warning");
    miss("a.b.c", "$b->c");
    hit("info", "$this->logger->info");
}

#[test]
fn leading_dollar_is_ignored_only_before_more_characters() {
    hit("$.ajax", "$.ajax");
    hit("ajax", "$.ajax");
    miss("x.ajax", "$.ajax");
    hit("$", "$");
    hit("$el.on", "$el.on");
    hit("el.on", "$el.on");
    hit("$el.on", "el.on");
    hit("*Error", "$Error");
    hit("*$Error", "$Error");
    miss("$$el", "$el");
    hit("$$el", "$$el");
}

#[test]
fn dollar_sigil_follows_the_segment_not_the_callee() {
    hit("logger.info", "Foo::$logger->info");
    hit("this.logger.info", "$this->logger->info");
    miss("this.logger.info", "$that->logger->info");
}

#[test]
fn arrow_and_backslash_patterns_are_rejected_with_a_message_naming_them() {
    for pattern in [
        "a->b",
        "$this->logger",
        "App\\Log::info",
        "a?->b",
        "\\error_log",
    ] {
        let message = match CalleePattern::parse(pattern) {
            Ok(_) => panic!("pattern {pattern:?} must be rejected"),
            Err(message) => message,
        };
        assert!(
            message.contains(pattern),
            "error for {pattern:?} must name the pattern, got {message:?}"
        );
    }
    hit("a.b", "a->b");
    hit("App.Log::info", "App\\Log::info");
}
