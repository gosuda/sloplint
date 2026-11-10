package probe

render_table :: proc(header: bool, footer: bool) -> string {
    out := ""
    if header {
        out = "h"
    }
    if footer {
        out = "f"
    }
    return out
}
