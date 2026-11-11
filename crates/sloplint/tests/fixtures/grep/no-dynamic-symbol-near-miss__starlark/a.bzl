def f(obj):
    return getattr(obj, "field_name")


def g(obj):
    out = []
    for attr in ["alpha", "beta"]:
        out.append(getattr(obj, attr))
    return out
