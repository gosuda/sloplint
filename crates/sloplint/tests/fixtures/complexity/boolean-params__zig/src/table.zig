pub fn render_table(header: bool, footer: bool) []const u8 {
    if (header) {
        return "h";
    }
    if (footer) {
        return "f";
    }
    return "";
}
