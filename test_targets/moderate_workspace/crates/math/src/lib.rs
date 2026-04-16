pub fn sum(values: &[i64]) -> i64 {
    values.iter().copied().sum()
}

pub fn mean(values: &[i64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    let total = sum(values);
    Some(total as f64 / values.len() as f64)
}

#[cfg(test)]
mod tests {
    #[test]
    fn computes_mean() {
        let mean = crate::mean(&[2, 4, 6]).expect("mean should exist for non-empty slice");
        assert_eq!(mean, 4.0);
    }
}
