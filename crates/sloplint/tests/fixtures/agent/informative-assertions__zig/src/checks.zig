const std = @import("std");

pub fn check_ready(ready: bool) void {
    std.debug.assert(ready);
}
