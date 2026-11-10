def confirm(abort: bool = False, err: bool = False):
    out = ""
    if abort:
        out += "a"
    if err:
        out += "e"
    return out
