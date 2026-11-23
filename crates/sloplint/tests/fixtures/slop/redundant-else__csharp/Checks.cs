class Checks {
    void ReturnEarly(bool stop) {
        if (stop) {
            return;
        } else {
            Proceed();
        }
    }

    void RejectEarly(bool stop) {
        if (stop) {
            throw new System.InvalidOperationException("invalid state");
        } else {
            Proceed();
        }
    }

    void NullGuard(object value) {
        if (value is null) {
            return;
        } else {
            Consume(value);
        }
    }
}
