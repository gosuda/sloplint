const std = @import("std");

pub fn load(path: []const u8) void {
    std.log.info(path, .{});
    std.log.info("oops", .{});
}
