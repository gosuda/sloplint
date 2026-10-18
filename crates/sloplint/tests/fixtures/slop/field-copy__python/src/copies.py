class Point:
    def __init__(self, x, y, z, w):
        self.x = x
        self.y = y
        self.z = z
        self.w = w


def convert(s):
    return Point(x=s.x, y=s.y, z=s.z, w=s.w)
