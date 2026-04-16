use std::collections::BTreeMap;

pub fn normalize(input: &str) -> String {
    input.trim().to_lowercase()
}

pub fn word_counts(input: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();

    for token in input.split_whitespace() {
        let token = token.trim_matches(|ch: char| !ch.is_alphanumeric());
        if token.is_empty() {
            continue;
        }

        let key = token.to_lowercase();
        *counts.entry(key).or_insert(0) += 1;
    }

    counts
}

#[cfg(test)]
mod tests {
    use crate::text::{normalize, word_counts};

    #[test]
    fn lowercases_and_trims() {
        assert_eq!(normalize("  HeLLo  "), "hello");
    }

    #[test]
    fn counts_words_case_insensitively() {
        let counts = word_counts("Rust rust RUST");
        assert_eq!(counts.get("rust"), Some(&3));
    }
}
