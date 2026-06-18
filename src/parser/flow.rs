//! Condition expression parser using winnow combinators.
//!
//! All parsers operate on `FlowStream<'s> = Stateful<&'s str, MysqlVersion>`,
//! threading the MySQL/MariaDB version through the combinators.

use winnow::combinator::{alt, cut_err, opt, preceded, terminated};
use winnow::stream::Stateful;
use winnow::stream::Stream as StreamTrait;
use winnow::token::{one_of, take_till, take_while};
use winnow::Parser;

use crate::ast::expr::{ComparisonOp, ComparisonRhs, Expr, QueryExpr, VariableRef};
use crate::ast::Span;
use crate::version::MysqlVersion;

/// Flow-specific stream type: text input + version state.
pub(crate) type FlowStream<'s> = Stateful<&'s str, MysqlVersion>;

// ---------------------------------------------------------------------------
// Top-level entry
// ---------------------------------------------------------------------------

/// Parse a condition expression from an if/while/assert command.
/// Input is the text after the command name, e.g., `($var == "value")` or `($var)`.
pub(crate) fn parse_condition(input: &str, version: MysqlVersion) -> Result<Expr, crate::error::ParseError> {
    let mut stream = FlowStream { input, state: version };
    parse_condition_inner(&mut stream).map_err(|e| crate::error::ParseError::InvalidExpression {
        message: format!("condition parse error: {}", e),
        span: Span::dummy(),
    })
}

/// Winnow combinator: parse a full condition expression (parens + optional negation).
fn parse_condition_inner(stream: &mut FlowStream) -> winnow::ModalResult<Expr> {
    let input = stream.input;
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
        return Err(winnow::error::ErrMode::Backtrack(winnow::error::ContextError::default()));
    }
    // Consume from stream
    stream.input = &stream.input[consumed..];

    // Parse the inner content as a separate stream
    let mut inner_stream = FlowStream { input: inner, state: stream.state };

    // Check for negation: !expr (optional)
    let negated: Option<char> = opt(one_of('!')).parse_next(&mut inner_stream)?;
    // Skip whitespace
    let _: winnow::ModalResult<&str> = take_while(0.., [' ', '\t']).parse_next(&mut inner_stream);
    let expr = parse_inner_expr.parse_next(&mut inner_stream)?;

    if negated.is_some() {
        Ok(Expr::Negated(Box::new(expr)))
    } else {
        Ok(expr)
    }
}

// ---------------------------------------------------------------------------
// Inner expression parsing
// ---------------------------------------------------------------------------

/// Parse a non-negation inner expression using winnow `alt`.
fn parse_inner_expr(stream: &mut FlowStream) -> winnow::ModalResult<Expr> {
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
fn parse_mariadb_closure(stream: &mut FlowStream) -> winnow::ModalResult<Expr> {
    if !stream.state.is_mariadb() {
        return Err(winnow::error::ErrMode::Backtrack(winnow::error::ContextError::default()));
    }
    let expr = preceded("$(", terminated(take_till(0.., [')']), ')')).parse_next(stream)?;
    Ok(Expr::MariaDBClosure {
        span: Span::dummy(),
        expression: expr.trim().to_string(),
    })
}

/// Parse `&&` or `||` MariaDB logical operator at top level.
/// Falls back (Backtrack) if no logical operator is found.
fn parse_mariadb_logical(stream: &mut FlowStream) -> winnow::ModalResult<Expr> {
    // Only for MariaDB — check version in the alt branch
    if !stream.state.is_mariadb() {
        return Err(winnow::error::ErrMode::Backtrack(winnow::error::ContextError::default()));
    }

    let input = stream.input;
    for op in ["&&", "||"] {
        if let Some(_pos) = find_logical_op(input, op) {
            return Ok(Expr::MariaDBLogical {
                span: Span::dummy(),
                expression: input.to_string(),
            });
        }
    }
    Err(winnow::error::ErrMode::Backtrack(winnow::error::ContextError::default()))
}

/// Parse a variable with optional comparison: `$var [op rhs]` or just `$var`.
fn parse_comparison(stream: &mut FlowStream) -> winnow::ModalResult<Expr> {
    let var = parse_variable(stream)?;
    // Save checkpoint to backtrack if no operator follows
    let checkpoint = StreamTrait::checkpoint(&stream.input);
    // Skip whitespace before operator
    let _: winnow::ModalResult<&str> = take_while(0.., [' ', '\t']).parse_next(stream);
    // Try to parse a comparison operator
    let op_result: winnow::ModalResult<ComparisonOp> = alt((
        "==".value(ComparisonOp::Eq),
        "!=".value(ComparisonOp::Neq),
        "<=".value(ComparisonOp::Le),
        ">=".value(ComparisonOp::Ge),
        "<".value(ComparisonOp::Lt),
        ">".value(ComparisonOp::Gt),
    ))
    .parse_next(stream);

    match op_result {
        Ok(op) => {
            // Skip whitespace after operator
            let _: winnow::ModalResult<&str> = take_while(0.., [' ', '\t']).parse_next(stream);
            let rhs = parse_rhs(stream)?;
            Ok(Expr::Comparison {
                left: var,
                operator: op,
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
fn parse_backtick_query(stream: &mut FlowStream) -> winnow::ModalResult<Expr> {
    let query = preceded('`', terminated(take_till(0.., ['`']), '`')).parse_next(stream)?;
    Ok(Expr::Query(QueryExpr::new(
        Span::dummy(),
        query.to_string(),
    )))
}

/// Parse an integer literal.
fn parse_integer(stream: &mut FlowStream) -> winnow::ModalResult<Expr> {
    let digits = take_while(1.., ('0'..='9',)).parse_next(stream)?;
    let n: i64 = digits.parse().map_err(|_| winnow::error::ErrMode::Backtrack(winnow::error::ContextError::default()))?;
    Ok(Expr::Integer(n))
}

// ---------------------------------------------------------------------------
// Component parsers
// ---------------------------------------------------------------------------

/// Parse a `$variable_name` reference.
fn parse_variable(stream: &mut FlowStream) -> winnow::ModalResult<VariableRef> {
    let _: char = one_of('$').parse_next(stream)?;
    let name = cut_err(take_while(1.., ('a'..='z', 'A'..='Z', '0'..='9', '_', '-'))).parse_next(stream)?;
    Ok(VariableRef::new(Span::dummy(), name.to_string()))
}

/// Parse the right-hand side of a comparison.
fn parse_rhs(stream: &mut FlowStream) -> winnow::ModalResult<ComparisonRhs> {
    alt((
        // Integer: digits → i64
        take_while(1.., ('0'..='9',))
            .map(|s: &str| ComparisonRhs::Integer(s.parse::<i64>().unwrap_or(0))),
        // Variable: $name
        parse_rhs_variable,
        // Backtick query: `SQL`
        parse_rhs_backtick,
        // Quote-delimited string: "value" or 'value'
        parse_rhs_quoted_string,
        // Bare string (fallback)
        take_till(0.., ['\r', '\n', ';', ')'])
            .map(|s: &str| ComparisonRhs::String(s.to_string())),
    ))
    .parse_next(stream)
}

/// Parse variable RHS: $name.
fn parse_rhs_variable(stream: &mut FlowStream) -> winnow::ModalResult<ComparisonRhs> {
    let _: char = one_of('$').parse_next(stream)?;
    let name = cut_err(take_while(1.., ('a'..='z', 'A'..='Z', '0'..='9', '_', '-'))).parse_next(stream)?;
    Ok(ComparisonRhs::Variable(VariableRef::new(Span::dummy(), name.to_string())))
}

/// Parse backtick query RHS: `SQL`.
fn parse_rhs_backtick(stream: &mut FlowStream) -> winnow::ModalResult<ComparisonRhs> {
    let query = preceded('`', terminated(take_till(0.., ['`']), '`')).parse_next(stream)?;
    Ok(ComparisonRhs::Query(QueryExpr::new(Span::dummy(), query.to_string())))
}

/// Parse quoted string RHS: "value" or 'value'.
fn parse_rhs_quoted_string(stream: &mut FlowStream) -> winnow::ModalResult<ComparisonRhs> {
    let quote: char = one_of(['"', '\'']).parse_next(stream)?;
    let content = take_till(0.., [quote]).parse_next(stream)?;
    let _: char = one_of([quote]).parse_next(stream)?;
    Ok(ComparisonRhs::String(content.to_string()))
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

impl ComparisonOp {
    pub(crate) fn display_len(&self) -> usize {
        match self {
            ComparisonOp::Eq => 2,
            ComparisonOp::Neq => 2,
            ComparisonOp::Le => 2,
            ComparisonOp::Ge => 2,
            ComparisonOp::Lt => 1,
            ComparisonOp::Gt => 1,
        }
    }
}
