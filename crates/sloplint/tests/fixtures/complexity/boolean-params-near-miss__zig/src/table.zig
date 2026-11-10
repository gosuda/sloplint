pub fn render_table(header: bool) []const u8 {
    if (header) {
        return "h";
    }
    return "";
}
