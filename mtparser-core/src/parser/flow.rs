//! Condition expression parser using winnow combinators.
//!
//! All parsers operate on `crate::parser::Stream`, threading the `ParserState`
//! (which carries the MySQL/MariaDB version) through the combinators.

use std::ops::Range;

use winnow::Parser;
use winnow::combinator::{alt, cut_err, opt, preceded, terminated};
use winnow::stream::LocatingSlice;
use winnow::stream::Location;
use winnow::stream::Stream as StreamTrait;
use winnow::token::{one_of, take_till, take_while};

use crate::ast::expr::{ComparisonOp, ComparisonRhs, Expr, QueryExpr, VariableRef};
use crate::parser::range_to_span;

/// The unified stream type, re-imported for brevity.
type Stream<'s> = crate::parser::Stream<'s>;

// ---------------------------------------------------------------------------
// Top-level entry
// ---------------------------------------------------------------------------

/// Winnow combinator: parse a full condition expression from a `Stream`.
/// Reads parens + optional negation, dispatches to the inner expression parser.
pub(crate) fn parse_condition_stream(stream: &mut Stream) -> winnow::ModalResult<Expr> {
    let input: &str = *stream.input;
    let (inner, consumed) = if input.starts_with('(') {
        // Find the matching closing paren, handling nesting and backticks
        if let Some(end) = find_matching_paren(input) {
            (&input[1..end], end + 1)
        } else {
            // No matching paren, take everything
            let end = find_any_char(input, ['\r', '\n', ';']);
            (&input[..end], end)
        }
    } else {
        let end = find_any_char(input, ['\r', '\n', ';']);
        (&input[..end], end)
    };

    let inner = inner.trim();
    if inner.is_empty() {
        return Err(winnow::error::ErrMode::Backtrack(
            winnow::error::ContextError::default(),
        ));
    }
    // Consume from stream (advance the LocatingSlice by `consumed` bytes)
    for _ in 0..consumed {
        let _: winnow::ModalResult<char> = winnow::token::any.parse_next(stream);
    }

    // Parse the inner content as a separate stream.
    // Set source to the inner text so range_to_span computes correct offsets.
    let mut inner_stream = Stream {
        input: LocatingSlice::new(inner),
        state: stream.state.clone(),
    };
    inner_stream.state.source = inner.to_string();

    // Check for negation: !expr (optional)
    // Capture the span of the '!' character using with_span
    let (negated, not_range) = opt(one_of('!')).with_span().parse_next(&mut inner_stream)?;
    // Skip whitespace
    let _: winnow::ModalResult<&str> = take_while(0.., [' ', '\t']).parse_next(&mut inner_stream);
    let expr = parse_inner_expr.parse_next(&mut inner_stream)?;

    if negated.is_some() {
        Ok(Expr::Negated {
            expr: Box::new(expr),
            span: range_to_span(&inner_stream, not_range),
        })
    } else {
        Ok(expr)
    }
}

// ---------------------------------------------------------------------------
// Inner expression parsing
// ---------------------------------------------------------------------------

/// Parse a non-negation inner expression using winnow `alt`.
fn parse_inner_expr(stream: &mut Stream) -> winnow::ModalResult<Expr> {
    alt((
        // MariaDB: $() expression — $(1 + 2), $($x > 0), etc.
        parse_mariadb_closure,
        // MariaDB: && / || logical operators
        parse_mariadb_logical,
        // Variable with optional comparison: $var [op rhs]
        parse_comparison,
        // Backtick query: `SELECT ...`
        parse_backtick_query,
        // Integer literal
        parse_integer,
    ))
    .parse_next(stream)
}

/// Parse `$()` MariaDB closure expression.
fn parse_mariadb_closure(stream: &mut Stream) -> winnow::ModalResult<Expr> {
    if !stream.state.version.is_mariadb() {
        return Err(winnow::error::ErrMode::Backtrack(
            winnow::error::ContextError::default(),
        ));
    }
    let (expr, range) = preceded("$(", terminated(take_till(0.., [')']), ')'))
        .with_span()
        .parse_next(stream)?;
    Ok(Expr::MariaDBClosure {
        span: range_to_span(stream, range),
        expression: expr.trim().to_string(),
    })
}

/// Parse `&&` or `||` MariaDB logical operator at top level.
/// Falls back (Backtrack) if no logical operator is found.
fn parse_mariadb_logical(stream: &mut Stream) -> winnow::ModalResult<Expr> {
    // Only for MariaDB — check version in the alt branch
    if !stream.state.version.is_mariadb() {
        return Err(winnow::error::ErrMode::Backtrack(
            winnow::error::ContextError::default(),
        ));
    }

    let input: &str = *stream.input;
    for op in ["&&", "||"] {
        if let Some(_pos) = find_logical_op(input, op) {
            let range = 0..input.len();
            return Ok(Expr::MariaDBLogical {
                span: range_to_span(stream, range),
                expression: input.to_string(),
            });
        }
    }
    Err(winnow::error::ErrMode::Backtrack(
        winnow::error::ContextError::default(),
    ))
}

/// Parse a variable with optional comparison: `$var [op rhs]` or just `$var`.
fn parse_comparison(stream: &mut Stream) -> winnow::ModalResult<Expr> {
    let var = parse_variable(stream)?;
    // Save checkpoint to backtrack if no operator follows
    let checkpoint = StreamTrait::checkpoint(&stream.input);
    // Skip whitespace before operator
    let _: winnow::ModalResult<&str> = take_while(0.., [' ', '\t']).parse_next(stream);
    // Try to parse a comparison operator
    let op_result: winnow::ModalResult<(ComparisonOp, Range<usize>)> = alt((
        "==".value(ComparisonOp::Eq),
        "!=".value(ComparisonOp::Neq),
        "<=".value(ComparisonOp::Le),
        ">=".value(ComparisonOp::Ge),
        "<".value(ComparisonOp::Lt),
        ">".value(ComparisonOp::Gt),
    ))
    .with_span()
    .parse_next(stream);

    match op_result {
        Ok((op, op_range)) => {
            // Skip whitespace after operator
            let _: winnow::ModalResult<&str> = take_while(0.., [' ', '\t']).parse_next(stream);
            let rhs = parse_rhs(stream)?;
            Ok(Expr::Comparison {
                left: var,
                operator: op,
                operator_span: range_to_span(stream, op_range),
                right: Box::new(rhs),
            })
        }
        Err(_) => {
            // No operator — just a variable reference (truthy check)
            StreamTrait::reset(&mut stream.input, &checkpoint);
            Ok(Expr::Variable(var))
        }
    }
}

/// Parse a backtick query: ``SELECT ...``.
fn parse_backtick_query(stream: &mut Stream) -> winnow::ModalResult<Expr> {
    let (query, range) = preceded('`', terminated(take_till(0.., ['`']), '`'))
        .with_span()
        .parse_next(stream)?;
    Ok(Expr::Query(QueryExpr::new(
        range_to_span(stream, range),
        query.to_string(),
    )))
}

/// Parse an integer literal.
fn parse_integer(stream: &mut Stream) -> winnow::ModalResult<Expr> {
    let (digits, range) = take_while(1.., ('0'..='9',))
        .with_span()
        .parse_next(stream)?;
    let n: i64 = digits
        .parse()
        .map_err(|_| winnow::error::ErrMode::Backtrack(winnow::error::ContextError::default()))?;
    Ok(Expr::Integer {
        value: n,
        span: range_to_span(stream, range),
    })
}

// ---------------------------------------------------------------------------
// Component parsers
// ---------------------------------------------------------------------------

/// Parse a `$variable_name` reference.
fn parse_variable(stream: &mut Stream) -> winnow::ModalResult<VariableRef> {
    let (name, range) = preceded(
        '$',
        cut_err(take_while(1.., ('a'..='z', 'A'..='Z', '0'..='9', '_', '-'))),
    )
    .with_span()
    .parse_next(stream)?;
    Ok(VariableRef::new(
        range_to_span(stream, range),
        name.to_string(),
    ))
}

/// Parse the right-hand side of a comparison.
fn parse_rhs(stream: &mut Stream) -> winnow::ModalResult<ComparisonRhs> {
    // Try integer first
    let checkpoint = StreamTrait::checkpoint(&stream.input);
    let int_result: winnow::ModalResult<(&str, Range<usize>)> =
        take_while(1.., ('0'..='9',))
            .with_span()
            .parse_next(stream);
    if let Ok((s, range)) = int_result {
        let value = s.parse::<i64>().unwrap_or(0);
        return Ok(ComparisonRhs::Integer {
            value,
            span: range_to_span(stream, range),
        });
    }
    StreamTrait::reset(&mut stream.input, &checkpoint);

    // Try variable
    let var_result = parse_rhs_variable.parse_next(stream);
    if var_result.is_ok() {
        return var_result;
    }
    StreamTrait::reset(&mut stream.input, &checkpoint);

    // Try backtick query
    let bt_result = parse_rhs_backtick.parse_next(stream);
    if bt_result.is_ok() {
        return bt_result;
    }
    StreamTrait::reset(&mut stream.input, &checkpoint);

    // Try quoted string
    let qs_result = parse_rhs_quoted_string.parse_next(stream);
    if qs_result.is_ok() {
        return qs_result;
    }
    StreamTrait::reset(&mut stream.input, &checkpoint);

    // Bare string (fallback)
    let (s, range) = take_till(0.., ['\r', '\n', ';', ')'])
        .with_span()
        .parse_next(stream)?;
    Ok(ComparisonRhs::String {
        value: s.to_string(),
        span: range_to_span(stream, range),
    })
}

/// Parse variable RHS: $name.
fn parse_rhs_variable(stream: &mut Stream) -> winnow::ModalResult<ComparisonRhs> {
    let (name, range) = preceded(
        '$',
        cut_err(take_while(1.., ('a'..='z', 'A'..='Z', '0'..='9', '_', '-'))),
    )
    .with_span()
    .parse_next(stream)?;
    Ok(ComparisonRhs::Variable(VariableRef::new(
        range_to_span(stream, range),
        name.to_string(),
    )))
}

/// Parse backtick query RHS: `SQL`.
fn parse_rhs_backtick(stream: &mut Stream) -> winnow::ModalResult<ComparisonRhs> {
    let (query, range) = preceded('`', terminated(take_till(0.., ['`']), '`'))
        .with_span()
        .parse_next(stream)?;
    Ok(ComparisonRhs::Query(QueryExpr::new(
        range_to_span(stream, range),
        query.to_string(),
    )))
}

/// Parse quoted string RHS: "value" or 'value'.
fn parse_rhs_quoted_string(stream: &mut Stream) -> winnow::ModalResult<ComparisonRhs> {
    let quote: char = one_of(['"', '\'']).parse_next(stream)?;
    let content_start = stream.input.current_token_start();
    let content = take_till(0.., [quote]).parse_next(stream)?;
    let content_end = stream.input.previous_token_end();
    let _: char = one_of([quote]).parse_next(stream)?;
    Ok(ComparisonRhs::String {
        value: content.to_string(),
        span: range_to_span(stream, content_start..content_end),
    })
}

/// Find the first occurrence of any character in `chars` within `s`.
fn find_any_char(s: &str, chars: impl IntoIterator<Item = char>) -> usize {
    let mut min = s.len();
    for c in chars {
        if let Some(pos) = s.find(c) {
            min = min.min(pos);
        }
    }
    min
}

// ---------------------------------------------------------------------------
// Hand-written scanner (cannot be expressed as combinators)
// ---------------------------------------------------------------------------

/// Find the matching closing paren for the first `(` in the input,
/// handling nesting and backtick-quoted strings.
/// Returns the byte offset of the closing `)`, or `None`.
fn find_matching_paren(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut backtick = false;
    let mut last_backtick = 0usize;

    for (i, &b) in bytes.iter().enumerate() {
        if b == b'`' {
            if backtick {
                // Close backtick
                backtick = false;
            } else {
                // Open backtick — record position in case it's unclosed
                backtick = true;
                last_backtick = i;
            }
            continue;
        }
        if backtick {
            continue;
        }
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }

    // If backtick is still open (unclosed), retry ignoring backticks
    if backtick {
        depth = 0;
        for (i, &b) in bytes.iter().enumerate() {
            if i == last_backtick {
                continue; // skip the unclosed backtick
            }
            if b == b'(' {
                depth += 1;
            } else if b == b')' {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    }

    None
}

/// Find a logical operator (`&&` or `||`) not inside parentheses or backticks.
fn find_logical_op(s: &str, op: &str) -> Option<usize> {
    let mut paren_depth = 0i32;
    let mut backtick = false;
    let bytes = s.as_bytes();

    for i in 0..s.len().saturating_sub(op.len()) {
        let c = bytes[i];
        if c == b'`' {
            backtick = !backtick;
            continue;
        }
        if backtick {
            continue;
        }
        if c == b'(' {
            paren_depth += 1;
            continue;
        }
        if c == b')' {
            paren_depth -= 1;
            continue;
        }
        if paren_depth == 0 && &s[i..i + op.len()] == op {
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let after_ok = i + op.len() >= s.len() || !bytes[i + op.len()].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return Some(i);
            }
        }
    }
    None
}
