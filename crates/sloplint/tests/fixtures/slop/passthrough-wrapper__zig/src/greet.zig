pub fn greet(name: []const u8, loud: bool) []const u8 {
    return render_greeting(name, loud);
}

fn render_greeting(name: []const u8, loud: bool) []const u8 {
    if (loud) {
        return name;
    }
    return name;
}
