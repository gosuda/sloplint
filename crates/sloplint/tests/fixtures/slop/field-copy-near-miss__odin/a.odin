package probe

Point :: struct { x: int, y: int, w: int }

move_point :: proc(p: Point) -> Point {
    return Point{ x = p.x, y = p.y, w = p.w + 1 }
}
