//! Validation shared by saved libraries and command inputs.
pub const MAX_LABEL_CHARS: usize = 200;

pub fn crossfade(value: &serde_json::Value) -> bool {
    value
        .as_f64()
        .is_some_and(|value| value.is_finite() && (0.0..=30000.0).contains(&value))
}

pub fn identifier(value: &str, kind: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.chars().count() > MAX_LABEL_CHARS
        || value.chars().any(|c| c.is_control() || c == '|')
    {
        return Err(format!(
            "{kind} must contain 1–200 characters without control characters or |"
        ));
    }
    Ok(())
}
pub fn name(value: &str, kind: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.chars().count() > MAX_LABEL_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(format!(
            "{kind} must contain 1–200 characters without control characters"
        ));
    }
    Ok(())
}
pub fn color(value: &str, optional: bool) -> bool {
    let bytes = value.as_bytes();
    optional && value.is_empty()
        || bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(u8::is_ascii_hexdigit)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifiers_cannot_collide_with_grant_or_group_separators() {
        for value in ["", " ", "a\0b", "a\nb", "a|b"] {
            assert!(identifier(value, "Identifier").is_err());
        }
        assert!(identifier(&"x".repeat(201), "Identifier").is_err());
        assert!(identifier(&"界".repeat(200), "Identifier").is_ok());
        assert!(name(&"界".repeat(200), "Name").is_ok());
        assert!(name(&"界".repeat(201), "Name").is_err());
        assert!(name("New\nstation", "Name").is_err());
    }
    #[test]
    fn colors_are_hex_and_empty_only_when_optional() {
        for value in ["#123456", "#AbCdEf"] {
            assert!(color(value, false));
        }
        for value in [
            "red",
            "#fff",
            "#12345g",
            "＃123456",
            "#123456;background:red",
        ] {
            assert!(!color(value, true));
        }
        assert!(color("", true));
        assert!(!color("", false));
    }
}
