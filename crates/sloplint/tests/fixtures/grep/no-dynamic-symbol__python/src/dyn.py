import ssl


def get_name(candidate):
    return getattr(ssl, candidate)
