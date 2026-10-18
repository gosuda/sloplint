enum Color {
    RED,
    GREEN,
    BLUE
};

int describe(enum Color c) {
    switch (c) {
    case RED:
        return 1;
    case GREEN:
        return 2;
    default:
        return 0;
    }
}
