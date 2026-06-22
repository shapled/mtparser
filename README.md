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
- 365 unit tests
- Built entirely with [winnow](https://crates.io/crates/winnow) 1.0 parser combinators

## Architecture

The parser is built on a unified `Stream` type (`Stateful<LocatingSlice<&str>, ParserState>`) that threads version and delimiter state through all combinators via `winnow::stream::Stateful`.

```
parse(input) = parse_statements(stream)
             = repeat(0.., parse_statement)(stream)

parse_statement = alt(
    empty_line,
    comment,
    "--" prefix → parse_command,
    bare keyword (if/while/write_file/...) → parse_command,
    fallback → delimiter-terminated SQL,
)

parse_command = read name → lowercase → match → parse_cmd_xxx_args
             (with_span for automatic span tracking)
```

- `parse_statements` uses `repeat` — no hand-written loops
- `parse_command` dispatches via `match name.as_str()` to `parse_cmd_xxx_args` functions
- Each `parse_cmd_xxx_args` is a standalone combinator returning `ModalResult<Statement>`
- Argument type parsers (`arg_rest`, `arg_variable`, `arg_token`, `arg_ws_tokens`, `arg_kv_pairs`, etc.) are shared in `parser/arg.rs`

## Installation

```toml
[dependencies]
mtparser = "0.1"
```

## Library Usage

### Quick Start

```rust
use mtparser::parser::{parse, ParserConfig};

fn main() {
    let input = "--echo hello world\nSELECT 1;\n";

    let statements = parse(input, ParserConfig::default()).unwrap();
    for stmt in &statements {
        println!("{:?}", stmt);
    }
}
// Output:
// Echo(EchoCmd { span: ..., text: "hello world" })
// Sql(SqlStatement { span: ..., sql: "SELECT 1" })
```

### Parsing

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

// Traverse statements
let statements = parse(input, ParserConfig::default())?;
let mut visitor = MyVisitor;
visitor.visit_statements(&statements);
```

`VisitResult` controls traversal: `Continue` visits children, `Skip` skips children, `Stop` aborts traversal entirely.

## Version Support

The parser validates commands against the target version. Unknown or version-incompatible commands produce errors in strict mode.

| Version | Identifier | Notable differences |
|---------|-----------|---------------------|
| MySQL 5.7 | `V57` | Only version with `require`, `system`, `real_sleep`, `disable_parsing`, `enable_parsing` |
| MySQL 8.0 | `V80` | First version with `assert` command |
| MySQL 8.4 | `V84` | Same command set as 8.0 |
| MySQL 9.7 | `V97` | Same command set as 8.0 |
| MariaDB 10.11 | `MariaDB_1011` | Superset of MySQL commands; enables `$()` expressions and `&&`/`\|\|` operators |
| MariaDB 11.4 | `MariaDB_114` | Same as 10.11 |
| MariaDB 11.8 | `MariaDB_118` | Same as 10.11 |
| MariaDB 12.3 | `MariaDB_123` | Same as 10.11 |

**Shorthand flags:**

| Flag | Covers |
|------|--------|
| `MySQL` | All MySQL versions (5.7 + 8.0 + 8.4 + 9.7) |
| `MariaDB` | All MariaDB versions (10.11 + 11.4 + 11.8 + 12.3) |
| `Compatible` | Everything (`MySQL \| MariaDB`) |

Versions can be combined with bitwise OR: `MysqlVersion::V80 | MysqlVersion::V84`.

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
