pub fn render(name: []const u8) []const u8 {
    return name;
}

test "render returns its argument" {
    if (render("x").len != 1) {
        unreachable;
    }
}
