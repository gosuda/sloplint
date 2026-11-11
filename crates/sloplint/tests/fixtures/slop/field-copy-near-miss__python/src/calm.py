class Service:
    def prepare(self, method=None):
        return method


def convert_via_method(s, p):
    return p.prepare(a=s.a, b=s.b, c=s.c, d=s.d)


def convert_mapping(s):
    return dict(x=s.x, y=s.y, z=s.z, w=s.w)


class Builder:
    def build(self, **params):
        return type(self)(**params)
