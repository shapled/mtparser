//! # mtparser
//!
//! MySQL [`mysqltest`](https://dev.mysql.com/doc/refman/8.0/en/mysql-test-suite.html)
//! test file parser — parses `.test` / `.inc` files into a typed AST.
//!
//! ## Supported Versions
//!
//! - MySQL 5.7 (superset of commands)
//! - MySQL 8.0
//! - MySQL 8.4
//! - MySQL 9.7
//!
//! Default is `MysqlVersion::Compatible`, which accepts commands from any version.
//!
//! ## Quick Start
//!
//! ```
//! use mtparser::parser::{parse, ParserConfig};
//!
//! let input = "--echo \"hello\"\nSELECT 1;\n";
//! let config = ParserConfig::default();
//! let test_file = parse(input, config).unwrap();
//! assert_eq!(test_file.statements.len(), 2);
//! ```

pub mod ast;
pub mod error;
pub mod parser;
pub mod version;
pub mod visitor;
