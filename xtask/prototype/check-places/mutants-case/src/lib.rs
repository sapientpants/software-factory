#[cfg(target_os = "linux")]
pub fn linux_only(a: i32) -> i32 {
    a + 1
}

pub fn everywhere(a: i32) -> i32 {
    a + 2
}

#[cfg(test)]
mod tests {
    #[test]
    fn everywhere_adds_two() {
        assert_eq!(super::everywhere(1), 3);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_only_adds_one() {
        assert_eq!(super::linux_only(1), 2);
    }
}
