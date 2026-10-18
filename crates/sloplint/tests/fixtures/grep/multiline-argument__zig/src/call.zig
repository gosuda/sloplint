pub fn f() void {
    compute(
        [_]u32{
            1,
            2,
            3,
            4,
        },
    );
}

fn compute(values: []const u32) void {
    _ = values;
}
