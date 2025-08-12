use std::any::Any;

pub fn panic_to_string(panic: Box<dyn Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&'static str>() {
        s.to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}
