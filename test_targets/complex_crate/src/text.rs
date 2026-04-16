use std::borrow::Cow;

pub fn trim_and_lower(input: &str) -> String {
    input.trim().to_lowercase()
}

pub fn first_token(input: &str) -> Cow<'_, str> {
    match input.split_whitespace().next() {
        Some(token) => Cow::Borrowed(token),
        None => Cow::Borrowed("<empty>"),
    }
}

#[cfg(test)]
mod tests {
    use crate::text::{first_token, trim_and_lower};

    mod normalization {
        use crate::text::trim_and_lower;

        #[test]
        fn removes_noise() {
            assert_eq!(trim_and_lower("  HelLo  "), "hello");
        }
    }

    #[test]
    fn returns_empty_marker_when_missing_token() {
        assert_eq!(first_token("   "), "<empty>");
        assert_eq!(trim_and_lower("  X "), "x");
    }
}
