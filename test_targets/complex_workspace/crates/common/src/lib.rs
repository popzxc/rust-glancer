#[macro_export]
macro_rules! status_line {
    ($phase:expr, $value:expr) => {
        format!("phase={} value={}", $phase, $value)
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItem {
    pub id: u32,
    pub label: String,
}

impl WorkItem {
    pub fn new(id: u32, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn macro_formats_values() {
        let line = crate::status_line!("ready", 4);
        assert_eq!(line, "phase=ready value=4");
    }
}
