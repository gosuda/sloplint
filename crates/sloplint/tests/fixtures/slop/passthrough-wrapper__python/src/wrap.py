def greet(name, loud):
    return render_greeting(name, loud)


def render_greeting(name, loud):
    if loud:
        return name.upper()
    return name
