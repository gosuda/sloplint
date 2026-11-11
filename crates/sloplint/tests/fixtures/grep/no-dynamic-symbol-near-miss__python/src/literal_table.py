import importlib


def load():
    for mod in ("os", "sys"):
        importlib.import_module(mod)
