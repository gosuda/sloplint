try:
    import optional_dep
    HAS_OPT = True
except ImportError:
    HAS_OPT = False


class Resource:
    def __del__(self):
        try:
            self.close()
        except OSError:
            pass
