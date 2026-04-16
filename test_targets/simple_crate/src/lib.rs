pub fn add_two_numbers(left: i32, right: i32) -> i32 {
    left + right
}

pub fn append_character(mut input: String, extra: char) -> String {
    input.push(extra);
    input
}

pub fn print_message(message: &str) {
    println!("{message}");
}

#[cfg(test)]
mod tests {
    #[test]
    fn adds_numbers() {
        assert_eq!(crate::add_two_numbers(2, 3), 5);
    }

    #[test]
    fn appends_character() {
        assert_eq!(crate::append_character(String::from("ab"), 'c'), "abc");
    }
}
