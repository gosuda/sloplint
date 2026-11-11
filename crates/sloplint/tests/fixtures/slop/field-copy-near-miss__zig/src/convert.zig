const Point = struct {
    x: u32,
    y: u32,
    w: u32,
};

const Raw = struct {
    x: u32,
    y: u32,
    w: u32,
};

pub fn convert(p: Raw) Point {
    return Point{ .x = p.x, .y = p.y, .w = p.w };
}
