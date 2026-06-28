use crate::ast::Span;

/// A variable reference: $variable_name
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct VariableRef {
    pub span: Span,
    pub name: String,
}

impl VariableRef {
    pub fn new(span: Span, name: String) -> Self {
        Self { span, name }
    }
}

/// A backtick-enclosed query expression: `SELECT ...`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct QueryExpr {
    pub span: Span,
    pub query: String,
}

impl QueryExpr {
    pub fn new(span: Span, query: String) -> Self {
        Self { span, query }
    }
}

/// Comparison operators for expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum ComparisonOp {
    Eq,  // ==
    Neq, // !=
    Lt,  // <
    Le,  // <=
    Gt,  // >
    Ge,  // >=
}

/// Right-hand side of a comparison expression.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum ComparisonRhs {
    Integer(i64),
    String(String),
    Variable(VariableRef),
    Query(QueryExpr),
}

/// Expression used in if/while/assert conditions.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Expr {
    /// $variable_name
    Variable(VariableRef),
    /// !expr — negation of any expression
    Negated(Box<Expr>),
    /// Integer literal
    Integer(i64),
    /// `SELECT ...`
    Query(QueryExpr),
    /// Variable ==|!=|<|<=|>|>= rhs
    Comparison {
        left: VariableRef,
        operator: ComparisonOp,
        right: Box<ComparisonRhs>,
    },
    /// MariaDB: $(expr) arithmetic/logical expression, e.g. $(1 + 2), $($x > 0)
    MariaDBClosure { span: Span, expression: String },
    /// MariaDB: && / || logical expression at top level, e.g. `0 && $have_debug`
    MariaDBLogical { span: Span, expression: String },
}
