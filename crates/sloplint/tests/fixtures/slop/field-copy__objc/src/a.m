struct Point {
    int x;
    int y;
    int z;
    int w;
};

struct Point CopyPoint(struct Point s) {
    struct Point p = { .x = s.x, .y = s.y, .z = s.z, .w = s.w };
    return p;
}
