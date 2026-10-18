const Status = enum {
    active,
    idle,
    closed,
};

pub fn describe(status: Status) []const u8 {
    return switch (status) {
        .active => "active",
        .idle => "idle",
        else => "other",
    };
}
