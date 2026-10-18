pub struct Point3 {
    x: i64,
    y: i64,
    z: i64,
}

pub struct Raw3 {
    x: i64,
    y: i64,
    z: i64,
}

pub struct Point4 {
    x: i64,
    y: i64,
    z: i64,
    w: i64,
}

pub fn convert3(p: &Raw3) -> Point3 {
    return Point3 { x: p.x, y: p.y, z: p.z };
}

pub fn convert4(p: &Raw3, extra: i64) -> Point4 {
    return Point4 { x: p.x, y: p.y, z: p.z, w: extra };
}
