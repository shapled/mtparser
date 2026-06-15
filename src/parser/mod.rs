//! Core parsing engine using winnow combinators.
//!
//! Entry points:
//! - [`parse`] — parse a string into a [`TestFile`]
//! - [`parse_bytes`] — parse raw bytes (UTF-8 lossy conversion)
//!
//! The parser operates line-by-line with winnow combinators for token-level
//! parsing (`take_till`, `take_while`) and `dispatch!` for prefix-based
//! operator matching in condition expressions.

pub mod command;
pub mod flow;

use winnow::token::{take_till, take_while};
use winnow::Parser;

use crate::ast::span::Span;
use crate::ast::statement::*;
use crate::ast::TestFile;
use crate::error::ParseError;
use crate::version::MysqlVersion;

/// Parser configuration. Created once and cannot be modified after creation.
#[derive(Debug, Clone)]
pub struct ParserConfig {
    pub version: MysqlVersion,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            version: MysqlVersion::Compatible,
        }
    }
}

impl ParserConfig {
    pub fn new(version: MysqlVersion) -> Self {
        Self { version }
    }
}

/// Mutable parsing context carried through the parse.
#[derive(Debug, Clone)]
pub(crate) struct ParseContext {
    pub(crate) config: ParserConfig,
    pub(crate) delimiter: String,
    pub(crate) line_num: u32,
    pub(crate) offset: usize,
    pub(crate) column: u32,
}

impl ParseContext {
    pub(crate) fn new(config: ParserConfig) -> Self {
        Self {
            config,
            delimiter: ";".to_string(),
            line_num: 1,
            offset: 0,
            column: 1,
        }
    }
}

/// Parse a complete test file into an AST.
pub fn parse_bytes(input: &[u8], config: ParserConfig) -> Result<TestFile, ParseError> {
    let text = String::from_utf8_lossy(input);
    parse(&text, config)
}

/// Parse a complete test file into an AST.
pub fn parse(input: &str, config: ParserConfig) -> Result<TestFile, ParseError> {
    let mut ctx = ParseContext::new(config);
    let (statements, _pos, _ln) = parse_statements(&mut ctx, input, 0, 1)?;
    Ok(TestFile::new(statements))
}

/// Read one line using winnow's `take_till` combinator.
fn read_line(input: &str) -> (&str, &str) {
    let mut stream = input;
    let line = take_till::<_, _, ()>(1.., ('\r', '\n'))
        .parse_next(&mut stream)
        .unwrap_or("");
    (line, stream)
}

/// Extract an identifier (alphanumeric + underscore) using winnow's `take_while` combinator.
fn parse_identifier(s: &str) -> &str {
    let mut stream = s;
    take_while::<_, _, ()>(1.., ('a'..='z', 'A'..='Z', '0'..='9', '_'))
        .parse_next(&mut stream)
        .unwrap_or("")
}

/// Parse a sequence of statements from input starting at pos.
fn parse_statements(
    ctx: &mut ParseContext,
    input: &str,
    mut pos: usize,
    mut line_num: u32,
) -> Result<(Vec<Statement>, usize, u32), ParseError> {
    let mut statements = Vec::new();

    while pos < input.len() {
        let remaining = &input[pos..];

        // Check for closing brace
        if remaining.trim_start().starts_with('}') {
            break;
        }

        let line_end = remaining.find('\n').unwrap_or(remaining.len());
        let line_len = if line_end < remaining.len() { line_end + 1 } else { line_end };
        let trimmed = remaining[..line_end].trim();
        let leading_spaces = remaining.len() - trimmed.len();
        ctx.column = (leading_spaces + 1) as u32;
        ctx.line_num = line_num;
        ctx.offset = pos;
        let span = Span::new(line_num, ctx.column, pos, line_len);

        if trimmed.is_empty() {
            statements.push(Statement::Empty);
        } else if let Some(comment_text) = trimmed.strip_prefix('#') {
            statements.push(Statement::Comment(CommentNode {
                span,
                text: comment_text.into(),
            }));
        } else if let Some(rest) = trimmed.strip_prefix("--") {
            let rest = rest.trim_start();
            let first_word = parse_identifier(rest);

            match first_word.to_ascii_lowercase().as_str() {
                "if" => {
                    let condition_text = rest[first_word.len()..].trim();
                    let (block, new_pos, new_ln) =
                        parse_if_block(ctx, condition_text, input, pos + line_len, line_num, span)?;
                    statements.push(Statement::If(block));
                    pos = new_pos;
                    line_num = new_ln;
                    continue;
                }
                "while" => {
                    let condition_text = rest[first_word.len()..].trim();
                    let (block, new_pos, new_ln) =
                        parse_while_block(ctx, condition_text, input, pos + line_len, line_num, span)?;
                    statements.push(Statement::While(block));
                    pos = new_pos;
                    line_num = new_ln;
                    continue;
                }
                "write_file" => {
                    let args = rest[first_word.len()..].trim();
                    let (stmt, new_pos, new_ln) =
                        parse_write_file_block(args, input, pos + line_len, line_num, span)?;
                    statements.push(stmt);
                    pos = new_pos;
                    line_num = new_ln;
                    continue;
                }
                "append_file" => {
                    let args = rest[first_word.len()..].trim();
                    let (stmt, new_pos, new_ln) =
                        parse_append_file_block(args, input, pos + line_len, line_num, span)?;
                    statements.push(stmt);
                    pos = new_pos;
                    line_num = new_ln;
                    continue;
                }
                "perl" => {
                    let arg_text = rest[first_word.len()..].trim();
                    let end_marker = if arg_text.is_empty() { "EOF".to_string() } else { arg_text.to_string() };
                    let (block, new_pos, new_ln) =
                        parse_perl_block(&end_marker, input, pos + line_len, line_num, span)?;
                    statements.push(Statement::Perl(block));
                    pos = new_pos;
                    line_num = new_ln;
                    continue;
                }
                _ => {
                    let stmt = parse_double_dash_command(ctx, rest, span)?;
                    statements.push(stmt);
                }
            }
        } else {
            // Non-`--` prefixed text
            let first_word = parse_identifier(trimmed);

            match first_word.to_ascii_lowercase().as_str() {
                "delimiter" => {
                    let stmt = command::parse_known_command(ctx, "delimiter", trimmed, span)?;
                    statements.push(stmt);
                    pos += line_len;
                    line_num += 1;
                    continue;
                }
                "write_file" => {
                    let rest = trimmed[first_word.len()..].trim();
                    // Strip trailing delimiter, but only after quotes are handled
                    // to avoid cutting inside quoted filenames like "file;name"
                    let rest = strip_trailing_delimiter_quoted(rest, &ctx.delimiter);
                    let (stmt, new_pos, new_ln) =
                        parse_write_file_block(rest, input, pos + line_len, line_num, span)?;
                    statements.push(stmt);
                    pos = new_pos;
                    line_num = new_ln;
                    continue;
                }
                "append_file" => {
                    let rest = trimmed[first_word.len()..].trim();
                    let rest = strip_trailing_delimiter_quoted(rest, &ctx.delimiter);
                    let (stmt, new_pos, new_ln) =
                        parse_append_file_block(rest, input, pos + line_len, line_num, span)?;
                    statements.push(stmt);
                    pos = new_pos;
                    line_num = new_ln;
                    continue;
                }
                "if" => {
                    let leading = line_end - trimmed.len();
                    let cond_start = pos + leading + first_word.len();
                    let (stmt, new_pos, new_ln) = parse_bare_flow_block(
                        ctx, input, pos, line_len, line_num, cond_start, span, false)?;
                    statements.push(stmt);
                    pos = new_pos;
                    line_num = new_ln;
                    continue;
                }
                "while" => {
                    let leading = line_end - trimmed.len();
                    let cond_start = pos + leading + first_word.len();
                    let (stmt, new_pos, new_ln) = parse_bare_flow_block(
                        ctx, input, pos, line_len, line_num, cond_start, span, true)?;
                    statements.push(stmt);
                    pos = new_pos;
                    line_num = new_ln;
                    continue;
                }
                "perl" => {
                    let rest = trimmed[first_word.len()..].trim();
                    let arg = rest.strip_suffix(&ctx.delimiter).unwrap_or(rest).trim();
                    let end_marker = if arg.is_empty() { "EOF".to_string() } else { arg.to_string() };
                    let (block, new_pos, new_ln) =
                        parse_perl_block(&end_marker, input, pos + line_len, line_num, span)?;
                    statements.push(Statement::Perl(block));
                    pos = new_pos;
                    line_num = new_ln;
                    continue;
                }
                _ => {
                    let (stmt, consumed) = parse_delimiter_terminated(ctx, input, pos, line_num, span)?;
                    statements.push(stmt);
                    pos += consumed;
                    line_num += input[pos - consumed..pos].matches('\n').count() as u32;
                    continue;
                }
            }
        }

        pos += line_len;
        line_num += 1;
    }

    Ok((statements, pos, line_num))
}

/// Classification result for a command name.
pub(crate) enum CommandKind {
    Known(String),
    Unknown,
}

/// Classify a command name. Returns `Known(lowercase_name)` for recognized commands,
/// `Unknown` otherwise.
pub(crate) fn classify_command(word: &str) -> CommandKind {
    let lower = word.to_ascii_lowercase();
    match lower.as_str() {
        "echo" | "let" | "error" | "source" | "skip" | "die" | "exit"
        | "exec" | "execw" | "exec_in_background" | "sleep" | "inc" | "dec"
        | "assert" | "expr" | "connect" | "connection" | "disconnect"
        | "change_user" | "reset_connection" | "query" | "eval" | "send"
        | "send_eval" | "reap" | "horizontal_results" | "vertical_results"
        | "replace_result" | "replace_column" | "replace_regex" | "sorted_result"
        | "partially_sorted_result" | "replace_numeric_round" | "delimiter"
        | "write_file" | "append_file" | "remove_file" | "remove_files_wildcard"
        | "copy_file" | "move_file" | "mkdir" | "rmdir" | "chmod"
        | "diff_files" | "file_exists" | "cat_file" | "list_files"
        | "shutdown_server" | "send_quit" | "send_shutdown"
        | "if" | "while" | "end" | "perl" | "character_set" | "system"
        | "real_sleep" | "require" | "lowercase_result"
        | "sync_slave_with_master" | "copy_files_wildcard"
        | "disable_warnings" | "enable_warnings"
        | "disable_query_log" | "enable_query_log"
        | "disable_result_log" | "enable_result_log"
        | "disable_info" | "enable_info"
        | "disable_metadata" | "enable_metadata"
        | "disable_ps_protocol" | "enable_ps_protocol"
        | "disable_reconnect" | "enable_reconnect"
        | "disable_connect_log" | "enable_connect_log"
        | "disable_session_track_info" | "enable_session_track_info"
        | "disable_testcase" | "enable_testcase"
        | "disable_parsing" | "enable_parsing"
        | "disable_async_client" | "enable_async_client"
        | "disable_prepare_warnings" | "enable_prepare_warnings" => {
            CommandKind::Known(lower)
        }
        _ => CommandKind::Unknown,
    }
}
/// Parse `--command args` line.
fn parse_double_dash_command(
    ctx: &mut ParseContext,
    rest: &str,
    span: Span,
) -> Result<Statement, ParseError> {
    let first_word = parse_identifier(rest);

    match classify_command(first_word) {
        CommandKind::Known(name) => command::parse_known_command(ctx, &name, rest, span),
        CommandKind::Unknown => Ok(Statement::Sql(SqlStatement {
            span,
            sql: rest.trim().into(),
        })),
    }
}

/// Parse `--if (condition)` followed by `{ body }` across multiple lines.
fn parse_if_block(
    ctx: &mut ParseContext,
    condition_text: &str,
    input: &str,
    after_first_line: usize,
    start_line: u32,
    span: Span,
) -> Result<(IfBlock, usize, u32), ParseError> {
    let condition = flow::parse_condition(condition_text, ctx.config.version)?;
    let brace_on_first_line = condition_text.trim().ends_with('{');
    let (open_brace_pos, open_brace_line) = if brace_on_first_line {
        (after_first_line, start_line + 1)
    } else {
        find_open_brace(input, after_first_line, start_line)?
    };
    let (body, after_body_pos, after_body_line) =
        parse_statements(ctx, input, open_brace_pos, open_brace_line)?;
    let (end_pos, end_line) = consume_close_brace(input, after_body_pos, after_body_line)?;
    Ok((IfBlock { span, condition, body }, end_pos, end_line))
}

/// Parse `--while (condition)` followed by `{ body }` across multiple lines.
fn parse_while_block(
    ctx: &mut ParseContext,
    condition_text: &str,
    input: &str,
    after_first_line: usize,
    start_line: u32,
    span: Span,
) -> Result<(WhileBlock, usize, u32), ParseError> {
    let condition = flow::parse_condition(condition_text, ctx.config.version)?;
    let brace_on_first_line = condition_text.trim().ends_with('{');
    let (open_brace_pos, open_brace_line) = if brace_on_first_line {
        (after_first_line, start_line + 1)
    } else {
        find_open_brace(input, after_first_line, start_line)?
    };
    let (body, after_body_pos, after_body_line) =
        parse_statements(ctx, input, open_brace_pos, open_brace_line)?;
    let (end_pos, end_line) = consume_close_brace(input, after_body_pos, after_body_line)?;
    Ok((WhileBlock { span, condition, body }, end_pos, end_line))
}

/// Find the opening `{` brace for an if/while block.
/// Handles `{` alone on a line or `{` with content after it.
/// Returns `(body_start_pos, body_line_num)`.
fn find_open_brace(input: &str, start_pos: usize, mut line_num: u32) -> Result<(usize, u32), ParseError> {
    let mut pos = start_pos;
    let start_line = line_num;
    loop {
        if pos >= input.len() {
            return Err(ParseError::UnterminatedFlowControl {
                kind: "if/while".to_string(),
                span: Span::new(start_line, 1, start_pos, pos - start_pos),
            });
        }
        let (line, _) = read_line(&input[pos..]);
        let line_end = input[pos..].find('\n').unwrap_or(input[pos..].len());
        let line_len = if line_end < input[pos..].len() { line_end + 1 } else { line_end };
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            let brace_leading = line.len() - trimmed.len();
            let after_brace = pos + brace_leading + 1;
            let rest_of_line = trimmed.get(1..).map(|s| s.trim()).unwrap_or("");
            if rest_of_line.is_empty() {
                // `{` alone — body starts on next line
                pos += line_len;
                line_num += 1;
                return Ok((pos, line_num));
            }
            // `{` with content — body starts right after `{`
            return Ok((after_brace, line_num + 1));
        }
        pos += line_len;
        line_num += 1;
    }
}

/// Scan for the opening `{` of a bare (non-`--` prefixed) if/while block.
/// Searches from `condition_start` (right after the keyword) in the input.
/// Returns `(condition_text, body_start_pos, body_line_num)`.
fn scan_bare_block_brace(
    input: &str,
    condition_start: usize,
    first_line_num: u32,
) -> Result<(String, usize, u32), ParseError> {
    let search = &input[condition_start..];
    let brace_offset = match search.find('{') {
        Some(p) => p,
        None => return Err(ParseError::UnterminatedFlowControl {
            kind: "if/while".to_string(),
            span: Span::new(first_line_num, 1, condition_start, search.len()),
        }),
    };
    let condition_text = search[..brace_offset].trim().to_string();
    let mut body_start = condition_start + brace_offset + 1;
    // Skip newline if `{` was at end of line
    if body_start < input.len() && input.as_bytes()[body_start] == b'\n' {
        body_start += 1;
    }
    let body_line = first_line_num + input[condition_start..body_start].matches('\n').count() as u32;
    Ok((condition_text, body_start, body_line))
}

/// Handle bare (non-`--` prefixed) if/while block parsing.
/// Supports multi-line conditions, `{` with content after, and inline `if (cond) { body; }`.
fn parse_bare_flow_block(
    ctx: &mut ParseContext,
    input: &str,
    pos: usize,
    line_len: usize,
    line_num: u32,
    cond_start: usize,
    span: Span,
    is_while: bool,
) -> Result<(Statement, usize, u32), ParseError> {
    let (condition_text, body_start, body_line) =
        scan_bare_block_brace(input, cond_start, line_num)?;

    // Check for inline block: `if (cond) { body; }` all on one line
    let same_line_end = input[body_start..].find('\n').unwrap_or(input.len() - body_start);
    if same_line_end > 0 {
        let same_line = &input[body_start..body_start + same_line_end];
        if let Some(close_pos) = same_line.rfind('}') {
            let body_text = same_line[..close_pos].trim();
            let condition = flow::parse_condition(&condition_text, ctx.config.version)?;
            let body_stmts = if body_text.is_empty() {
                vec![Statement::Empty]
            } else {
                let body_span = Span::new(line_num, 1, body_start, same_line_end);
                match parse_command_body(ctx, body_text, body_span) {
                    Ok(stmt) => vec![stmt],
                    Err(_) => vec![Statement::Sql(SqlStatement { span: body_span, sql: body_text.into() })],
                }
            };
            let stmt = if is_while {
                Statement::While(WhileBlock { span, condition, body: body_stmts })
            } else {
                Statement::If(IfBlock { span, condition, body: body_stmts })
            };
            return Ok((stmt, pos + line_len, line_num + 1));
        }
    }

    // Body continues on subsequent lines
    let condition = flow::parse_condition(&condition_text, ctx.config.version)?;
    let (body, after_body_pos, after_body_line) =
        parse_statements(ctx, input, body_start, body_line)?;
    let (end_pos, end_line) = consume_close_brace(input, after_body_pos, after_body_line)?;
    let stmt = if is_while {
        Statement::While(WhileBlock { span, condition, body })
    } else {
        Statement::If(IfBlock { span, condition, body })
    };
    Ok((stmt, end_pos, end_line))
}

/// Consume the closing `}` and return position after it.
fn consume_close_brace(input: &str, mut pos: usize, mut line_num: u32) -> Result<(usize, u32), ParseError> {
    if pos < input.len() && input.as_bytes()[pos] == b'}' {
        pos += 1;
        if pos < input.len() && input.as_bytes()[pos] == b'\n' {
            pos += 1;
            line_num += 1;
        }
    }
    Ok((pos, line_num))
}

/// Parse `--write_file filename END_MARKER ... END_MARKER`.
fn parse_write_file_block(
    args: &str,
    input: &str,
    after_first_line: usize,
    start_line: u32,
    span: Span,
) -> Result<(Statement, usize, u32), ParseError> {
    let (filename, end_marker) = parse_file_args(args)?;
    let (content, new_pos, new_ln) = read_until_end_marker(&end_marker, input, after_first_line, start_line)?;
    Ok((Statement::WriteFile(crate::ast::commands::WriteFileCmd {
        span, filename: filename.into(), end_marker, content: content.into(),
    }), new_pos, new_ln))
}

/// Parse `--append_file filename END_MARKER ... END_MARKER`.
fn parse_append_file_block(
    args: &str,
    input: &str,
    after_first_line: usize,
    start_line: u32,
    span: Span,
) -> Result<(Statement, usize, u32), ParseError> {
    let (filename, end_marker) = parse_file_args(args)?;
    let (content, new_pos, new_ln) = read_until_end_marker(&end_marker, input, after_first_line, start_line)?;
    Ok((Statement::AppendFile(crate::ast::commands::AppendFileCmd {
        span, filename: filename.into(), end_marker, content: content.into(),
    }), new_pos, new_ln))
}

/// Parse `--perl [delimiter] ... END_PERL`.
fn parse_perl_block(
    end_marker: &str,
    input: &str,
    after_first_line: usize,
    start_line: u32,
    span: Span,
) -> Result<(PerlBlock, usize, u32), ParseError> {
    let (content, new_pos, new_ln) = read_until_end_marker(end_marker, input, after_first_line, start_line)?;
    Ok((PerlBlock { span, end_marker: end_marker.to_string(), content: content.into() }, new_pos, new_ln))
}

/// Strip a trailing delimiter from the argument string, respecting quotes.
/// e.g. `"file;name" EOF;` → `"file;name" EOF` (the ; inside quotes is kept).
fn strip_trailing_delimiter_quoted<'a>(s: &'a str, delimiter: &str) -> &'a str {
    let s = s.trim();
    // Simple fast path: no quotes in the string
    if !s.contains('"') && !s.contains('\'') && !s.contains('`') {
        return s.strip_suffix(delimiter).unwrap_or(s).trim();
    }
    // Count quote nesting and only strip delimiter after the outermost close
    let mut depth = 0u32;
    let mut quote_char: char = '\0';
    let bytes = s.as_bytes();
    for i in 0..bytes.len() {
        let c = bytes[i] as char;
        if depth == 0 && (c == '"' || c == '\'' || c == '`') {
            depth = 1;
            quote_char = c;
        } else if depth > 0 && c == quote_char {
            depth = 0;
        }
    }
    if depth == 0 {
        // All quotes closed — safe to strip delimiter from the end
        s.strip_suffix(delimiter).unwrap_or(s).trim()
    } else {
        // Unclosed quotes — don't strip
        s
    }
}

/// Parse filename and END_MARKER from arguments.
fn parse_file_args(args: &str) -> Result<(String, String), ParseError> {
    let trimmed = args.trim();
    let unquoted = command::strip_quotes(trimmed);
    // If strip_quotes actually removed outer quotes, the entire content is the filename
    // (the filename may contain spaces). Only split on space if no quotes were stripped.
    let (filename, end_marker) = if unquoted.len() < trimmed.len() {
        (unquoted.trim().to_string(), "EOF".to_string())
    } else {
        let parts: Vec<&str> = unquoted.splitn(2, ' ').collect();
        let fname = parts[0].trim().to_string();
        let marker = parts.get(1).map(|s| s.trim().to_string()).unwrap_or_else(|| "EOF".to_string());
        (fname, marker)
    };
    if filename.is_empty() {
        return Err(ParseError::Syntax { message: "filename must not be empty".to_string(), span: Span::dummy() });
    }
    Ok((filename, end_marker))
}

/// Read lines until we find a line that exactly matches the end_marker.
fn read_until_end_marker(
    end_marker: &str,
    input: &str,
    mut pos: usize,
    mut line_num: u32,
) -> Result<(String, usize, u32), ParseError> {
    let mut content = String::new();
    let start_line = line_num;
    loop {
        if pos >= input.len() {
            return Err(ParseError::UnterminatedBlock {
                command: "write_file/append_file".to_string(),
                marker: end_marker.to_string(),
                span: Span::new(start_line, 1, pos, 0),
            });
        }
        let (line, _) = read_line(&input[pos..]);
        let line_end = input[pos..].find('\n').unwrap_or(input[pos..].len());
        let line_len = if line_end < input[pos..].len() { line_end + 1 } else { line_end };

        if line.trim() == end_marker {
            pos += line_len;
            line_num += 1;
            return Ok((content, pos, line_num));
        }
        if !content.is_empty() { content.push('\n'); }
        content.push_str(line);
        pos += line_len;
        line_num += 1;
    }
}

/// Parse delimiter-terminated text: accumulate lines until the delimiter is found.
fn parse_delimiter_terminated(
    ctx: &mut ParseContext,
    input: &str,
    start_pos: usize,
    start_line: u32,
    _first_span: Span,
) -> Result<(Statement, usize), ParseError> {
    let mut body = String::new();
    let mut pos = start_pos;
    loop {
        let (line, _) = read_line(&input[pos..]);
        let line_end = input[pos..].find('\n').unwrap_or(input[pos..].len());
        let line_len = if line_end < input[pos..].len() { line_end + 1 } else { line_end };
        let trimmed = line.trim_end();

        if let Some(delim_idx) = trimmed.rfind(&ctx.delimiter) {
            let before_delim = &trimmed[..delim_idx];
            let after_delim = &trimmed[delim_idx + ctx.delimiter.len()..];
            let after = after_delim.trim();
            if after.is_empty() || after.starts_with('#') {
                if !body.is_empty() { body.push('\n'); }
                body.push_str(before_delim);
                let span = Span::new(start_line, 1, start_pos, pos + line_len - start_pos);
                let stmt = parse_command_body(ctx, &body, span)?;
                return Ok((stmt, pos + line_len - start_pos));
            }
        }

        if !body.is_empty() { body.push('\n'); }
        body.push_str(line);
        pos += line_len;

        if pos >= input.len() {
            let total_len = pos - start_pos;
            let span = Span::new(start_line, 1, start_pos, total_len);
            return Ok((Statement::Sql(SqlStatement { span, sql: body.trim().into() }), total_len));
        }
    }
}

/// Parse the body of a delimiter-terminated command.
fn parse_command_body(
    ctx: &mut ParseContext,
    body: &str,
    span: Span,
) -> Result<Statement, ParseError> {
    let first_word = parse_identifier(body.trim());
    match classify_command(first_word) {
        CommandKind::Known(name) => command::parse_known_command(ctx, &name, body.trim(), span),
        CommandKind::Unknown => Ok(Statement::Sql(SqlStatement { span, sql: body.into() })),
    }
}
