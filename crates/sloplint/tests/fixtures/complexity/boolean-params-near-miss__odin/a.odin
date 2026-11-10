package probe

render_table :: proc(header: bool, style: string) -> string {
    if header {
        return style
    }
    return "plain"
}
