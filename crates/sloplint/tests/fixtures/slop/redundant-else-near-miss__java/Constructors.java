class Constructors {
    Constructors(boolean stop) {
        if (stop) {
            return;
        } else {
            int local = read();
            consume(local);
        }
    }

    Constructors(Object value) {
        if (!(value instanceof String text)) {
            return;
        } else {
            consume(text);
        }
    }

    Constructors(boolean stop, boolean ready) {
        while (ready)
            if (stop) return;
            else proceed();
    }

    Constructors(boolean stop, Object value) {
        if (stop) {
            if (value == null) return;
        } else {
            consume(value);
        }
    }
}
