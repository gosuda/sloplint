class Point:
    def __init__(self, x, y, z, w, v, u):
        self.x = x
        self.y = y
        self.z = z
        self.w = w
        self.v = v
        self.u = u


def convert(s):
    return Point(x=s.x, y=s.y, z=s.z, w=s.w, v=s.v, u=s.u)
