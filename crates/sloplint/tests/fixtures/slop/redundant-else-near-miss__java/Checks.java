class Checks {
    void scopedDeclaration(boolean stop) {
        if (stop) {
            return;
        } else {
            int value = read();
            consume(value);
        }
    }

    void patternCondition(Object value) {
        if (!(value instanceof String text)) {
            return;
        } else {
            consume(text);
        }
    }

    void patternInsideElse(boolean stop, Object value) {
        if (stop) {
            return;
        } else {
            if (!(value instanceof String text)) {
                return;
            }
            consume(text);
        }
    }

    void decisionChain(boolean stop, boolean ready) {
        if (stop) return;
        else if (ready) return;
        else proceed();
    }

    void danglingElse(boolean stop, boolean ready) {
        while (ready)
            if (stop) return;
            else proceed();
    }

    void conditionalExit(boolean stop, boolean ready) {
        if (stop) {
            if (ready) return;
        } else {
            proceed();
        }
    }

    void namedCall(boolean stop) {
        if (stop) {
            fatal();
        } else {
            proceed();
        }
    }
}
