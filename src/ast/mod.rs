//! Abstract Syntax Tree types for mysqltest files.
//!
//! The AST consists of:
//! - [`Statement`] — the root node type with 58 variants for every possible statement
//! - [`commands`] — individual command struct definitions (EchoCmd, LetCmd, etc.)
//! - [`Span`] — source location tracking
//! - [`InterpolatedText`] — text with `$variable` references
//! - [`Expr`] — condition expressions for if/while/assert

pub mod commands;
pub mod expr;
pub mod span;
pub mod statement;
pub mod text;

pub use commands::*;
pub use expr::*;
pub use span::*;
pub use statement::*;
pub use text::*;
