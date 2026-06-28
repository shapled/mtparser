//! Argument type parsers for mysqltest commands.
//!
//! These parsers operate on the unified [`Stream`] type and return typed
//! argument values (`InterpolatedText`, `String`, `&str`, etc.).
//!
//! ## Argument type taxonomy
//!
//! | Parser | Returns | Used for |
//! |--------|---------|----------|
//! | [`arg_rest`] | `InterpolatedText` | echo text, eval sql, exec command |
//! | [`arg_rest_opt`] | `Option<InterpolatedText>` | skip/die message (optional) |
//! | [`arg_rest_literal`] | `String` | delimiter, charset (no interpolation) |
//! | [`arg_variable`] | `String` | inc/dec/expr ($var → name) |
//! | [`arg_token`] | `&str` | source file, mkdir dir (single token) |
//! | [`arg_ws_tokens`] | `Vec<&str>` | copy_file src dest [retry] |
//! | [`arg_kv_pairs`] | `Vec<(IT, IT)>` | replace_result old new ... |
//! | [`arg_kv_triples`] | `Vec<(&str, IT, IT)>` | replace_column col old new ... |

use winnow::Parser;
use winnow::combinator::{alt, delimited, separated};
use winnow::token::{one_of, take_till, take_while};

use crate::ast::InterpolatedText;
use crate::parser::Stream;

// ---------------------------------------------------------------------------
// Basic whitespace
// ---------------------------------------------------------------------------

/// Skip zero or more ASCII whitespace characters (space, tab).
pub(crate) fn ws(s: &mut Stream) -> winnow::ModalResult<()> {
    let _: &str = take_while(0.., [' ', '\t']).parse_next(s)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// "Take rest" family — consume remaining input on the current line
// ---------------------------------------------------------------------------

/// Take remaining input (trimmed) as [`InterpolatedText`] (with `$var` interpolation).
/// Strips trailing delimiter (e.g., `;`) for bare command compatibility.
pub(crate) fn arg_rest(s: &mut Stream) -> winnow::ModalResult<InterpolatedText> {
    let _ = ws(s);
    let rest = take_till(0.., ['\r', '\n']).parse_next(s)?;
    let trimmed = rest.trim_end();
    let delim = &s.state.delimiter;
    let stripped = trimmed
        .strip_suffix(delim.as_str())
        .unwrap_or(trimmed)
        .trim_end();
    Ok(InterpolatedText::from(stripped))
}

/// Take remaining input as [`InterpolatedText`], or `None` if empty.
/// Strips trailing delimiter.
pub(crate) fn arg_rest_opt(s: &mut Stream) -> winnow::ModalResult<Option<InterpolatedText>> {
    let _ = ws(s);
    let rest = take_till(0.., ['\r', '\n']).parse_next(s)?;
    let trimmed = rest.trim_end();
    let delim = &s.state.delimiter;
    let stripped = trimmed
        .strip_suffix(delim.as_str())
        .unwrap_or(trimmed)
        .trim_end();
    if stripped.is_empty() {
        Ok(None)
    } else {
        Ok(Some(InterpolatedText::from(stripped)))
    }
}

/// Take remaining input as plain [`String`] (no interpolation).
/// Strips trailing delimiter.
pub(crate) fn arg_rest_literal(s: &mut Stream) -> winnow::ModalResult<String> {
    let _ = ws(s);
    let rest = take_till(0.., ['\r', '\n']).parse_next(s)?;
    let trimmed = rest.trim_end();
    let delim = &s.state.delimiter;
    let stripped = trimmed
        .strip_suffix(delim.as_str())
        .unwrap_or(trimmed)
        .trim_end();
    Ok(stripped.to_string())
}

// ---------------------------------------------------------------------------
// Structured argument parsers
// ---------------------------------------------------------------------------

/// Parse `$variable_name`, returning the name without the `$` prefix.
pub(crate) fn arg_variable(s: &mut Stream) -> winnow::ModalResult<String> {
    let _ = one_of('$').parse_next(s)?;
    let name = take_while(1.., ('a'..='z', 'A'..='Z', '0'..='9', '_')).parse_next(s)?;
    Ok(name.to_string())
}

/// Parse a single token: either a quoted string or non-whitespace text.
/// Quotes recognized: `'`, `"`, `` ` ``.
pub(crate) fn arg_token<'s>(s: &mut Stream<'s>) -> winnow::ModalResult<&'s str> {
    alt((arg_quoted, take_till(1.., [' ', '\t', '\r', '\n']))).parse_next(s)
}

/// Parse a quote-delimited string: `"content"`, `'content'`, or `` `content` ``.
/// Returns the inner content (without quotes).
fn arg_quoted<'s>(s: &mut Stream<'s>) -> winnow::ModalResult<&'s str> {
    let quote = one_of(['\'', '"', '`']).parse_next(s)?;
    let content = take_till(0.., [quote]).parse_next(s)?;
    one_of([quote]).parse_next(s)?; // consume closing quote
    Ok(content)
}

/// Parse whitespace-separated tokens.
pub(crate) fn arg_ws_tokens<'s>(s: &mut Stream<'s>) -> winnow::ModalResult<Vec<&'s str>> {
    let _ = ws(s)?;
    separated(
        0..,
        take_till(1.., [' ', '\t', '\n', '\r']),
        take_while(1.., [' ', '\t']),
    )
    .parse_next(s)
}

/// Parse comma-separated elements, each surrounded by optional whitespace.
/// Returns a reusable parser (not a direct function call).
pub(crate) fn arg_comma_list<'p, E>(
    elem: E,
) -> impl winnow::Parser<Stream<'p>, Vec<&'p str>, winnow::error::ErrMode<winnow::error::ContextError>>
+ 'p
where
    E: winnow::Parser<Stream<'p>, &'p str, winnow::error::ErrMode<winnow::error::ContextError>>
        + 'p,
{
    separated(0.., delimited(ws, elem, ws), ',')
}

/// Parse whitespace-separated key-value pairs: `old new old2 new2 ...`.
pub(crate) fn arg_kv_pairs(
    s: &mut Stream,
) -> winnow::ModalResult<Vec<(InterpolatedText, InterpolatedText)>> {
    let tokens = arg_ws_tokens(s)?;
    let pairs: Vec<(InterpolatedText, InterpolatedText)> = tokens
        .chunks_exact(2)
        .filter_map(|chunk| {
            Some((
                InterpolatedText::from(chunk[0]),
                InterpolatedText::from(chunk[1]),
            ))
        })
        .collect();
    Ok(pairs)
}

/// Parse whitespace-separated triples: `col old new col2 old2 new2 ...`.
pub(crate) fn arg_kv_triples(
    s: &mut Stream,
) -> winnow::ModalResult<Vec<(String, InterpolatedText, InterpolatedText)>> {
    let tokens = arg_ws_tokens(s)?;
    let triples: Vec<(String, InterpolatedText, InterpolatedText)> = tokens
        .chunks_exact(3)
        .filter_map(|chunk| {
            Some((
                chunk[0].to_string(),
                InterpolatedText::from(chunk[1]),
                InterpolatedText::from(chunk[2]),
            ))
        })
        .collect();
    Ok(triples)
}
