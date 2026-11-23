class Checks {
    void ScopedResource(bool stop) {
        if (stop) {
            return;
        } else {
            using var resource = Acquire();
            Consume(resource);
        }
        AfterDisposal();
    }

    void ScopedLocalFunction(bool stop) {
        if (stop) {
            return;
        } else {
            void Local() { Proceed(); }
            Local();
        }
    }

    void PatternCondition(object value) {
        if (value is not string text) {
            return;
        } else {
            Consume(text);
        }
    }

    void OutBindingCondition() {
        if (TryRead(out var value)) {
            return;
        } else {
            Consume(value);
        }
    }

    void OutBindingInsideElse(bool stop) {
        if (stop) {
            return;
        } else {
            TryRead(out var value);
            Consume(value);
        }
    }

    void ConditionalChain(bool stop, bool ready) {
        if (stop) {
            return;
        }
#if FEATURE
        else if (ready) {
            return;
        }
#endif
        else {
            Proceed();
        }
    }

    void HiddenDeclaration(bool stop) {
        if (stop) {
            return;
        } else {
#if FEATURE
            using var resource = Acquire();
#endif
            Proceed();
        }
    }

    void DanglingElse(bool stop, bool ready) {
        while (ready)
            if (stop) return;
            else Proceed();
    }
}
