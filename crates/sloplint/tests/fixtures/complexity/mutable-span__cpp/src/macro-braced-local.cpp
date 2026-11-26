void cleanup(int step);

void flush(int limit) {
#ifdef DEBUG
    ON_SCOPE_EXIT {
        int retries = 0;
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
        cleanup(retries + limit);
    };
#endif
}

void drain(int limit) {
    ON_SCOPE_EXIT() {
        int retries = 0;
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
        cleanup(retries + limit);
    };
}
