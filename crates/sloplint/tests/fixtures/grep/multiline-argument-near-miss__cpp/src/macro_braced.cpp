int compute(std::initializer_list<int> xs);
int other(std::initializer_list<int> xs);
// Guarded braced macro member: the collection arg is the field
// declaration's default_value — the call still initializes a binding,
// so its long argument stays silent.
#ifdef FLAG
WIDGET() {
    int v = compute({
        1,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11,
        12,
        13,
        14,
        15,
    }),
        // A second declarator's initializer is still a default_value:
        // `other`'s long argument initializes `w`, so it stays silent.
        w = other({
            16,
            17,
            18,
            19,
            20,
            21,
            22,
            23,
            24,
            25,
            26,
            27,
            28,
            29,
            30,
        });
};
#endif
