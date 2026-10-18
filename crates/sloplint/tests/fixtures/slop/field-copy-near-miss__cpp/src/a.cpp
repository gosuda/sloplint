struct Point { long x; long y; long z; };
struct Raw { long x; long y; long z; };
Point convert(Raw p) { return Point{ .x = p.x, .y = p.y, .z = p.z }; }
