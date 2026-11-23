fn tail_value(stop: bool) -> u32 {
    if stop {
        return 0;
    } else {
        1
    }
}

fn tail_position(stop: bool) {
    if stop {
        return;
    } else {
        proceed();
    }
}

fn scoped_cleanup(stop: bool) {
    if stop {
        return;
    } else {
        let guard = acquire();
        consume(&guard);
    }
    after_cleanup();
}

fn temporary_condition() {
    if acquire().ready() {
        return;
    } else {
        proceed();
    }
    finish();
}

fn pattern_condition(value: Option<u32>) {
    if let Some(found) = value {
        return;
    } else {
        proceed();
    }
    finish();
}

fn attributed_statement(stop: bool) {
    #[cfg(feature = "conditional")]
    if stop {
        return;
    } else {
        proceed();
    }
    finish();
}

fn attributed_return(stop: bool) {
    if stop {
        #[cfg(feature = "conditional")]
        return;
    } else {
        proceed();
    }
    finish();
}

fn macro_scope(stop: bool) {
    if stop {
        return;
    } else {
        scoped_work!();
    }
    finish();
}

fn named_panic(stop: bool) {
    if stop {
        panic!("not a syntax-proven return");
    } else {
        proceed();
    }
    finish();
}
