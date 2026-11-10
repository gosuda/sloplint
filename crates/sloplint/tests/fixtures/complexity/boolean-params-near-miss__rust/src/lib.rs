#[cfg(test)]
mod tests {
    pub fn either(left: bool, right: bool) -> bool {
        left || right
    }

    pub fn both(left: bool, right: bool) -> bool {
        left && right
    }
}

#[bench]
pub fn bench_either(enabled: bool) -> bool {
    enabled
}

pub(crate) fn file(f: bool) -> bool {
    f
}
