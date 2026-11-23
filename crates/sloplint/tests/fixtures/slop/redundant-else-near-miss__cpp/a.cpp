void scoped_cleanup(bool stop) {
    if (stop) {
        return;
    } else {
        Guard guard;
        consume(guard);
    }
    after_cleanup();
}

void condition_binding() {
    if (Guard guard = acquire()) {
        return;
    } else {
        consume(guard);
    }
}

void condition_initializer(bool stop) {
    if (Guard guard = acquire(); stop) {
        return;
    } else {
        consume(guard);
    }
}

template <class T>
void compile_time_branch() {
    if constexpr (T::ready) {
        return;
    } else {
        T::dependent_work();
    }
}

void hidden_cleanup(bool stop) {
    if (stop) {
        return;
    } else {
#if FEATURE
        Guard guard;
#endif
        proceed();
    }
    after_cleanup();
}

void decision_chain(bool stop, bool ready) {
    if (stop) return;
    else if (ready) return;
    else proceed();
}

void dangling_else(bool stop, bool ready) {
    if (ready)
        if (stop) return;
        else proceed();
}

void conditional_exit(bool stop, bool ready) {
    if (stop) {
        if (ready) return;
    } else {
        proceed();
    }
}
