//! The name and the version of a check tool, both of which become directory
//! names on the host (SPEC WS-7a).

use factory_core::check_tool::{CheckTool, CheckToolError};

#[test]
fn a_name_and_a_version_of_their_forms_make_a_check_tool() {
    for (name, version) in [
        ("typos", "1.52.2"),
        ("a", "0"),
        ("some-tool", "0.9.152"),
        ("x1-2", "v2.0.0-rc.1+build.5"),
        ("tool", "Z"),
    ] {
        let tool = CheckTool::new(name, version).expect("a valid check tool");
        assert_eq!((tool.name(), tool.version()), (name, version));
        assert_eq!(tool.to_string(), format!("{name} {version}"));
    }
}

#[test]
fn a_name_of_another_form_is_refused() {
    for name in [
        "", "Typos", "1typos", "-typos", "ty_pos", "ty.pos", "ty pos", "..", "a/b", "é",
    ] {
        assert_eq!(
            CheckTool::new(name, "1.0.0"),
            Err(CheckToolError::Name(name.to_owned())),
            "{name:?}"
        );
    }
}

#[test]
fn a_version_of_another_form_is_refused() {
    for version in [
        "", ".1", "-1", "+1", "..", "1/2", "1 2", "1_2", "../1", "1é", "*", "^1",
    ] {
        assert_eq!(
            CheckTool::new("typos", version),
            Err(CheckToolError::Version {
                name: "typos".to_owned(),
                version: version.to_owned()
            }),
            "{version:?}"
        );
    }
}

#[test]
fn an_error_says_what_is_wrong() {
    assert_eq!(
        CheckToolError::Name("A".to_owned()).to_string(),
        "the check tool `A` has no name of WS-7a's form: lowercase letters, digits and hyphens, beginning with a letter"
    );
    assert_eq!(
        CheckToolError::Version {
            name: "typos".to_owned(),
            version: "*".to_owned()
        }
        .to_string(),
        "the check tool `typos` has a version `*` of no form of WS-7a: letters, digits, dots, hyphens and `+`, beginning with a letter or a digit"
    );
}
