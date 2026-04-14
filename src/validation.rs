use once_cell::sync::Lazy;
use regex::Regex;
// use rocket::form;

pub fn is_ascii_no_spaces(username: &str) -> Result<(), String> {
    match username.chars().all(|c| c.is_ascii() && !c.is_whitespace()) {
        true => Ok(()),
        false => Err("should be an ascii string without spaces".to_string()),
    }
}

pub fn is_valid_username(username: &str) -> Result<(), String> {
    is_ascii_no_spaces(username)?;

    if username.chars().count() < 3 {
        return Err("Username too short".to_string());
    }

    if username.len() >= 128 {
        return Err("Username too long".to_string());
    }

    Ok(())
}

pub fn is_valid_email(string: &str) -> Result<(), String> {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
        r#"(?:[a-z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[a-z0-9!#$%&'*+/=?^_`{|}~-]+)*|"(?:[\x01-\x08\x0b\x0c\x0e-\x1f\x21\x23-\x5b\x5d-\x7f]|\\[\x01-\x09\x0b\x0c\x0e-\x7f])*")@(?:(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z0-9](?:[a-z0-9-]*[a-z0-9])?|\[(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?|[a-z0-9-]*[a-z0-9]:(?:[\x01-\x08\x0b\x0c\x0e-\x1f\x21-\x5a\x53-\x7f]|\\[\x01-\x09\x0b\x0c\x0e-\x7f])+)\])"#
    ).unwrap()
    });
    match RE.is_match(string) {
        true => Ok(()),
        false => Err("invalid email".to_string()),
    }
}

pub fn is_valid_slug(string: &str) -> Result<(), String> {
    let cmp = string
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_');

    match cmp {
        true => Ok(()),
        false => Err("invalid slug".to_string()),
    }
}

#[cfg(test)]
#[test]
fn test_email() {
    assert!(is_valid_email("hello@example.com").is_ok())
}

#[cfg(test)]
#[test]
fn test_username() {
    assert_eq!(is_valid_username("").unwrap_err(), "Username too short");
    assert_eq!(is_valid_username("ab").unwrap_err(), "Username too short");
    assert!(is_valid_username("abc").is_ok());
    assert_eq!(
        is_valid_username("hello world").unwrap_err(),
        "should be an ascii string without spaces"
    );
    assert_eq!(
        is_valid_username(&"a".repeat(128)).unwrap_err(),
        "Username too long"
    );
}
