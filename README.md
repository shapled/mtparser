# mtparser

A Rust library that parses MySQL `mysqltest` and MariaDB `mariadb-test` `.test`/`.inc` files into a typed AST.

## Features

- Parses `.test` and `.inc` files into a fully typed AST with source location spans
- Supports MySQL 5.7, 8.0, 8.4, 9.7 and MariaDB 10.11, 11.4, 11.8, 12.3
- MariaDB mode enables `$()` sub-expression expansion and `&&` logical operators
- Bitflag-based version selection with union support (`V80 | V84`)
- Visitor and MutVisitor traits for AST traversal and transformation
- Version-gated command recognition (e.g., `assert` requires 8.0+, `require` requires 5.7/MariaDB)
- Flow control: `--if`/`--while` blocks with condition expressions, bare `if`/`while`, inline `if`
- Parses 39,000+ MySQL files and 31,000+ MariaDB files at 100% success rate
- 56 unit tests
- Built with [winnow](https://crates.io/crates/winnow) 1.0 parser combinators

## Installation

```toml
[dependencies]
mtparser = "0.1"
```

## Library Usage

### Parsing

```rust
use mtparser::parser::{parse, ParserConfig};
use mtparser::version::MysqlVersion;

// Parse a test file (default: Compatible mode, all MySQL versions)
let config = ParserConfig::default();
let test_file = parse(input, config)?;

// Parse with MariaDB support (enables $() expressions and && operators)
let config = ParserConfig::new(MysqlVersion::MariaDB);
let test_file = parse(input, config)?;

// Parse with a specific MySQL version
let config = ParserConfig::new(MysqlVersion::V80);
let test_file = parse(input, config)?;

// Custom version union
let config = ParserConfig::new(MysqlVersion::V80 | MysqlVersion::V84);
let test_file = parse(input, config)?;

// Both MySQL and MariaDB
let config = ParserConfig::new(MysqlVersion::Compatible | MysqlVersion::MariaDB);
let test_file = parse(input, config)?;
```

The returned `TestFile` contains a `statements: Vec<Statement>` where each `Statement` is an enum variant representing a command, SQL statement, flow control block, comment, or empty line.

### Visitor API

```rust
use mtparser::visitor::{Visitor, VisitResult};
use mtparser::ast::*;

struct MyVisitor;

impl Visitor for MyVisitor {
    fn visit_echo(&mut self, cmd: &EchoCmd) -> VisitResult {
        println!("echo: {:?}", cmd);
        VisitResult::Continue
    }

    fn visit_statement(&mut self, stmt: &Statement) -> VisitResult {
        // Called before dispatching to specific command hooks
        VisitResult::Continue
    }

    fn leave_statement(&mut self, stmt: &Statement) {
        // Called after all children have been visited
    }

    fn visit_sql(&mut self, stmt: &SqlStatement) -> VisitResult {
        // Plain SQL statements (not prefixed with --)
        VisitResult::Continue
    }
    // ... many more hooks for each command type
}
```

`VisitResult` controls traversal: `Continue` visits children, `Skip` skips children, `Stop` aborts traversal entirely.

## CLI Usage

```
mtparser <file>                                    Parse a single file, print AST
mtparser --analyze <dir> [--version X.Y]           Analyze directory, report success rate
mtparser --version-errors <dir> [--version X.Y]   Scan for version-incompatible commands
```

## Supported Commands

**General**: `echo`, `let`, `error`, `source`, `skip`, `die`, `exit`, `inc`, `dec`, `assert`, `expr`, `output`, `end`

**Execution**: `exec`, `execw`, `exec_in_background`, `system` (5.7/MariaDB)

**Connection**: `connect`, `connection`, `disconnect`, `change_user`, `reset_connection`

**Query**: `query`, `eval`, `send`, `send_eval`, `reap`, `horizontal_results`, `vertical_results`, `sorted_result`, `partially_sorted_result`

**Result manipulation**: `replace_result`, `replace_column`, `replace_regex`, `replace_numeric_round`, `lowercase_result`

**Flow control**: `--if` (condition) `{ ... }`, `--while` (condition) `{ ... }`, bare `if`/`while`, inline `if (cond) { body; }`, `perl` blocks

**Delimiter**: `delimiter`

**File I/O**: `write_file`, `append_file`, `remove_file`, `remove_files_wildcard`, `copy_file`, `move_file`, `mkdir`, `rmdir`, `chmod`, `diff_files`, `file_exists`, `cat_file`, `list_files`, `copy_files_wildcard`

**Server**: `shutdown_server`, `send_quit`, `send_shutdown`, `sync_slave_with_master`

**Disable/enable**: `disable_parsing`, `enable_parsing` (5.7/MariaDB), `disable_warnings`, `enable_warnings`, `disable_query_log`, `enable_query_log`, etc.

**Other**: `sleep`, `real_sleep` (5.7/MariaDB), `character_set` (5.7/MariaDB), `require` (5.7/MariaDB)

## License

Apache-2.0
