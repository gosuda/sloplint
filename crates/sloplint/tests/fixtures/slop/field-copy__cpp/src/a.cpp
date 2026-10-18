struct Point { long x; long y; long z; long w; };
struct Raw { long x; long y; long z; long w; };
Point convert(Raw p) { return Point{ .x = p.x, .y = p.y, .z = p.z, .w = p.w }; }
