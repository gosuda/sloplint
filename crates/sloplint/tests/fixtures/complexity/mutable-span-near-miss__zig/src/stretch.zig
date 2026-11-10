pub fn stretch_assign(seed: u32) u32 {
    var scratch: u32 = seed;
    scratch = scratch + 1;
    scratch = scratch + 1;
    scratch = scratch + 1;
    return scratch;
}
