use std::collections::BTreeMap;

use components::reverse_components;

mod components;

#[derive(Default, Debug)]
struct Node {
    full_paths: Vec<String>,
    children: BTreeMap<String, Node>,
}

#[derive(Default, Debug)]
pub struct ReverseFileTree {
    root: Node,
}

impl ReverseFileTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: &str) {
        let mut node = &mut self.root;

        for component in reverse_components(path) {
            node = node.children.entry(component.into()).or_default();
        }

        node.full_paths.push(path.into());
    }

    fn lookup(&self, path: &str, min_matches: Option<usize>) -> Vec<String> {
        let mut matching_components = 0;
        let mut components = reverse_components(path);
        let mut node = &self.root;
        let mut last_matching_paths = &vec![];

        for component in &mut components {
            match node.children.get(component) {
                Some(child) => {
                    matching_components += 1;
                    node = child;
                    if !node.full_paths.is_empty() {
                        last_matching_paths = &node.full_paths;
                    }
                }
                None => break,
            }
        }

        let mut results = last_matching_paths.clone();
        if matching_components >= min_matches.map_or(1, |n| n + 1) {
            // we have exhausted all the path components, but the tree might still have more children
            // so we follow a straight branch down the tree if one exists, and extend the results with whatever we find
            while node.children.len() == 1 {
                node = node.children.first_key_value().unwrap().1;
                if !node.full_paths.is_empty() {
                    results.extend_from_slice(&node.full_paths);
                    return results;
                }
            }
        }

        results
    }
}

impl<T> FromIterator<T> for ReverseFileTree
where
    T: AsRef<str>,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut tree = Self::new();
        for path in iter {
            tree.insert(path.as_ref());
        }
        tree
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_min_matches() {
        let tree = ReverseFileTree::from_iter(&["x/y/z"]);

        let cases: &[(usize, &str, &[&str])] = &[
            // only the basename has to match
            (0, "z", &["x/y/z"]),
            (0, "R/z", &["x/y/z"]),
            (0, "R/y/z", &["x/y/z"]),
            (0, "x/y/z", &["x/y/z"]),
            (0, "w/x/y/z", &["x/y/z"]),
            // basename + one ancestor have to match
            (1, "z", &[]),
            (1, "R/z", &[]),
            (1, "R/y/z", &["x/y/z"]),
            (1, "x/y/z", &["x/y/z"]),
            (1, "w/x/y/z", &["x/y/z"]),
            // 3 components have to match
            (2, "z", &[]),
            (2, "R/z", &[]),
            (2, "R/y/z", &[]),
            (2, "x/y/z", &["x/y/z"]),
            (2, "w/x/y/z", &["x/y/z"]),
        ];
        for &(min_matches, lookup, result) in cases {
            assert_eq!(tree.lookup(lookup, Some(min_matches)), result);
        }
    }

    #[test]
    fn test_lookup() {
        let tree = ReverseFileTree::from_iter(&["mod.rs"]);
        // exact lookup
        assert_eq!(tree.lookup("mod.rs", None), &["mod.rs"]);
        // no match
        assert!(tree.lookup("not-found", None).is_empty());

        let tree = ReverseFileTree::from_iter(&["foo/bar/mod.rs"]);

        // the tree will follow unambiguous paths:
        assert_eq!(tree.lookup("bar/mod.rs", None), &["foo/bar/mod.rs"]);
        // it will also follow unambiguous partial matches:
        assert_eq!(tree.lookup("qux/baz/bar/mod.rs", None), &["foo/bar/mod.rs"]);
    }
}
