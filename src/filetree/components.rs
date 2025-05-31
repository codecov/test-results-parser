/// Yields path components in reverse order.
///
/// This also takes care to normalize `.` and `..` components,
/// and it skips any trailing `..`.
pub fn reverse_components(path: &str) -> impl Iterator<Item = &str> {
    let mut skip = 0;
    let mut components = path.split('/').rev();

    std::iter::from_fn(move || {
        while let Some(next) = components.next() {
            match next {
                "." => continue,
                ".." => {
                    skip += 1;
                    continue;
                }
                _ if skip > 0 => {
                    skip -= 1;
                    continue;
                }
                component => return Some(component),
            }
        }
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reverse_components() {
        let components: Vec<_> = reverse_components("mod.rs").collect();
        assert_eq!(components, &["mod.rs"]);

        let components: Vec<_> = reverse_components("./foo/./bar/mod.rs").collect();
        assert_eq!(components, &["mod.rs", "bar", "foo"]);

        let components: Vec<_> = reverse_components("./foo/../bar/mod.rs").collect();
        assert_eq!(components, &["mod.rs", "bar"]);

        let components: Vec<_> = reverse_components("foo/../bar/../mod.rs").collect();
        assert_eq!(components, &["mod.rs"]);

        let components: Vec<_> = reverse_components("foo/bar/foobar/../../mod.rs").collect();
        assert_eq!(components, &["mod.rs", "foo"]);
    }
}
