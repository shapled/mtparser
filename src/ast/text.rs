use std::fmt;

/// A segment within interpolated text: either a literal string or a variable reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextPart {
    /// Literal text (including escaped `\$` → `$`, and bare `$` before non-identifier chars).
    Literal(String),
    /// A variable reference. Stores the variable name without the `$` prefix.
    Variable(String),
}

/// Text that may contain `$variable_name` references.
///
/// Preserves structure for downstream tools that need to reason about variable usage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpolatedText(Vec<TextPart>);

impl InterpolatedText {
    /// Parse a raw string into an InterpolatedText, scanning for `$variable` references.
    ///
    /// Scanning rules:
    /// - `\$` → `Literal("$")`
    /// - `$` followed by `[a-zA-Z_]` then `[a-zA-Z0-9_-]*` → `Variable(name)`
    /// - `$` followed by anything else → `Literal("$")`
    /// - all other chars → accumulated into `Literal`
    pub fn parse(s: &str) -> Self {
        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut chars = s.char_indices().peekable();

        while let Some(&(_, ch)) = chars.peek() {
            if ch == '\\' {
                // Check for \$
                chars.next(); // consume '\'
                if chars.peek().map(|&(_, c)| c) == Some('$') {
                    chars.next(); // consume '$'
                    literal.push('$');
                } else {
                    literal.push('\\');
                }
            } else if ch == '$' {
                chars.next(); // consume '$'
                if let Some(&(_, next_ch)) = chars.peek() {
                    if next_ch.is_ascii_alphabetic() || next_ch == '_' {
                        // Start of a variable name
                        if !literal.is_empty() {
                            parts.push(TextPart::Literal(std::mem::take(&mut literal)));
                        }
                        let mut name = String::new();
                        while let Some(&(_, c)) = chars.peek() {
                            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                                name.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        parts.push(TextPart::Variable(name));
                    } else {
                        // `$` followed by non-identifier char → literal `$`
                        literal.push('$');
                    }
                } else {
                    // `$` at end of string
                    literal.push('$');
                }
            } else {
                // Regular character — advance by byte offset to handle multi-byte chars
                chars.next();
                literal.push(ch);
            }
        }

        if !literal.is_empty() {
            parts.push(TextPart::Literal(literal));
        }

        // Optimization: single literal → store as single-element vec
        // (could use a dedicated SingleLiteral variant, but keeping it simple)
        InterpolatedText(parts)
    }

    /// Get the raw string representation (variable references rendered with `$` prefix).
    pub fn to_raw_string(&self) -> String {
        let mut s = String::new();
        for part in &self.0 {
            match part {
                TextPart::Literal(lit) => s.push_str(lit),
                TextPart::Variable(name) => {
                    s.push('$');
                    s.push_str(name);
                }
            }
        }
        s
    }

    /// Return true if the text contains no variable references.
    pub fn is_literal(&self) -> bool {
        self.0.iter().all(|p| matches!(p, TextPart::Literal(_)))
    }

    /// Access the parts slice.
    pub fn parts(&self) -> &[TextPart] {
        &self.0
    }

    /// Collect all variable names referenced in this text.
    pub fn variable_names(&self) -> Vec<&str> {
        self.0
            .iter()
            .filter_map(|p| match p {
                TextPart::Variable(name) => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }
}

impl fmt::Display for TextPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextPart::Literal(s) => f.write_str(s),
            TextPart::Variable(name) => write!(f, "${name}"),
        }
    }
}

impl fmt::Display for InterpolatedText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for part in &self.0 {
            write!(f, "{part}")?;
        }
        Ok(())
    }
}

impl From<&str> for InterpolatedText {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for InterpolatedText {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<InterpolatedText> for String {
    fn from(t: InterpolatedText) -> String {
        t.to_raw_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pure_literal() {
        let t = InterpolatedText::parse("hello world");
        assert!(t.is_literal());
        assert_eq!(t.to_raw_string(), "hello world");
        assert_eq!(t.parts().len(), 1);
    }

    #[test]
    fn test_single_variable() {
        let t = InterpolatedText::parse("$mysql_errno");
        assert!(!t.is_literal());
        assert_eq!(t.to_raw_string(), "$mysql_errno");
        assert_eq!(t.variable_names(), vec!["mysql_errno"]);
    }

    #[test]
    fn test_mixed() {
        let t = InterpolatedText::parse("counter is $counter");
        assert!(!t.is_literal());
        assert_eq!(t.to_raw_string(), "counter is $counter");
        assert_eq!(t.parts().len(), 2);
        assert!(matches!(&t.parts()[0], TextPart::Literal(s) if s == "counter is "));
        assert!(matches!(&t.parts()[1], TextPart::Variable(v) if v == "counter"));
    }

    #[test]
    fn test_escaped_dollar() {
        let t = InterpolatedText::parse(r"price is \$10");
        assert!(t.is_literal());
        assert_eq!(t.to_raw_string(), "price is $10");
    }

    #[test]
    fn test_bare_dollar_non_ident() {
        let t = InterpolatedText::parse("price is $10");
        assert!(t.is_literal());
        assert_eq!(t.to_raw_string(), "price is $10");
    }

    #[test]
    fn test_dollar_at_end() {
        let t = InterpolatedText::parse("total$");
        assert!(t.is_literal());
        assert_eq!(t.to_raw_string(), "total$");
    }

    #[test]
    fn test_multiple_variables() {
        let t = InterpolatedText::parse("$host:$port/$db");
        assert!(!t.is_literal());
        assert_eq!(t.variable_names(), vec!["host", "port", "db"]);
        assert_eq!(t.to_raw_string(), "$host:$port/$db");
    }

    #[test]
    fn test_variable_with_dash() {
        let t = InterpolatedText::parse("$MYSQL_TMP_DIR/path");
        assert!(!t.is_literal());
        assert_eq!(t.variable_names(), vec!["MYSQL_TMP_DIR"]);
    }

    #[test]
    fn test_variable_with_underscore() {
        let t = InterpolatedText::parse("$my_var_name");
        assert_eq!(t.variable_names(), vec!["my_var_name"]);
    }

    #[test]
    fn test_empty_string() {
        let t = InterpolatedText::parse("");
        assert!(t.is_literal());
        assert_eq!(t.to_raw_string(), "");
    }

    #[test]
    fn test_display_trait() {
        let t = InterpolatedText::parse("hello $world");
        assert_eq!(format!("{t}"), "hello $world");
    }

    #[test]
    fn test_from_str() {
        let t: InterpolatedText = "test $var".into();
        assert!(!t.is_literal());
        assert_eq!(t.variable_names(), vec!["var"]);
    }
}
