use winnow::combinator::{dispatch, fail};
use winnow::token::{take, take_while};
use winnow::Parser;

use crate::ast::expr::{ComparisonOp, ComparisonRhs, Expr, MariaDBExpr, QueryExpr, VariableRef};
use crate::ast::Span;
use crate::error::ParseError;
use crate::version::MysqlVersion;

/// Parse a condition expression from an if/while/assert command.
/// Input is the text after the command name, e.g., `($var == "value")` or `($var)`.
pub(crate) fn parse_condition(input: &str, version: MysqlVersion) -> Result<Expr, ParseError> {
    // Extract content inside parentheses if present
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
    let mut stream = rest;
    let name = take_while::<_, _, ()>(1.., ('a'..='z', 'A'..='Z', '0'..='9', '_', '-'))
        .parse_next(&mut stream)
        .unwrap_or("");
    if !name.is_empty() {
        Some(VariableRef::new(Span::dummy(), name.to_string()))
    } else {
        None
    }
}

/// Parse a comparison operator using winnow's `dispatch!` for O(1) prefix matching.
fn parse_comparison_op(s: &str) -> Option<ComparisonOp> {
    let trimmed = s.trim();
    let mut stream = trimmed;

    // Try two-character operators first using dispatch!
    let result = dispatch!(take::<_, _, ()>(2usize);
        "==" => |_i: &mut &str| -> Result<ComparisonOp, ()> { Ok(ComparisonOp::Eq) },
        "!=" => |_i: &mut &str| -> Result<ComparisonOp, ()> { Ok(ComparisonOp::Neq) },
        "<=" => |_i: &mut &str| -> Result<ComparisonOp, ()> { Ok(ComparisonOp::Le) },
        ">=" => |_i: &mut &str| -> Result<ComparisonOp, ()> { Ok(ComparisonOp::Ge) },
        _ => fail::<_, _, ()>,
    ).parse_next(&mut stream);

    if result.is_ok() {
        return result.ok();
    }

    // Try single-character operators: < or > (but not << or >>)
    let _: Result<&str, ()> = "<".parse_next(&mut stream);
    if !trimmed.starts_with("<<") && trimmed.starts_with('<') {
        return Some(ComparisonOp::Lt);
    }
    let _: Result<&str, ()> = ">".parse_next(&mut stream);
    if !trimmed.starts_with(">>") && trimmed.starts_with('>') {
        return Some(ComparisonOp::Gt);
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

/// Parse the right-hand side of a comparison expression.
fn parse_rhs(s: &str) -> ComparisonRhs {
    let s = s.trim();
    let inner = s.trim_matches('"').trim_matches('\'').trim();

    // Try integer
    if let Ok(n) = inner.parse::<i64>() {
        return ComparisonRhs::Integer(n);
    }

    // Try variable
    if let Some(var) = parse_variable(inner) {
        return ComparisonRhs::Variable(var);
    }

    // Try backtick query
    if let Some(query) = inner.strip_prefix('`') {
        if let Some(query) = query.strip_suffix('`') {
            return ComparisonRhs::Query(QueryExpr::new(
                Span::dummy(),
                query.to_string(),
            ));
        }
    }

    // String
    ComparisonRhs::String(inner.to_string())
}

/// Parse a non-negation inner expression.
fn parse_inner_expr(inner: &str, version: MysqlVersion) -> Result<Expr, ParseError> {
    // MariaDB: $() expression — $(1 + 2), $($x > 0), etc.
    if version.is_mariadb() {
        if let Some(rest) = inner.strip_prefix('$') {
            if let Some(rest) = rest.strip_prefix('(') {
                if let Some(pos) = rest.rfind(')') {
                    let expr = rest[..pos].trim();
                    return Ok(Expr::MariaDB(MariaDBExpr::new(
                        Span::dummy(),
                        expr.to_string(),
                    )));
                }
            }
        }

        // MariaDB: && / || logical operators at top level
        // e.g. `0 && $have_debug`, `$x || $y`
        for op in ["&&", "||"] {
            if find_logical_op(inner, op).is_some() {
                return Ok(Expr::MariaDB(MariaDBExpr::new(
                    Span::dummy(),
                    inner.to_string(),
                )));
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
