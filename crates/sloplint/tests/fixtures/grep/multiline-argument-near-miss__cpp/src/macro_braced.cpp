int compute(std::initializer_list<int> xs);
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
    });
};
#endif
