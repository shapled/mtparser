//! Error types for the mysqltest parser.

use crate::ast::Span;

/// Parse error types.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("syntax error at {span}: {message}")]
    Syntax { message: String, span: Span },

    #[error("unexpected end of file while parsing {context}")]
    UnexpectedEof { context: String, span: Span },

    #[error("unknown command '{command}' at {span}")]
    UnknownCommand {
        command: String,
        span: Span,
        suggestion: Option<String>,
    },

    #[error("version error: command '{command}' is not supported in {version} at {span}")]
    VersionMismatch {
        command: String,
        version: String,
        span: Span,
    },

    #[error("unterminated block command '{command}', expected end marker '{marker}' at {span}")]
    UnterminatedBlock {
        command: String,
        marker: String,
        span: Span,
    },

    #[error("unterminated flow control '{kind}' at {span}")]
    UnterminatedFlowControl { kind: String, span: Span },

    #[error("unexpected '{token}' at {span}, expected {expected}")]
    UnexpectedToken {
        token: String,
        expected: String,
        span: Span,
    },

    #[error("invalid connect parameters at {span}: {message}")]
    InvalidConnectParams { message: String, span: Span },

    #[error("invalid expression at {span}: {message}")]
    InvalidExpression { message: String, span: Span },
}

/// Diagnostic with severity level for non-fatal issues.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}
