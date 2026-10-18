pub fn greet(name: &str, loud: bool) -> String {
    return render_greeting(name, loud);
}

fn render_greeting(name: &str, loud: bool) -> String {
    if loud {
        return name.to_uppercase();
    }
    return name.to_string();
}
