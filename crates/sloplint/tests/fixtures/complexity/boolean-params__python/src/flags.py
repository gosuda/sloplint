def render_table(header: bool, footer: bool):
    out = ""
    if header:
        out += "h"
    if footer:
        out += "f"
    return out
