fn read_value() !u32 {
    return 1;
}

pub fn load() void {
    read_value() catch {};
}
