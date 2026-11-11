const Box = struct { x: u32, y: u32 };

pub fn pick(box: Box, field: []const u8) u32 {
    _ = field;
    return @field(box, field);
}
