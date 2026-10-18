struct Raw {
    int x;
    int y;
    int z;
};

struct Point {
    int x;
    int y;
    int z;
};

struct Point convert(struct Raw *src) {
    struct Point out = { .x = src->x, .y = src->y, .z = src->z + 1 };
    return out;
}
