void cleanup(int step);

void flush(bool failed) {
    ON_SCOPE_EXIT() {
        int retries = 0;
        if (failed) goto done;
        ++retries;
        cleanup(1);
        cleanup(2);
        cleanup(3);
        cleanup(4);
        cleanup(5);
        cleanup(6);
        cleanup(7);
        cleanup(8);
        cleanup(9);
        cleanup(10);
        cleanup(11);
        cleanup(12);
        cleanup(13);
        cleanup(14);
        done: int result = retries;
        ++retries;
        cleanup(result + retries);
    };
}

void class_body() {
    LOCAL_STATE(State) {
        private:
        int retries = 0;
        public:
        void retry() {
            ++retries;
            cleanup(1);
            cleanup(2);
            cleanup(3);
            cleanup(4);
            cleanup(5);
            cleanup(6);
            cleanup(7);
            cleanup(8);
            cleanup(9);
            cleanup(10);
            cleanup(11);
            cleanup(12);
            cleanup(13);
            cleanup(14);
            ++retries;
        }
    };
}
