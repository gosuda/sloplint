class Point {
  final int x;
  final int y;
  final int z;
  final int w;
  Point({required this.x, required this.y, required this.z, required this.w});
}

int copyX(Point p) {
  return Point(x: p.x, y: p.y, z: p.z, w: p.w).x;
}
