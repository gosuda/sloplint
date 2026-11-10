pub fn f() void {
    compute(
        [_]u32{
            1,
            2,
            3,
            4,
            5,
            6,
            7,
            8,
            9,
            10,
            11,
            12,
            13,
            14,
            15,
        },
    );
}

fn compute(values: []const u32) void {
    _ = values;
}
