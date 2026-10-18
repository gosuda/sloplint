pub struct Point {
    x: i64,
    y: i64,
    z: i64,
    w: i64,
}

pub struct Raw {
    x: i64,
    y: i64,
    z: i64,
    w: i64,
}

pub fn convert(p: &Raw) -> Point {
    return Point { x: p.x, y: p.y, z: p.z, w: p.w };
}
