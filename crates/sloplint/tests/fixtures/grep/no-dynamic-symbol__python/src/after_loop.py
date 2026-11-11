import importlib

LIBS = ("os", "sys")


def load():
    for lib in LIBS:
        importlib.import_module(lib)
    return importlib.import_module(lib)
