def _render_table(header: bool, footer: bool):
    return (header, footer)


class Box:
    def __eq__(self, other):
        return True


def send(stream: bool = False, verify: VerifyType = True):
    return (stream, verify)


def inspect(o, *, help: bool = False):
    return (o, help)
