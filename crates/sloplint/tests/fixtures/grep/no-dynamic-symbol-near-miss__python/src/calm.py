import importlib


class Lazy:
    def __getattr__(self, name):
        return getattr(self._fallback, name)


LIBS = ("os", "sys")


def load():
    for lib in LIBS:
        importlib.import_module(lib)
