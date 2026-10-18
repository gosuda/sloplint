def convert3(p):
    return Point3(x=p.x, y=p.y, z=p.z)


def convert4(p, extra):
    return Point4(x=p.x, y=p.y, z=p.z, w=extra + 1)
