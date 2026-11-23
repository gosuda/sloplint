void scoped_declaration(int stop) {
    int value = 1;
    if (stop) {
        return;
    } else {
        int value = 2;
        consume(value);
    }
    consume(value);
}

void conditional_exit(int stop, int ready) {
    if (stop) {
        if (ready) return;
    } else {
        proceed();
    }
}

void decision_chain(int stop, int ready) {
    if (stop) return;
    else if (ready) return;
    else proceed();
}

void dangling_else(int stop, int ready) {
    while (ready)
        if (stop) return;
        else proceed();
}

void hidden_declaration(int stop) {
    if (stop) {
        return;
    } else {
#if FEATURE
        int value = read();
        consume(value);
#endif
        proceed();
    }
}

void conditional_directive_exit(int stop) {
    if (stop) {
#if FEATURE
        return;
#endif
    } else {
        proceed();
    }
}

void named_call(int stop) {
    if (stop) {
        fatal();
    } else {
        proceed();
    }
}
