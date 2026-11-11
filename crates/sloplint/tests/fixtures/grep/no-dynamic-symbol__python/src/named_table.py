import importlib

TABLE = ("os", "sys")


def load():
    for mod in TABLE:
        importlib.import_module(mod)
