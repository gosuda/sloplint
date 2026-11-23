fn return_early(stop: bool) {
    if stop {
        return;
    } else {
        proceed();
    }
    finish();
}

fn break_early(stop: bool) {
    loop {
        if stop {
            break;
        } else {
            proceed();
        };
    }
}

fn continue_early(stop: bool) {
    loop {
        if stop {
            continue;
        } else {
            proceed();
        };
    }
}
