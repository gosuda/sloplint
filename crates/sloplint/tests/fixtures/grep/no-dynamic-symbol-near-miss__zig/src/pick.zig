const Box = struct { x: u32, y: u32 };

pub fn pick(box: Box) u32 {
    return @field(box, "x");
}
