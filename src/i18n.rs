use std::sync::OnceLock;

static SPANISH: OnceLock<bool> = OnceLock::new();

/// Selects the UI language. English remains the fallback for unknown values.
pub fn init(language: &str) {
    let language = language.to_ascii_lowercase();
    let _ = SPANISH.set(matches!(language.as_str(), "es" | "es-es"));
}

pub fn es() -> bool {
    *SPANISH.get_or_init(|| false)
}

pub fn text(en: &'static str, es: &'static str) -> &'static str {
    if self::es() { es } else { en }
}

#[cfg(test)]
mod tests {
    use super::text;

    #[test]
    fn defaults_to_english() {
        assert_eq!(text("English", "Español"), "English");
    }
}
