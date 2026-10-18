const std = @import("std");

pub fn load(path: []const u8) void {
    std.log.info("failed to open file", .{});
    _ = path;
}
