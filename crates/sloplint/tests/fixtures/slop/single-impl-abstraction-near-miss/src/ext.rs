pub trait WidgetExt {
    fn describe(&self) -> String;
}

pub struct Button;

impl WidgetExt for Button {
    fn describe(&self) -> String {
        return "button".to_string();
    }
}
