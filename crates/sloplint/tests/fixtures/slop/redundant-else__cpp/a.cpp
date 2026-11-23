void return_early(bool stop) {
    if (stop) {
        return;
    } else {
        proceed();
    }
}

void reject_early(bool stop) {
    if (stop) {
        throw Failure();
    } else {
        proceed();
    }
}

void retain_nested_scope(bool stop) {
    if (stop) {
        return;
    } else {
        {
            Guard guard;
            consume(guard);
        }
    }
    after_cleanup();
}
