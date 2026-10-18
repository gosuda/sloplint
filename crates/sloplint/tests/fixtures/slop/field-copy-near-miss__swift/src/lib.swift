public struct Point {
    public var x: Int
    public var y: Int
    public var z: Int
    public init(x: Int, y: Int, z: Int) {
        self.x = x
        self.y = y
        self.z = z
    }
}

public func convert(_ p: Point) -> Point {
    return Point(x: p.x, y: p.y, z: p.z)
}
