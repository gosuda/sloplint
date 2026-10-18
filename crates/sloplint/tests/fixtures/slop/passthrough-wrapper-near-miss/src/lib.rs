pub struct Stack {
    inner: Vec<i64>,
}

impl Stack {
    pub fn len(&self) -> usize {
        return self.inner.len();
    }

    pub fn move_to(x: i64, y: i64) -> i64 {
        return place(y, x);
    }
}

fn place(x: i64, y: i64) -> i64 {
    return x + y;
}

pub fn fresh() -> Vec<i64> {
    return Vec::new();
}
