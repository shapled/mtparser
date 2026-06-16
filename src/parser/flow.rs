use winnow::combinator::alt;
use winnow::token::{take_till, take_while};
use winnow::Parser;

use crate::ast::expr::{ComparisonOp, ComparisonRhs, Expr, QueryExpr, VariableRef};
use crate::ast::Span;
use crate::error::ParseError;
use crate::version::MysqlVersion;

/// Parse a condition expression from an if/while/assert command.
/// Input is the text after the command name, e.g., `($var == "value")` or `($var)`.
pub(crate) fn parse_condition(input: &str, version: MysqlVersion) -> Result<Expr, ParseError> {
    // Extract content inside parentheses if present.
    // Uses rfind(')') to correctly handle nested parens.
    let inner = if let Some(rest) = input.trim().strip_prefix('(') {
        if let Some(pos) = rest.rfind(')') {
            rest[..pos].trim()
        } else {
            input.trim()
        }
    } else {
        input.trim()
    };

    if inner.is_empty() {
        return Err(ParseError::InvalidExpression {
            message: "empty condition".to_string(),
            span: Span::dummy(),
        });
    }

    // Check for negation: !expr (negates any expression)
    if let Some(rest) = inner.strip_prefix('!') {
        let rest = rest.trim();
        let inner_expr = parse_inner_expr(rest, version)?;
        return Ok(Expr::Negated(Box::new(inner_expr)));
    }

    parse_inner_expr(inner, version)
}

/// Parse a `$variable` reference using winnow's `take_while` combinator.
fn parse_variable(s: &str) -> Option<VariableRef> {
    let s = s.trim();
    let Some(rest) = s.strip_prefix('$') else {
        return None;
    };
    let name = parse_var_name(rest);
    if !name.is_empty() {
        Some(VariableRef::new(Span::dummy(), name.to_string()))
    } else {
        None
    }
}

/// Parse a variable name (identifier) from input using winnow take_while.
fn parse_var_name(input: &str) -> &str {
    let mut stream: &str = input;
    parse_var_name_winnow_inner(&mut stream).unwrap_or("")
}

/// Winnow parser for variable name.
fn parse_var_name_winnow_inner<'s>(input: &mut &'s str) -> winnow::ModalResult<&'s str> {
    take_while(1.., ('a'..='z', 'A'..='Z', '0'..='9', '_', '-')).parse_next(input)
}

/// Parse a comparison operator using winnow `alt` combinators.
fn parse_comparison_op(s: &str) -> Option<ComparisonOp> {
    let trimmed = s.trim();

    // Two-character operators first (longest match via alt)
    let mut stream: &str = trimmed;
    if let Ok(op) = parse_two_char_op(&mut stream) {
        return Some(op);
    }

    // Single-character operators: < or > (but not << or >>)
    match trimmed.as_bytes() {
        [b'<', b'<', ..] | [b'>', b'>', ..] => None,
        [b'<', ..] => Some(ComparisonOp::Lt),
        [b'>', ..] => Some(ComparisonOp::Gt),
        _ => None,
    }
}

/// Winnow parser for two-character comparison operators.
fn parse_two_char_op<'s>(input: &mut &'s str) -> winnow::ModalResult<ComparisonOp> {
    alt((
        "==".value(ComparisonOp::Eq),
        "!=".value(ComparisonOp::Neq),
        "<=".value(ComparisonOp::Le),
        ">=".value(ComparisonOp::Ge),
    )).parse_next(input)
}

/// Parse a quote-delimited string ("content" or 'content'), returning the content.
fn parse_quoted_string<'s>(input: &mut &'s str) -> winnow::ModalResult<&'s str> {
    let quote = winnow::token::one_of(['"', '\'']).parse_next(input)?;
    let content = take_till(0.., [quote]).parse_next(input)?;
    let _ = winnow::token::one_of([quote]).parse_next(input)?;
    Ok(content)
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

/// Parse the right-hand side of a comparison expression using winnow `alt`.
fn parse_rhs(s: &str) -> ComparisonRhs {
    let s = s.trim();

    // Try integer first (fast path)
    if let Ok(n) = s.parse::<i64>() {
        return ComparisonRhs::Integer(n);
    }

    // Try variable: $name
    if let Some(var) = parse_variable(s) {
        return ComparisonRhs::Variable(var);
    }

    // Try backtick query: `SQL`
    if let Some(query) = s.strip_prefix('`') {
        if let Some(query) = query.strip_suffix('`') {
            return ComparisonRhs::Query(QueryExpr::new(
                Span::dummy(),
                query.to_string(),
            ));
        }
    }

    // Try quote-delimited string: "value" or 'value'
    let mut stream: &str = s;
    if let Ok(content) = parse_quoted_string(&mut stream) {
        return ComparisonRhs::String(content.to_string());
    }

    // Bare string
    ComparisonRhs::String(s.to_string())
}

/// Parse a non-negation inner expression.
fn parse_inner_expr(inner: &str, version: MysqlVersion) -> Result<Expr, ParseError> {
    // MariaDB: $() expression — $(1 + 2), $($x > 0), etc.
    if version.is_mariadb() {
        if let Some(rest) = inner.strip_prefix('$') {
            if let Some(rest) = rest.strip_prefix('(') {
                if let Some(pos) = rest.rfind(')') {
                    let expr = rest[..pos].trim();
                    return Ok(Expr::MariaDBClosure {
                        span: Span::dummy(),
                        expression: expr.to_string(),
                    });
                }
            }
        }

        // MariaDB: && / || logical operators at top level
        // e.g. `0 && $have_debug`, `$x || $y`
        for op in ["&&", "||"] {
            if find_logical_op(inner, op).is_some() {
                return Ok(Expr::MariaDBLogical {
                    span: Span::dummy(),
                    expression: inner.to_string(),
                });
            }
        }
    }

    // Check for variable (possibly with comparison): $var op rhs
    if let Some(var) = parse_variable(inner) {
        let after_var = &inner[var.name.len() + 1..].trim(); // +1 for $
        if let Some(op) = parse_comparison_op(after_var) {
            let after_op = &after_var[op.display_len()..].trim();
            let rhs = parse_rhs(after_op);
            return Ok(Expr::Comparison {
                left: var,
                operator: op,
                right: Box::new(rhs),
            });
        }
        // Just a variable reference (truthy check)
        return Ok(Expr::Variable(var));
    }

    // Check for backtick query: `SELECT ...`
    if let Some(query) = inner.strip_prefix('`') {
        if let Some(query) = query.strip_suffix('`') {
            return Ok(Expr::Query(QueryExpr::new(
                Span::dummy(),
                query.to_string(),
            )));
        }
    }

    // Check for integer literal
    if let Ok(n) = inner.parse::<i64>() {
        return Ok(Expr::Integer(n));
    }

    Err(ParseError::InvalidExpression {
        message: format!("cannot parse condition: {}", inner),
        span: Span::dummy(),
    })
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
            // Ensure we're matching a whole operator, not part of another
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let after_ok = i + op.len() >= s.len() || !bytes[i + op.len()].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return Some(i);
            }
        }
    }
    None
}
