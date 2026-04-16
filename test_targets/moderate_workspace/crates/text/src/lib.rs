use std::collections::BTreeSet;

pub fn collect_unique_words(input: &str) -> BTreeSet<String> {
    input
        .split_whitespace()
        .map(|word| word.to_lowercase())
        .collect()
}

pub fn first_line(input: &str) -> &str {
    input.lines().next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    #[test]
    fn collects_unique_words_case_insensitively() {
        let words = crate::collect_unique_words("Alpha beta ALPHA");
        assert_eq!(words.len(), 2);
    }
}
