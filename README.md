# mtparser

[![Package Version](https://img.shields.io/hexpm/v/mtparser)](https://hex.pm/packages/mtparser)
[![Hex Docs](https://img.shields.io/badge/hex-docs-ffaff3)](https://mtparser.hexdocs.pm/)

Parser for MySQL `mysqltest` / MariaDB `mariadb-test` test files (`.test` / `.inc`), written in [Gleam](https://gleam.run).

Gleam rewrite of the Rust/winnow version ([mtparser-rs](https://github.com/shapled/mtparser)). New architecture: mysqltest syntax is line-oriented, so instead of a combinator parser the new parser runs in three explicit stages — split into lines, classify each line, parse arguments within the line — with parser state (delimiter) threaded through plain function calls. The command set is the union of all supported MySQL/MariaDB versions (maximum compatibility, no version gating).

## Status

Early rewrite in progress. Working: three-stage parse flow — `echo`, `assert`, `cat_file`, `append_file` (block command with end markers), comments, blank lines, SQL statements, and `$variable` interpolation.

## Development

```sh
gleam test                          # run tests (Erlang target)
gleam test --target javascript      # run tests (JS target)
gleam build --target javascript     # build for npm consumption later
```
