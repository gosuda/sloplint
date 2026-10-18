pub trait Renderer {
    fn render(&self, verbose: bool);
}

pub struct Console;

impl Renderer for Console {
    fn render(&self, verbose: bool) {
        let _ = verbose;
    }
}

fn helper(flag: bool) -> bool {
    !flag
}
