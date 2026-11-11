import importlib


class Lazy:
    def __getattr__(self, name):
        return getattr(self._fallback, name)


def load():
    for lib in ("os", "sys"):
        importlib.import_module(lib)
