struct Raw {
    int x;
    int y;
    int z;
    int w;
};

struct Point {
    int x;
    int y;
    int z;
    int w;
};

struct Point convert(struct Raw *src) {
    struct Point out = { .x = src->x, .y = src->y, .z = src->z, .w = src->w };
    return out;
}
