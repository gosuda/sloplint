class Point3 {
  final int x;
  final int y;
  final int z;
  Point3({required this.x, required this.y, required this.z});
}

class Point4 {
  final int x;
  final int y;
  final int z;
  final int w;
  Point4({required this.x, required this.y, required this.z, required this.w});
}

int convert3(Point3 p) {
  return Point3(x: p.x, y: p.y, z: p.z).x;
}

int convert4(Point3 p, int extra) {
  return Point4(x: p.x, y: p.y, z: p.z, w: extra).x;
}
