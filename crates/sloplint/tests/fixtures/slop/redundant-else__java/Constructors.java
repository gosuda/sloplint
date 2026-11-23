class Constructors {
    Constructors(boolean stop) {
        super();
        if (stop) {
            return;
        } else {
            proceed();
        }
    }

    Constructors(boolean stop, Object value) {
        if (stop) {
            throw new IllegalArgumentException();
        } else {
            consume(value);
        }
    }
}
