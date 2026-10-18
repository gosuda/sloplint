struct Point {
    int x;
    int y;
    int z;
};

struct Point CopyPoint(struct Point s) {
    struct Point p = { .x = s.x, .y = s.y, .z = s.z };
    return p;
}
