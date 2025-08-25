use once_cell::sync::Lazy;
use regex::Regex;
use rocket::form;

pub fn is_ascii_no_spaces<'v>(username: &str) -> form::Result<'v, ()> {
    match username.chars().all(|c| c.is_ascii() && !c.is_whitespace()) {
        true => Ok(()),
        false => {
            return Err(rocket::form::Error::validation(
                "should be an ascii string without spaces",
            )
            .into());
        }
    }
}

pub fn is_valid_email<'v>(string: &str) -> form::Result<'v, ()> {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
        r#"(?:[a-z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[a-z0-9!#$%&'*+/=?^_`{|}~-]+)*|"(?:[\x01-\x08\x0b\x0c\x0e-\x1f\x21\x23-\x5b\x5d-\x7f]|\\[\x01-\x09\x0b\x0c\x0e-\x7f])*")@(?:(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z0-9](?:[a-z0-9-]*[a-z0-9])?|\[(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?|[a-z0-9-]*[a-z0-9]:(?:[\x01-\x08\x0b\x0c\x0e-\x1f\x21-\x5a\x53-\x7f]|\\[\x01-\x09\x0b\x0c\x0e-\x7f])+)\])"#
    ).unwrap()
    });
    match RE.is_match(string) {
        true => Ok(()),
        false => {
            return Err(rocket::form::Error::validation("invalid email").into());
        }
    }
}

pub fn is_valid_slug<'v>(string: &str) -> form::Result<'v, ()> {
    let cmp = string
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_');

    match cmp {
        true => Ok(()),
        false => return Err(rocket::form::Error::validation("invalid slug").into()),
    }
}

#[cfg(test)]
#[test]
fn test_email() {
    assert!(is_valid_email("hello@example.com").is_ok())
}
