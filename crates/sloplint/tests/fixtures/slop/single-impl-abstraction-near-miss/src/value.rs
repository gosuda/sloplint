pub trait ValueEnum {
    fn value(&self) -> i32;
}

pub struct Single;

impl ValueEnum for Single {
    fn value(&self) -> i32 {
        return 1;
    }
}
