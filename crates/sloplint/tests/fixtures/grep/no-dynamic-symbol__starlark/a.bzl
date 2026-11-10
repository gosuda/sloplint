def f(obj, name):
    return getattr(obj, name)


def g(cfg, k):
    return getattr(cfg, f"opt_{k}")
