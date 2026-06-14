use crate::ast::Span;

/// A variable reference: $variable_name
#[derive(Debug, Clone, PartialEq, Eq)]
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
pub enum ComparisonRhs {
    Integer(i64),
    String(String),
    Variable(VariableRef),
    Query(QueryExpr),
}

/// Expression used in if/while/assert conditions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// $variable_name
    Variable(VariableRef),
    /// !$variable_name
    NegatedVariable(VariableRef),
    /// !`SELECT ...`
    NegatedQuery(QueryExpr),
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
}
