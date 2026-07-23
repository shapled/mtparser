use std::fmt;

use crate::ast::Span;

/// A segment within interpolated text: either a literal string or a variable reference.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum TextPart {
    /// Literal text (including escaped `\$` → `$`, and bare `$` before non-identifier chars).
    Literal { text: String, span: Span },
    /// A variable reference. Stores the variable name without the `$` prefix.
    /// Span covers the full `$name` token (including the `$`).
    Variable { name: String, span: Span },
}

/// Text that may contain `$variable_name` references.
///
/// Preserves structure for downstream tools that need to reason about variable usage.
/// `base_offset` is the byte offset of this text within the original source, used to
/// compute correct spans for each TextPart.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct InterpolatedText(Vec<TextPart>);

impl InterpolatedText {
    /// Parse a raw string into an InterpolatedText, scanning for `$variable` references.
    ///
    /// Spans are relative to the input string (offset 0 = start of `s`).
    ///
    /// Scanning rules:
    /// - `\$` → `Literal("$")`
    /// - `$` followed by `[a-zA-Z_]` then `[a-zA-Z0-9_-]*` → `Variable(name)`
    /// - `$` followed by anything else → `Literal("$")`
    /// - all other chars → accumulated into `Literal`
    pub fn parse(s: &str) -> Self {
        Self::parse_with_offset(s, 0)
    }

    /// Parse with a base offset for span computation.
    /// `base_offset` is added to all span offsets.
    pub fn parse_with_offset(s: &str, base_offset: usize) -> Self {
        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut literal_start = 0usize;
        let mut chars = s.char_indices().peekable();

        // Helper to flush accumulated literal text
        let flush_literal = |literal: &mut String, literal_start: &mut usize, parts: &mut Vec<TextPart>| {
            if !literal.is_empty() {
                let span = Span::new(1, 1, base_offset + *literal_start, literal.len());
                parts.push(TextPart::Literal {
                    text: std::mem::take(literal),
                    span,
                });
                *literal_start = 0;
            }
        };

        while let Some(&(byte_idx, ch)) = chars.peek() {
            if ch == '\\' {
                chars.next(); // consume '\'
                if chars.peek().map(|&(_, c)| c) == Some('$') {
                    chars.next(); // consume '$'
                    if literal.is_empty() {
                        literal_start = byte_idx;
                    }
                    literal.push('$');
                } else {
                    if literal.is_empty() {
                        literal_start = byte_idx;
                    }
                    literal.push('\\');
                }
            } else if ch == '$' {
                chars.next(); // consume '$'
                if let Some(&(_, next_ch)) = chars.peek() {
                    if next_ch.is_ascii_alphabetic() || next_ch == '_' {
                        // Start of a variable name — flush literal first
                        flush_literal(&mut literal, &mut literal_start, &mut parts);

                        let var_start = byte_idx; // includes the $
                        let mut name = String::new();
                        while let Some(&(_, c)) = chars.peek() {
                            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                                name.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        let var_len = name.len() + 1; // +1 for $
                        parts.push(TextPart::Variable {
                            name,
                            span: Span::new(1, 1, base_offset + var_start, var_len),
                        });
                    } else {
                        // `$` followed by non-identifier char → literal `$`
                        if literal.is_empty() {
                            literal_start = byte_idx;
                        }
                        literal.push('$');
                    }
                } else {
                    // `$` at end of string
                    if literal.is_empty() {
                        literal_start = byte_idx;
                    }
                    literal.push('$');
                }
            } else {
                // Regular character
                if literal.is_empty() {
                    literal_start = byte_idx;
                }
                chars.next();
                literal.push(ch);
            }
        }

        // Flush remaining literal
        if !literal.is_empty() {
            let span = Span::new(1, 1, base_offset + literal_start, literal.len());
            parts.push(TextPart::Literal { text: literal, span });
        }

        InterpolatedText(parts)
    }

    /// Get the raw string representation (variable references rendered with `$` prefix).
    pub fn to_raw_string(&self) -> String {
        let mut s = String::new();
        for part in &self.0 {
            match part {
                TextPart::Literal { text, .. } => s.push_str(text),
                TextPart::Variable { name, .. } => {
                    s.push('$');
                    s.push_str(name);
                }
            }
        }
        s
    }

    /// Return true if the text contains no variable references.
    pub fn is_literal(&self) -> bool {
        self.0.iter().all(|p| matches!(p, TextPart::Literal { .. }))
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
                TextPart::Variable { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }
}

impl fmt::Display for TextPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextPart::Literal { text, .. } => f.write_str(text),
            TextPart::Variable { name, .. } => write!(f, "${name}"),
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
        assert!(matches!(&t.parts()[0], TextPart::Literal { text, .. } if text == "counter is "));
        assert!(matches!(&t.parts()[1], TextPart::Variable { name, .. } if name == "counter"));
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

    #[test]
    fn test_span_offsets() {
        let t = InterpolatedText::parse("ab$cd");
        assert_eq!(t.parts().len(), 2);
        match &t.parts()[0] {
            TextPart::Literal { text, span } => {
                assert_eq!(text, "ab");
                assert_eq!(span.offset, 0);
                assert_eq!(span.len, 2);
            }
            _ => panic!("expected Literal"),
        }
        match &t.parts()[1] {
            TextPart::Variable { name, span } => {
                assert_eq!(name, "cd");
                assert_eq!(span.offset, 2);
                assert_eq!(span.len, 3); // $cd = 3 bytes
            }
            _ => panic!("expected Variable"),
        }
    }

    #[test]
    fn test_span_with_base_offset() {
        let t = InterpolatedText::parse_with_offset("ab$cd", 100);
        match &t.parts()[0] {
            TextPart::Literal { span, .. } => assert_eq!(span.offset, 100),
            _ => panic!("expected Literal"),
        }
        match &t.parts()[1] {
            TextPart::Variable { span, .. } => assert_eq!(span.offset, 102),
            _ => panic!("expected Variable"),
        }
    }
}
