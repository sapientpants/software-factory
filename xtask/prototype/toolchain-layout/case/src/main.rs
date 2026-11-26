fn main() {
    println!("ok");
}

#[cfg(test)]
mod tests {
    #[test]
    fn one() {
        assert_eq!(1 + 1, 2);
    }
}
