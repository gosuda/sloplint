const Config = struct {
    name: []const u8,
};

pub fn load_config(path: []const u8) !Config {
    _ = path;
    return Config{ .name = "default" };
}
