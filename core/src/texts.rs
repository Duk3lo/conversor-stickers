pub fn decimal(value: f64) -> String {
    value.to_string().replace('.', ",")
}

pub fn decimal_1(value: f64) -> String {
    format!("{value:.1}").replace('.', ",")
}

pub fn readable_size(bytes: u64) -> String {
    if bytes < 1_000_000 {
        format!("{} KB", bytes.div_ceil(1000))
    } else {
        format!("{} MB", decimal(bytes.div_ceil(100_000) as f64 / 10.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_write_with_comma() {
        assert_eq!(decimal(0.5), "0,5");
        assert_eq!(decimal(10.0), "10");
        assert_eq!(decimal(-1.25), "-1,25");
        assert_eq!(decimal_1(20.0), "20,0");
        assert_eq!(decimal_1(1.25), "1,2");
    }

    #[test]
    fn sizes_write_in_kb_or_mb_towards_above() {
        assert_eq!(readable_size(0), "0 KB");
        assert_eq!(readable_size(500_000), "500 KB");
        assert_eq!(readable_size(612_001), "613 KB");
        assert_eq!(readable_size(10_000_000), "10 MB");
        assert_eq!(readable_size(1_520_000), "1,6 MB");
    }
}
