pub fn on_int(code: u32) []const u8 {
    return switch (code) {
        0 => "zero",
        1 => "one",
        else => "many",
    };
}
