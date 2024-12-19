use crate::testrun::Framework;
use quick_xml::escape::unescape;
use std::{borrow::Cow, collections::HashSet};

fn compute_pytest_using_filename(classname: &str, name: &str, filename: &str) -> String {
    let path_components = filename.split('/').count();

    let classname_components = classname.split(".");

    let actual_classname = classname_components
        .skip(path_components)
        .collect::<Vec<_>>()
        .join("::");

    if actual_classname.is_empty() {
        format!("{}::{}", filename, name)
    } else {
        format!("{}::{}::{}", filename, actual_classname, name)
    }
}

fn path_from_classname(classname: &[&str]) -> String {
    format!("{}.py", classname.join("/"))
}

fn compute_pytest_using_network(classname: &str, name: &str, network: &HashSet<String>) -> String {
    let classname_components = classname.split(".").collect::<Vec<_>>();
    let mut path_component_count = 0;
    let start = classname_components.len();

    while path_component_count < start {
        let path = path_from_classname(&classname_components[..start - path_component_count]);
        if network.contains(&path) {
            if path_component_count > 0 {
                let actual_classname = classname_components
                    .into_iter()
                    .skip(start - path_component_count)
                    .collect::<Vec<_>>()
                    .join("::");
                return format!("{}::{}::{}", path, actual_classname, name);
            } else {
                return format!("{}::{}", path, name);
            }
        }

        path_component_count += 1;
    }

    format!("{}::{}", classname, name)
}

pub fn unescape_str(s: &str) -> Cow<'_, str> {
    unescape(s).unwrap_or(Cow::Borrowed(s))
}

pub fn compute_name(
    classname: &str,
    name: &str,
    framework: Framework,
    filename: Option<&str>,
    network: Option<&HashSet<String>>,
) -> String {
    let name = unescape_str(name);
    let classname = unescape_str(classname);
    let filename = filename.map(|f| unescape_str(f));

    match framework {
        Framework::Jest => name.to_string(),
        Framework::Pytest => {
            if let Some(filename) = filename {
                compute_pytest_using_filename(&classname, &name, &filename)
            } else if let Some(network) = network {
                compute_pytest_using_network(&classname, &name, network)
            } else {
                format!("{}::{}", classname, name)
            }
        }
        Framework::Vitest => {
            format!("{} > {}", classname, name)
        }
        Framework::PHPUnit => {
            format!("{}::{}", classname, name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_name() {
        assert_eq!(
            compute_name("a.b.c", "d", Framework::Pytest, None, None),
            "a.b.c::d"
        );
    }

    #[test]
    fn test_compute_name_with_filename() {
        assert_eq!(
            compute_name("a.b.c", "d", Framework::Pytest, Some("a/b/c.py"), None),
            "a/b/c.py::d"
        );
    }

    #[test]
    fn test_compute_name_with_filename_classname() {
        assert_eq!(
            compute_name("a.b.c", "d", Framework::Pytest, Some("a/b.py"), None),
            "a/b.py::c::d"
        );
    }

    #[test]
    fn test_compute_name_with_network() {
        let network = ["a/b/c.py"].iter().map(|e| e.to_string()).collect();
        assert_eq!(
            compute_name("a.b.c", "d", Framework::Pytest, None, Some(&network)),
            "a/b/c.py::d"
        );
    }

    #[test]
    fn test_compute_name_with_network_actual_classname() {
        let network = ["a/b.py"].iter().map(|e| e.to_string()).collect();
        assert_eq!(
            compute_name("a.b.c", "d", Framework::Pytest, None, Some(&network)),
            "a/b.py::c::d"
        );
    }

    #[test]
    fn test_compute_name_with_network_actual_classname_no_match() {
        let network = ["d.py"].iter().map(|e| e.to_string()).collect();
        assert_eq!(
            compute_name("a.b.c", "d", Framework::Pytest, None, Some(&network)),
            "a.b.c::d"
        );
    }

    #[test]
    fn test_compute_name_jest() {
        assert_eq!(
            compute_name(
                "it does the thing &gt; it does the thing",
                "it does the thing &gt; it does the thing",
                Framework::Jest,
                None,
                None
            ),
            "it does the thing > it does the thing"
        );
    }

    #[test]
    fn test_compute_name_vitest() {
        assert_eq!(
            compute_name(
                "tests/thing.js",
                "it does the thing &gt; it does the thing",
                Framework::Vitest,
                None,
                None
            ),
            "tests/thing.js > it does the thing > it does the thing"
        );
    }

    #[test]
    fn test_compute_name_phpunit() {
        assert_eq!(
            compute_name("class.className", "test1", Framework::PHPUnit, None, None),
            "class.className::test1"
        );
    }
}
