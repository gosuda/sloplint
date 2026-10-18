def load(path):
    try:
        return open(path).read()
    except OSError:
        pass
