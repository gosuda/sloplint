package probe

greet :: proc(name: string, loud: bool) -> string {
    return render_greeting(name, !loud)
}

render_greeting :: proc(name: string, loud: bool) -> string {
    if loud {
        return name
    }
    return name
}
