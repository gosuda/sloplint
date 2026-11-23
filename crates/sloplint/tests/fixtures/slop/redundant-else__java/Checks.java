class Checks {
    void returnEarly(boolean stop) {
        if (stop) {
            return;
        } else {
            proceed();
        }
    }

    void rejectEarly(boolean stop) {
        if (stop) {
            throw new IllegalArgumentException("invalid state");
        } else {
            proceed();
        }
    }

    void loopEarly(boolean stop) {
        while (true) {
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
}
