//// MySQL mysqltest / MariaDB mariadb-test file parser.
////
//// Parses `.test` / `.inc` files into an AST.
////
//// ## Architecture
////
//// mysqltest syntax is line-oriented, so the parser runs in three stages:
////
//// 1. **Lines** — split the source into lines, each tagged with a byte offset
////    ([`mtparser/lines`](lines.html))
//// 2. **Classify** — decide what each line is (blank / comment / command /
////    delimiter-terminated statement)
//// 3. **Parse** — parse command arguments within the statement text
////    ([`mtparser/parser`](parser.html))
////
//// Parser state (current delimiter) is threaded explicitly through function
//// calls; there is no mutable state. The command set is the union of all
//// supported MySQL/MariaDB versions (maximum compatibility, no version
//// gating), and unrecognized `--` commands degrade to SQL text, so `parse`
//// succeeds on any input.
////
//// Gleam rewrite of the Rust/winnow version (`../mtparser-rs`), which hit
//// complexity issues from the mismatch between winnow's pure-combinator
//// model and line-oriented mutable-delimiter parsing.

import mtparser/ast.{type Statement}
import mtparser/error.{type ParserError}
import mtparser/helper.{split_lines}
import mtparser/parser

/// Parse a complete test file into an AST.
pub fn parse(input: String) -> Result(List(Statement), ParserError) {
  case parser.parse_statements(split_lines(input)) {
    Ok(#(statements, _state)) -> Ok(statements)
    Error(e) -> Error(e)
  }
}
