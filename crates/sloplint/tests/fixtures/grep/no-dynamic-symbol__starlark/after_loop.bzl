def h(obj):
    for attr in ["alpha", "beta"]:
        getattr(obj, attr)
    return getattr(obj, attr)
