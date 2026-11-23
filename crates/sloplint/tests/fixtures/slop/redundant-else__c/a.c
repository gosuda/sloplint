void return_early(int stop) {
    if (stop) {
        return;
    } else {
        proceed();
    }
}

void loop_early(int stop) {
    for (;;) {
        if (stop) {
            break;
        } else {
            proceed();
        }
        if (stop) {
            continue;
        } else {
            proceed();
        }
    }
}
