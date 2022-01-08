pub trait ContextMenu {
    fn get_values(&self) -> Vec<&str>;
}

/// Example implementation of Context Menu
pub struct ExampleMenu {}

impl ContextMenu for ExampleMenu {
    fn get_values(&self) -> Vec<&str> {
        vec!["one", "two", "three", "four", "five", "six"]
    }
}

impl ExampleMenu {
    /// Creates new instance of Example Menu
    pub fn new() -> Self {
        ExampleMenu {}
    }
}
