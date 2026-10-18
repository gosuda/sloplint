fn read_value() !u32 {
    return 1;
}

pub fn load() u32 {
    return read_value() catch 0;
}
