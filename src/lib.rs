//! # mtparser
//!
//! Rust parser for MySQL [`mysqltest`](https://dev.mysql.com/doc/refman/8.0/en/mysql-test-suite.html)
//! and MariaDB `mariadb-test` `.test` / `.inc` files, built with [winnow](https://crates.io/crates/winnow).
//!
//! ## Supported Versions
//!
//! - MySQL 5.7, 8.0, 8.4, 9.7
//! - MariaDB 10.11, 11.4, 11.8, 12.3
//!
//! Default is `MysqlVersion::MySQL`, which accepts commands from any MySQL version.
//!
//! ## Quick Start
//!
//! ```
//! use mtparser::parser::{parse, ParserConfig};
//!
//! let input = "--echo \"hello\"\nSELECT 1;\n";
//! let config = ParserConfig::default();
//! let statements = parse(input, config).unwrap();
//! assert_eq!(statements.len(), 2);
//! ```

pub mod ast;
pub mod error;
pub mod parser;
pub mod version;
pub mod visitor;
