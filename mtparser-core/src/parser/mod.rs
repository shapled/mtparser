//! Core parsing engine using winnow combinators.
//!
//! Entry points:
//! - [`parse`] — parse a string into a `Vec<Statement>`
//! - [`parse_bytes`] — parse raw bytes (UTF-8 lossy conversion)
//!
//! The parser uses `winnow::stream::Stateful<LocatingSlice<&str>, ParserState>`
//! as the unified stream type. `ParserState` carries version and delimiter,
//! while `LocatingSlice` tracks byte offsets for span generation.

pub mod arg;
pub mod command;
pub mod flow;

use std::ops::Range;

use winnow::Parser;
use winnow::combinator::repeat;
use winnow::stream::{LocatingSlice, Stateful, Stream as StreamTrait};
use winnow::token::{take_till, take_while};

use crate::ast::span::Span;
use crate::ast::statement::*;
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
            version: MysqlVersion::MySQL,
        }
    }
}

impl ParserConfig {
    pub fn new(version: MysqlVersion) -> Self {
        Self { version }
    }
}

/// Parsing state threaded through winnow combinators via `Stateful`.
/// Contains version (immutable), delimiter (mutable), and original source (for span computation).
#[derive(Debug, Clone)]
pub(crate) struct ParserState {
    pub(crate) version: MysqlVersion,
    pub(crate) delimiter: String,
    pub(crate) source: String,
}

impl ParserState {
    pub(crate) fn new(version: MysqlVersion, source: &str) -> Self {
        Self {
            version,
            delimiter: ";".to_string(),
            source: source.to_string(),
        }
    }
}

/// The unified stream type used throughout the parser.
pub(crate) type Stream<'s> = Stateful<LocatingSlice<&'s str>, ParserState>;

/// Convert a byte-offset range from `LocatingSlice` into an `ast::Span`.
pub(crate) fn range_to_span(stream: &Stream, range: Range<usize>) -> Span {
    let source = &stream.state.source;
    let offset = range.start.min(source.len());
    let end = range.end.min(source.len());
    let len = end - offset;
    let line = source[..offset].matches('\n').count() as u32 + 1;
    let last_nl = source[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let column = (offset - last_nl + 1) as u32;
    Span::new(line, column, offset, len)
}

/// Bridge: convert ParseError to ErrMode<Cut<ContextError>> with the message preserved as a Label.
/// Used when sub-functions return Result<_, ParseError> but the caller needs ModalResult.
pub(crate) fn parse_err_to_modal(
    e: &ParseError,
) -> winnow::error::ErrMode<winnow::error::ContextError> {
    let mut err = winnow::error::ContextError::new();
    let msg: &'static str = Box::leak(format!("{}", e).into_boxed_str());
    err.push(winnow::error::StrContext::Label(msg));
    winnow::error::ErrMode::Cut(err)
}

/// Convert a winnow error at a given span into a `ParseError`.
pub(crate) fn modal_err_to_parse_err(
    e: winnow::error::ErrMode<winnow::error::ContextError>,
    span: Span,
    context: &str,
) -> ParseError {
    ParseError::Syntax {
        message: format!("{}: {}", context, e),
        span,
    }
}

/// Create a span at the current stream offset with zero length (used as placeholder).
fn span_for_current_offset(_stream: &Stream) -> Span {
    // We can't get current_token_start easily without Location trait,
    // so use a simple approximation
    Span::dummy()
}

/// Parse a complete test file into an AST.
pub fn parse_bytes(input: &[u8], config: ParserConfig) -> Result<Vec<Statement>, ParseError> {
    let text = String::from_utf8_lossy(input);
    parse(&text, config)
}

/// Parse a complete test file into an AST.
pub fn parse(input: &str, config: ParserConfig) -> Result<Vec<Statement>, ParseError> {
    let state = ParserState::new(config.version, input);
    let mut stream = Stream {
        input: LocatingSlice::new(input),
        state,
    };
    parse_statements(&mut stream)
        .map_err(|e| modal_err_to_parse_err(e, Span::dummy(), "parse_statements"))
}

/// Parse one line from the stream, consuming the line ending.
/// Advances the stream past the line and its newline.
#[allow(dead_code)]
fn parse_line<'s>(stream: &mut Stream<'s>) -> winnow::ModalResult<&'s str> {
    let line = take_till(1.., ['\r', '\n']).parse_next(stream)?;
    // Consume optional \r\n or \n
    let _ = take_while(0.., ['\r', '\n']).parse_next(stream)?;
    Ok(line)
}

/// Parse one line including leading whitespace, returning trimmed content and the span range.
fn peek_line_trimmed<'s>(stream: &mut Stream<'s>) -> winnow::ModalResult<(&'s str, Range<usize>)> {
    // Take content until \n, then consume exactly one \n (not all of them)
    let (line_content, range) = take_till(0.., ['\r', '\n'])
        .with_span()
        .parse_next(stream)?;
    // Consume exactly one newline character (\n or \r\n)
    // Consume exactly one newline character (\n or \r\n)
    let _: winnow::ModalResult<char> = winnow::token::one_of(['\n']).parse_next(stream);
    let _: winnow::ModalResult<char> = winnow::token::one_of(['\r']).parse_next(stream);
    let trimmed = line_content.trim();
    // Adjust range start to skip leading whitespace
    let leading = line_content.len() - trimmed.len();
    // Range end should be the end of visible content, not including newline
    let content_end = range.start + line_content.len();
    let adjusted_range = (range.start + leading)..content_end;
    Ok((trimmed, adjusted_range))
}

/// Parse one line returning the original (untrimmed) content.
/// Used for delimiter-terminated SQL and file content where leading whitespace matters.
fn read_line_original<'s>(stream: &mut Stream<'s>) -> winnow::ModalResult<&'s str> {
    let line = take_till(0.., ['\r', '\n']).parse_next(stream)?;
    // Consume optional \r\n or \n
    let _ = take_while(0.., ['\r', '\n']).parse_next(stream)?;
    Ok(line)
}

/// Extract an identifier (alphanumeric + underscore + hyphen) as a plain string.
/// Used for quick lookahead before dispatch — not a stream combinator.
fn parse_identifier(s: &str) -> &str {
    let mut stream: &str = s;
    let result: winnow::ModalResult<&str> =
        take_while(1.., ('a'..='z', 'A'..='Z', '0'..='9', '_', '-')).parse_next(&mut stream);
    result.unwrap_or("")
}

/// Internal: parse a sequence of statements from the stream until `}` or EOF.
/// Parse a sequence of statements using `repeat(0.., parse_one_line)`.
/// Stops at `}`, `--}`, `-- }`, or EOF.
fn parse_statements(stream: &mut Stream) -> winnow::ModalResult<Vec<Statement>> {
    repeat(0.., parse_statement).parse_next(stream)
}

/// Parse a single statement from the stream.
/// Returns `Backtrack` when encountering `}`, `--}`, or EOF (stops `repeat`).
/// Returns `Cut` on parse errors (fatal — stops parsing immediately).
fn parse_statement(stream: &mut Stream) -> winnow::ModalResult<Statement> {
    // EOF → stop
    if stream.input.is_empty() {
        return Err(winnow::error::ErrMode::Backtrack(
            winnow::error::ContextError::new(),
        ));
    }

    // Closing brace check → stop (bare } or --})
    // Only check the FIRST LINE, not across newlines
    let remaining = *stream.input;
    let line_end = remaining.find('\n').unwrap_or(remaining.len());
    let first_line = &remaining[..line_end];
    let trimmed_first = first_line.trim_start();
    if trimmed_first.starts_with('}') || trimmed_first == "--}" || trimmed_first == "-- }" {
        return Err(winnow::error::ErrMode::Backtrack(
            winnow::error::ContextError::new(),
        ));
    }

    // Save checkpoint
    let start = StreamTrait::checkpoint(&stream.input);

    // Read the trimmed line for classification
    let (trimmed, range) = peek_line_trimmed(stream)
        .map_err(|_| winnow::error::ErrMode::Backtrack(winnow::error::ContextError::new()))?;
    let span = range_to_span(stream, range);

    // Empty line
    if trimmed.is_empty() {
        return Ok(Statement::Empty);
    }

    // Comment
    if let Some(comment_text) = trimmed.strip_prefix('#') {
        return Ok(Statement::Comment(CommentNode {
            span,
            text: comment_text.into(),
        }));
    }

    // -- prefix: consume "--" + ws, then parse_command
    if trimmed.starts_with("--") {
        let rest = trimmed.strip_prefix("--").unwrap().trim_start();
        if rest == "}" {
            return Err(winnow::error::ErrMode::Backtrack(
                winnow::error::ContextError::new(),
            ));
        }
        // Reset to line start. Wrap -- consumption + parse_command in with_span
        // so span includes the -- prefix. Trailing newline is consumed outside.
        StreamTrait::reset(&mut stream.input, &start);
        let (mut stmt, range) = (|s: &mut Stream| -> winnow::ModalResult<Statement> {
            let _ = (
                take_while(0.., [' ', '\t']),
                "--",
                take_while(0.., [' ', '\t']),
            )
                .parse_next(s)?;
            command::parse_command(s)
        })
        .with_span()
        .parse_next(stream)
        .map_err(|e| e.cut())?;

        // Consume trailing newline (outside span)
        let _: &str = take_while(0.., ['\r', '\n']).parse_next(stream)?;

        stmt.set_span(range_to_span(stream, range));
        return Ok(stmt);
    }

    // Non-`--`: only special bare keywords are commands, everything else is SQL
    let first_word = parse_identifier(trimmed);
    let is_bare_cmd = matches!(
        first_word.to_ascii_lowercase().as_str(),
        "if" | "while" | "write_file" | "append_file" | "perl" | "delimiter"
    );

    if is_bare_cmd {
        StreamTrait::reset(&mut stream.input, &start);
        let mut stmt = command::parse_command(stream).map_err(|e| e.cut())?;
        // Consume trailing newline (outside span)
        let _: &str = take_while(0.., ['\r', '\n']).parse_next(stream)?;
        Ok(stmt)
    } else {
        StreamTrait::reset(&mut stream.input, &start);
        parse_delimiter_terminated(stream, span).map_err(|e| parse_err_to_modal(&e))
    }
}

/// Classification result for a command name.
pub(crate) enum CommandKind {
    Known,
    Unknown,
}

/// Classify a command name. Returns `Known(lowercase_name)` for recognized commands,
/// `Unknown` otherwise.
pub(crate) fn classify_command(word: &str) -> CommandKind {
    let lower = word.to_ascii_lowercase();
    match lower.as_str() {
        "echo"
        | "let"
        | "error"
        | "source"
        | "skip"
        | "die"
        | "exit"
        | "exec"
        | "execw"
        | "exec_in_background"
        | "sleep"
        | "inc"
        | "dec"
        | "assert"
        | "expr"
        | "connect"
        | "connection"
        | "disconnect"
        | "change_user"
        | "reset_connection"
        | "query"
        | "eval"
        | "send"
        | "send_eval"
        | "reap"
        | "output"
        | "horizontal_results"
        | "query_horizontal"
        | "vertical_results"
        | "replace_result"
        | "replace_column"
        | "replace_regex"
        | "sorted_result"
        | "partially_sorted_result"
        | "replace_numeric_round"
        | "delimiter"
        | "write_file"
        | "append_file"
        | "remove_file"
        | "remove_files_wildcard"
        | "copy_file"
        | "move_file"
        | "mkdir"
        | "rmdir"
        | "chmod"
        | "diff_files"
        | "file_exists"
        | "cat_file"
        | "list_files"
        | "shutdown_server"
        | "send_quit"
        | "send_shutdown"
        | "if"
        | "while"
        | "end"
        | "perl"
        | "character_set"
        | "system"
        | "real_sleep"
        | "require"
        | "lowercase_result"
        | "sync_slave_with_master"
        | "copy_files_wildcard"
        | "query_vertical"
        | "result_format"
        | "query_attributes"
        | "list_files_write_file"
        | "list_files_append_file"
        | "force-rmdir"
        | "force-cpdir"
        | "save_master_pos"
        | "sync_with_master"
        | "wait_for_slave_to_stop"
        | "skip_if_hypergraph"
        | "evalp"
        | "write_line"
        | "dirty_close"
        | "ping"
        | "disable_warnings"
        | "enable_warnings"
        | "disable_query_log"
        | "enable_query_log"
        | "disable_result_log"
        | "enable_result_log"
        | "disable_info"
        | "enable_info"
        | "disable_metadata"
        | "enable_metadata"
        | "disable_ps_protocol"
        | "enable_ps_protocol"
        | "disable_reconnect"
        | "enable_reconnect"
        | "disable_connect_log"
        | "enable_connect_log"
        | "disable_session_track_info"
        | "enable_session_track_info"
        | "disable_testcase"
        | "enable_testcase"
        | "disable_parsing"
        | "enable_parsing"
        | "disable_async_client"
        | "enable_async_client"
        | "disable_prepare_warnings"
        | "enable_prepare_warnings"
        | "disable_abort_on_error"
        | "enable_abort_on_error"
        | "disable_cursor_protocol"
        | "enable_cursor_protocol"
        | "disable_non_blocking_api"
        | "enable_non_blocking_api"
        | "disable_ps2_protocol"
        | "enable_ps2_protocol"
        | "disable_service_connection"
        | "enable_service_connection"
        | "disable_view_protocol"
        | "enable_view_protocol"
        | "disable_column_names"
        | "enable_column_names"
        | "ps_prepare"
        | "ps_bind"
        | "ps_execute"
        | "ps_close"
        | "optimizer_trace" => CommandKind::Known,
        _ => CommandKind::Unknown,
    }
}

/// Parse filename and END_MARKER from arguments.
/// Uses winnow `parse_quoted_arg` for quote-wrapped filenames.
fn parse_file_args(args: &str) -> Result<(String, String), ParseError> {
    let mut stream: &str = args.trim();

    // Try quoted form: "filename" or 'filename' or `filename`
    if let Ok(content) = command::parse_quoted_arg(&mut stream) {
        let rest = stream.trim();
        let end_marker = if rest.is_empty() {
            "EOF".to_string()
        } else {
            rest.to_string()
        };
        if content.trim().is_empty() {
            return Err(ParseError::Syntax {
                message: "filename must not be empty".to_string(),
                span: Span::dummy(),
            });
        }
        return Ok((content.trim().to_string(), end_marker));
    }

    // Unquoted form: filename [end_marker]
    let filename = parse_filename_part(args.trim());
    let rest = args.trim();
    let after_name = &rest[filename.len()..].trim_start();
    let end_marker = if after_name.is_empty() {
        "EOF".to_string()
    } else {
        after_name.to_string()
    };
    if filename.is_empty() {
        return Err(ParseError::Syntax {
            message: "filename must not be empty".to_string(),
            span: Span::dummy(),
        });
    }
    Ok((filename, end_marker))
}

/// Extract the filename part (until whitespace) using winnow `take_till`.
fn parse_filename_part(input: &str) -> String {
    let mut stream: &str = input;
    parse_filename_part_inner(&mut stream)
        .unwrap_or("")
        .to_string()
}

/// Winnow parser: consume filename characters (non-whitespace).
fn parse_filename_part_inner<'s>(input: &mut &'s str) -> winnow::ModalResult<&'s str> {
    take_till(1.., [' ', '\t', '\n', '\r']).parse_next(input)
}

/// Read lines from the stream until we find a line that exactly matches the end_marker.
fn read_until_end_marker(stream: &mut Stream, end_marker: &str) -> Result<String, ParseError> {
    let mut content = String::new();
    let mut start_span = Span::dummy();

    loop {
        if stream.input.is_empty() {
            return Err(ParseError::UnterminatedBlock {
                command: "write_file/append_file".to_string(),
                marker: end_marker.to_string(),
                span: start_span,
            });
        }
        // Get original (untrimmed) line for content, but trim for marker comparison
        let line = read_line_original(stream)
            .map_err(|e| modal_err_to_parse_err(e, start_span, "read_until_end_marker"))?;
        if start_span == Span::dummy() {
            start_span = span_for_current_offset(stream);
        }

        if line.trim() == end_marker {
            return Ok(content);
        }
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(line);
    }
}

/// Parse delimiter-terminated text: accumulate lines until the delimiter is found.
fn parse_delimiter_terminated(stream: &mut Stream, span: Span) -> Result<Statement, ParseError> {
    let mut body = String::new();
    let mut start_range: Option<Range<usize>> = None;

    loop {
        if stream.input.is_empty() {
            let total_span = start_range
                .as_ref()
                .map(|r| range_to_span(stream, r.clone()))
                .unwrap_or(span);
            return Ok(Statement::Sql(SqlStatement {
                span: total_span,
                sql: body.trim().into(),
            }));
        }
        // Use parse_line to get the original (untrimmed) line content
        let line = parse_line(stream)
            .map_err(|e| modal_err_to_parse_err(e, span, "parse_delimiter_terminated"))?;
        if start_range.is_none() {
            // Record approximate start — we consumed the line so we use the
            // span from the caller
            start_range = Some(span.offset..span.offset + span.len);
        }
        let trimmed = line.trim_end();

        if let Some(delim_idx) = trimmed.rfind(&stream.state.delimiter) {
            let before_delim = &trimmed[..delim_idx];
            let after_delim = &trimmed[delim_idx + stream.state.delimiter.len()..];
            let after = after_delim.trim();
            if after.is_empty() || after.starts_with('#') {
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(before_delim);
                let total_span = start_range
                    .as_ref()
                    .map(|r| range_to_span(stream, r.clone()))
                    .unwrap_or(span);
                return parse_command_body(stream, &body, total_span);
            }
        }

        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(line);
    }
}

/// Parse the body of a delimiter-terminated command.
fn parse_command_body(
    stream: &mut Stream,
    body: &str,
    span: Span,
) -> Result<Statement, ParseError> {
    let first_word = parse_identifier(body.trim());
    match classify_command(first_word) {
        CommandKind::Known => {
            // Build a sub-stream for parse_command.
            // Delimiter is already stripped by parse_delimiter_terminated,
            // so set it to empty string — this tells arg_rest to read to
            // end of input (supports multi-line bare command bodies).
            let real_delim = stream.state.delimiter.clone();
            let mut sub_stream = Stream {
                input: LocatingSlice::new(body.trim()),
                state: stream.state.clone(),
            };
            sub_stream.state.delimiter.clear();
            let stmt = command::parse_command(&mut sub_stream)
                .map_err(|e| modal_err_to_parse_err(e, span, "command body"))?;
            // Preserve real delimiter (sub_stream's was cleared)
            stream.state.delimiter = real_delim;
            Ok(stmt)
        }
        CommandKind::Unknown => Ok(Statement::Sql(SqlStatement {
            span,
            sql: body.into(),
        })),
    }
}

// ---------------------------------------------------------------------------
// Stream-based multi-line command parsers (called from command::dispatch!)
// ---------------------------------------------------------------------------

/// Parse if/while block from stream positioned AFTER the command name.
/// Reads condition text (rest of line), finds `{`, parses body, consumes `}`.
pub(crate) fn parse_cmd_if_args(
    stream: &mut Stream,
    is_while: bool,
) -> winnow::ModalResult<Statement> {
    let span = Span::dummy();

    // Read condition text from rest of current line
    let condition_line = take_till(0.., ['\r', '\n']).parse_next(stream)?;

    // Check if { is on the same line
    if let Some(brace_pos) = condition_line.find('{') {
        let condition_text = condition_line[..brace_pos].trim();
        let after_brace = condition_line[brace_pos + 1..].trim();

        // Check for inline block: if (cond) { body; }
        if let Some(close_pos) = after_brace.rfind('}') {
            let body_text = after_brace[..close_pos].trim();
            let mut cond_stream = Stream {
                input: LocatingSlice::new(condition_text),
                state: stream.state.clone(),
            };
            let condition = crate::parser::flow::parse_condition_stream(&mut cond_stream)?;
            let body = if body_text.is_empty() {
                vec![Statement::Empty]
            } else {
                vec![Statement::Sql(SqlStatement {
                    span,
                    sql: body_text.into(),
                })]
            };
            // Consume newline after the inline block
            let _: &str = take_while(0.., ['\r', '\n']).parse_next(stream)?;
            return Ok(if is_while {
                Statement::While(WhileBlock {
                    span,
                    condition,
                    body,
                })
            } else {
                Statement::If(IfBlock {
                    span,
                    condition,
                    body,
                })
            });
        }

        // { on same line but no } — multi-line body
        let mut cond_stream = Stream {
            input: LocatingSlice::new(condition_text),
            state: stream.state.clone(),
        };
        let condition = crate::parser::flow::parse_condition_stream(&mut cond_stream)?;
        // Consume newline
        let _: &str = take_while(0.., ['\r', '\n']).parse_next(stream)?;
        let body = parse_statements(stream)?;
        consume_close_brace_modal(stream)?;
        return Ok(if is_while {
            Statement::While(WhileBlock {
                span,
                condition,
                body,
            })
        } else {
            Statement::If(IfBlock {
                span,
                condition,
                body,
            })
        });
    }

    // { not on this line — scan subsequent lines
    let mut condition_parts = vec![condition_line.trim().to_string()];
    let _: &str = take_while(0.., ['\r', '\n']).parse_next(stream)?;
    let mut found_brace = false;

    while !stream.input.is_empty() {
        let remaining = *stream.input;
        let trimmed = remaining.trim_start();
        if let Some(brace_pos) = trimmed.find('{') {
            found_brace = true;
            let before = trimmed[..brace_pos].trim();
            if !before.is_empty() {
                condition_parts.push(before.to_string());
            }
            // Consume up to and including {
            let skip = remaining.len() - trimmed.len() + brace_pos + 1;
            for _ in 0..skip {
                let _: winnow::ModalResult<char> = winnow::token::any.parse_next(stream);
            }
            let _: &str = take_while(0.., ['\r', '\n']).parse_next(stream)?;
            break;
        }
        // No { on this line — part of condition
        let (line, _) = peek_line_trimmed(stream).unwrap_or(("", 0..0));
        condition_parts.push(line.to_string());
        let _ = consume_line_modal(stream); // Ignore error at EOF
    }

    if !found_brace {
        return Err(winnow::error::ErrMode::Cut({
            let mut err = winnow::error::ContextError::new();
            err.push(winnow::error::StrContext::Label(
                "unterminated flow control 'if/while'",
            ));
            err
        }));
    }

    let full_condition = condition_parts.join(" ");
    let mut cond_stream = Stream {
        input: LocatingSlice::new(&full_condition),
        state: stream.state.clone(),
    };
    let condition = crate::parser::flow::parse_condition_stream(&mut cond_stream)?;
    let body = parse_statements(stream)?;
    consume_close_brace_modal(stream)?;
    Ok(if is_while {
        Statement::While(WhileBlock {
            span,
            condition,
            body,
        })
    } else {
        Statement::If(IfBlock {
            span,
            condition,
            body,
        })
    })
}

/// Parse write_file from stream positioned AFTER the command name.
pub(crate) fn parse_cmd_write_file_args(stream: &mut Stream) -> winnow::ModalResult<Statement> {
    let span = Span::dummy();
    let delim = stream.state.delimiter.clone();
    let args = take_till(0.., ['\r', '\n']).parse_next(stream)?;
    let args = args
        .trim()
        .strip_suffix(delim.as_str())
        .unwrap_or(args.trim())
        .trim();
    let _: &str = take_while(0.., ['\r', '\n']).parse_next(stream)?;
    let (filename, end_marker) = parse_file_args(args).map_err(|e| parse_err_to_modal(&e))?;
    let content = read_until_end_marker(stream, &end_marker).map_err(|e| parse_err_to_modal(&e))?;
    Ok(Statement::WriteFile(crate::ast::commands::WriteFileCmd {
        span,
        filename: filename.into(),
        end_marker,
        content: content.into(),
    }))
}

/// Parse append_file from stream positioned AFTER the command name.
pub(crate) fn parse_cmd_append_file_args(stream: &mut Stream) -> winnow::ModalResult<Statement> {
    let span = Span::dummy();
    let delim = stream.state.delimiter.clone();
    let args = take_till(0.., ['\r', '\n']).parse_next(stream)?;
    let args = args
        .trim()
        .strip_suffix(delim.as_str())
        .unwrap_or(args.trim())
        .trim();
    let _: &str = take_while(0.., ['\r', '\n']).parse_next(stream)?;
    let (filename, end_marker) = parse_file_args(args).map_err(|e| parse_err_to_modal(&e))?;
    let content = read_until_end_marker(stream, &end_marker).map_err(|e| parse_err_to_modal(&e))?;
    Ok(Statement::AppendFile(crate::ast::commands::AppendFileCmd {
        span,
        filename: filename.into(),
        end_marker,
        content: content.into(),
    }))
}

/// Parse perl block from stream positioned AFTER the command name.
pub(crate) fn parse_cmd_perl_args(stream: &mut Stream) -> winnow::ModalResult<Statement> {
    let span = Span::dummy();
    let delim = stream.state.delimiter.clone();
    let args = take_till(0.., ['\r', '\n']).parse_next(stream)?;
    let args = args
        .trim()
        .strip_suffix(delim.as_str())
        .unwrap_or(args.trim())
        .trim();
    let end_marker = if args.is_empty() {
        "EOF".to_string()
    } else {
        args.to_string()
    };
    let _: &str = take_while(0.., ['\r', '\n']).parse_next(stream)?;
    let content = read_until_end_marker(stream, &end_marker).map_err(|e| parse_err_to_modal(&e))?;
    Ok(Statement::Perl(PerlBlock {
        span,
        end_marker,
        content,
    }))
}

/// Consume closing brace (ModalResult version).
fn consume_close_brace_modal(stream: &mut Stream) -> winnow::ModalResult<()> {
    let remaining = *stream.input;
    if remaining.trim_start().starts_with('}') {
        let _: &str = take_while(0.., [' ', '\t']).parse_next(stream)?;
        let _: char = winnow::token::one_of('}').parse_next(stream)?;
        let _: &str = take_while(0.., ['\r', '\n']).parse_next(stream)?;
    }
    Ok(())
}

/// Consume one line (ModalResult version).
fn consume_line_modal(stream: &mut Stream) -> winnow::ModalResult<()> {
    let _: &str = take_till(1.., ['\r', '\n']).parse_next(stream)?;
    let _: &str = take_while(0.., ['\r', '\n']).parse_next(stream)?;
    Ok(())
}
