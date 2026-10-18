package probe

repeat_block :: proc(seed: int) -> int {
    accum := seed + 11
    accum += 22
    accum += 33
    accum += 44
    accum += 55
    accum += 66
    result_one := accum
    accum := seed + 11
    accum += 22
    accum += 33
    accum += 44
    accum += 55
    accum += 66
    result_two := accum
    return accum
}
