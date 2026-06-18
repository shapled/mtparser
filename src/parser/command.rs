//! Command-specific parsers using winnow combinators.
//!
//! All command argument parsing is implemented as winnow parser functions
//! following the signature `fn(&mut &str) -> ModalResult<T>`.

use winnow::combinator::{alt, cut_err, delimited, opt, preceded, separated};
use winnow::token::{one_of, take_till, take_while};
use winnow::Parser;

use crate::ast::commands::*;
use crate::ast::statement::*;
use crate::ast::text::InterpolatedText;
use crate::ast::Span;
use crate::error::ParseError;
use crate::parser::{modal_err_to_parse_err, Stream};
use crate::version::MysqlVersion;

// ---------------------------------------------------------------------------
// Shared primitive combinators — reusable across commands with common patterns
// ---------------------------------------------------------------------------

/// Skip zero or more ASCII whitespace characters (space, tab).
fn ws<'s>(input: &mut &'s str) -> winnow::ModalResult<&'s str> {
    take_while(0.., [' ', '\t']).parse_next(input)
}

/// Take the remaining input, trimmed of leading/trailing whitespace. Infallible.
fn take_rest<'s>(input: &mut &'s str) -> &'s str {
    let _: winnow::ModalResult<&str> = take_while(0.., [' ', '\t']).parse_next(input);
    let rest = *input;
    *input = "";
    rest.trim_end()
}

/// Take the remaining input, returning `None` if empty after trimming.
fn take_rest_opt<'s>(input: &mut &'s str) -> Option<&'s str> {
    let rest = take_rest(input);
    if rest.is_empty() { None } else { Some(rest) }
}

/// Parse a `$variable_name`, returning the variable name without the `$` prefix.
fn dollar_var<'s>(input: &mut &'s str) -> winnow::ModalResult<&'s str> {
    preceded(
        '$',
        cut_err(take_while(1.., ('a'..='z', 'A'..='Z', '0'..='9', '_'))),
    )
    .parse_next(input)
}

/// Parse a possibly quote-wrapped argument, returning the inner content.
/// Quotes recognized: `'`, `"`, `` ` ``.
pub(crate) fn parse_quoted_arg<'s>(input: &mut &'s str) -> winnow::ModalResult<&'s str> {
    let open = one_of(['\'', '"', '`']).parse_next(input)?;
    let content = take_till(1.., [open]).parse_next(input)?;
    one_of([open]).parse_next(input)?; // consume closing quote
    Ok(content)
}

/// Parse a single filename token: either a quoted string or non-space text.
fn single_token<'s>(input: &mut &'s str) -> winnow::ModalResult<&'s str> {
    alt((
        parse_quoted_arg,
        take_till(1.., [' ', '\t']),
    ))
    .parse_next(input)
}

/// Parse whitespace-separated tokens. Returns all tokens in the input.
fn ws_tokens<'s>(input: &mut &'s str) -> winnow::ModalResult<Vec<&'s str>> {
    separated(0.., take_till(1.., [' ', '\t', '\n', '\r']), take_while(1.., [' ', '\t']))
        .parse_next(input)
}

/// Parse a comma-separated list of elements, each trimmed of surrounding whitespace.
fn comma_list<'s, F>(elem: F) -> impl winnow::Parser<&'s str, Vec<&'s str>, winnow::error::ErrMode<winnow::error::ContextError>>
where
    F: winnow::Parser<&'s str, &'s str, winnow::error::ErrMode<winnow::error::ContextError>>,
{
    separated(
        0..,
        delimited(ws, elem, ws),
        ',',
    )
}

/// Parse `name = value` assignment, returning (name, value).
/// Name extends until `=` or whitespace; optional leading `$` is stripped.
fn name_eq_value<'s>(input: &mut &'s str) -> winnow::ModalResult<(&'s str, &'s str)> {
    let _ = ws(input)?;
    let name = take_till(1.., ['=', ' ', '\t']).parse_next(input)?;
    let _ = ws(input)?;
    one_of('=').parse_next(input)?;
    let value = take_rest(input);
    Ok((name, value))
}

/// Chunk whitespace-separated tokens into pairs.
fn paired_tokens<'s>(input: &mut &'s str) -> winnow::ModalResult<Vec<(&'s str, &'s str)>> {
    let tokens = ws_tokens(input)?;
    let pairs: Vec<(&str, &str)> = tokens.chunks_exact(2)
        .filter_map(|chunk| Some((chunk.first().copied()?, chunk.get(1).copied()?)))
        .collect();
    Ok(pairs)
}

/// Chunk whitespace-separated tokens into triples.
fn triple_tokens<'s>(input: &mut &'s str) -> winnow::ModalResult<Vec<(&'s str, &'s str, &'s str)>> {
    let tokens = ws_tokens(input)?;
    let triples: Vec<(&str, &str, &str)> = tokens.chunks_exact(3)
        .filter_map(|chunk| Some((chunk.first().copied()?, chunk.get(1).copied()?, chunk.get(2).copied()?)))
        .collect();
    Ok(triples)
}

/// Helper: run a parser on `args`, converting ModalResult to ParseError.
fn run<'a, P, O>(parser: P, args: &'a str, span: Span, ctx: &str) -> Result<O, ParseError>
where
    P: winnow::Parser<&'a str, O, winnow::error::ErrMode<winnow::error::ContextError>>,
{
    let mut s = args;
    let mut parser = parser;
    parser.parse_next(&mut s).map_err(|e| modal_err_to_parse_err(e, span, ctx))
}

// ---------------------------------------------------------------------------
// Command dispatcher
// ---------------------------------------------------------------------------

/// Parse a known command by name.
pub(crate) fn parse_known_command(
    stream: &mut Stream,
    name: &str,
    input: &str,
    span: Span,
) -> Result<Statement, ParseError> {
    // Check version compatibility for version-specific commands
    if !stream.state.version.has_command(name) {
        return Err(ParseError::VersionMismatch {
            command: name.to_string(),
            version: format!("{:?}", stream.state.version),
            span,
        });
    }

    let args = skip_command_name(name, input);
    let mut s: &str = args;

    match name {
        "echo" => Ok(Statement::Echo(EchoCmd { span, text: take_rest(&mut s).into() })),
        "source" => {
            let _ = ws(&mut s);
            let file = single_token(&mut s).unwrap_or("");
            Ok(Statement::Source(SourceCmd { span, file: file.into() }))
        }
        "skip" => Ok(Statement::Skip(SkipCmd { span, message: take_rest_opt(&mut s).map(|m| m.into()) })),
        "die" => Ok(Statement::Die(DieCmd { span, message: take_rest_opt(&mut s).map(|m| m.into()) })),
        "exit" => Ok(Statement::Exit(ExitCmd { span })),
        "exec" => Ok(Statement::Exec(ExecCmd { span, command: take_rest(&mut s).into() })),
        "execw" => Ok(Statement::Execw(ExecwCmd { span, command: take_rest(&mut s).into() })),
        "exec_in_background" => Ok(Statement::ExecInBackground(ExecInBackgroundCmd { span, command: take_rest(&mut s).into() })),
        "sleep" => Ok(Statement::Sleep(SleepCmd { span, seconds: take_rest(&mut s).to_string() })),
        "reap" => Ok(Statement::Reap(ReapCmd { span })),
        "output" => {
            let file = strip_quotes(take_rest(&mut s));
            Ok(Statement::Output(OutputCmd { span, file: file.into() }))
        }

        "let" => {
            let result = name_eq_value(&mut s);
            let (var, value) = match result {
                Ok(v) => v,
                Err(_) => return Err(ParseError::Syntax { message: format!("invalid let syntax: {}", args), span }),
            };
            let var = var.strip_prefix('$').unwrap_or(var);
            if var.is_empty() {
                return Err(ParseError::Syntax { message: format!("invalid let syntax: {}", args), span });
            }
            if let Some(query) = value.strip_prefix('`').and_then(|v| v.strip_suffix('`')) {
                Ok(Statement::Let(LetCmd {
                    span,
                    variable: var.to_string(),
                    value: LetValue::Query(crate::ast::expr::QueryExpr::new(Span::dummy(), query.to_string())),
                }))
            } else {
                Ok(Statement::Let(LetCmd { span, variable: var.to_string(), value: LetValue::Literal(value.to_string()) }))
            }
        }
        "inc" => {
            let var = dollar_var(&mut s).unwrap_or("");
            if var.is_empty() {
                return Err(ParseError::Syntax { message: "inc requires a variable name ($var)".to_string(), span });
            }
            Ok(Statement::Inc(IncCmd { span, variable: var.to_string() }))
        }
        "dec" => {
            let var = dollar_var(&mut s).unwrap_or("");
            if var.is_empty() {
                return Err(ParseError::Syntax { message: "dec requires a variable name ($var)".to_string(), span });
            }
            Ok(Statement::Dec(DecCmd { span, variable: var.to_string() }))
        }
        "expr" => {
            let var = dollar_var(&mut s).map_err(|e| modal_err_to_parse_err(e, span, "expr"))?;
            let _: winnow::ModalResult<Option<char>> = opt(one_of('=')).parse_next(&mut s);
            let expression = take_rest(&mut s).to_string();
            if expression.is_empty() {
                return Err(ParseError::Syntax { message: format!("invalid expr syntax: {}", args), span });
            }
            Ok(Statement::Expr(ExprCmd { span, variable: var.to_string(), expression }))
        }
        "error" => {
            let elems = run(comma_list(take_till(1.., [','])), args, span, "error")?;
            let codes: Vec<InterpolatedText> = elems.into_iter().map(Into::into).collect();
            Ok(Statement::Error(ErrorCmd { span, error_codes: codes }))
        }

        "connect" => {
            let _ = ws(&mut s);
            // Try parenthesized form: (name, host, ...)
            let paren_result: winnow::ModalResult<Vec<&str>> = delimited(
                one_of('('),
                comma_list(take_till(0.., [',', ')'])),
                one_of(')'),
            ).parse_next(&mut s);
            match paren_result {
                Ok(parts) => {
                    let name = parts.first().map(|s| (*s).trim().into());
                    let mut params = ConnectParams::default();
                    if let Some(h) = parts.get(1) { params.host = Some(h.trim().into()); }
                    if let Some(u) = parts.get(2) { params.user = Some(u.trim().into()); }
                    if let Some(p) = parts.get(3) { params.password = Some(p.trim().into()); }
                    if let Some(d) = parts.get(4) { params.database = Some(d.trim().into()); }
                    if let Some(p) = parts.get(5) { params.port = Some(p.trim().into()); }
                    if let Some(so) = parts.get(6) { params.socket = Some(so.trim().into()); }
                    Ok(Statement::Connect(ConnectCmd { span, name, params }))
                }
                Err(_) => {
                    let mut s2: &str = args;
                    let name = take_rest(&mut s2);
                    Ok(Statement::Connect(ConnectCmd { span, name: Some(name.into()), params: ConnectParams::default() }))
                }
            }
        }
        "connection" => Ok(Statement::Connection(ConnectionCmd { span, name: take_rest(&mut s).into() })),
        "disconnect" => Ok(Statement::Disconnect(DisconnectCmd { span, name: take_rest(&mut s).into() })),
        "change_user" => {
            let elems = run(comma_list(take_till(1.., [','])), args, span, "change_user")?;
            let get = |i: usize| elems.get(i).map(|s| (*s).trim()).filter(|s| !s.is_empty()).map(|s| s.into());
            Ok(Statement::ChangeUser(ChangeUserCmd {
                span,
                user: get(0),
                password: get(1),
                database: get(2),
            }))
        }
        "reset_connection" => Ok(Statement::ResetConnection(ResetConnectionCmd { span })),

        "query" => Ok(Statement::Query(QueryCmd { span, sql: take_rest(&mut s).into() })),
        "eval" => Ok(Statement::Eval(EvalCmd { span, sql: take_rest(&mut s).into() })),
        "send" => Ok(Statement::Send(SendCmd { span, sql: take_rest(&mut s).into() })),
        "send_eval" => Ok(Statement::SendEval(SendEvalCmd { span, sql: take_rest(&mut s).into() })),

        "horizontal_results" | "query_horizontal" => Ok(Statement::HorizontalResults(HorizontalResultsCmd { span })),
        "vertical_results" => Ok(Statement::VerticalResults(VerticalResultsCmd { span })),
        "sorted_result" => Ok(Statement::SortedResult(SortedResultCmd { span })),
        "replace_result" => {
            let pairs = run(paired_tokens, args, span, "replace_result")?;
            let replacements = pairs.into_iter().map(|(a, b)| (a.into(), b.into())).collect();
            Ok(Statement::ReplaceResult(ReplaceResultCmd { span, replacements }))
        }
        "replace_column" => {
            let triples = run(triple_tokens, args, span, "replace_column")?;
            let replacements = triples.into_iter().map(|(c, o, n)| ReplaceColumnItem {
                column: c.to_string(), old_value: o.into(), new_value: n.into(),
            }).collect();
            Ok(Statement::ReplaceColumn(ReplaceColumnCmd { span, replacements }))
        }
        "replace_regex" => parse_replace_regex(stream, args, span),
        "partially_sorted_result" => Ok(Statement::PartiallySortedResult(PartiallySortedResultCmd { span, columns: take_rest(&mut s).to_string() })),
        "replace_numeric_round" => Ok(Statement::ReplaceNumericRound(ReplaceNumericRoundCmd { span, decimals: take_rest(&mut s).to_string() })),

        "disable_warnings" | "enable_warnings"
        | "disable_query_log" | "enable_query_log"
        | "disable_result_log" | "enable_result_log"
        | "disable_info" | "enable_info"
        | "disable_metadata" | "enable_metadata"
        | "disable_ps_protocol" | "enable_ps_protocol"
        | "disable_reconnect" | "enable_reconnect"
        | "disable_connect_log" | "enable_connect_log"
        | "disable_session_track_info" | "enable_session_track_info"
        | "disable_testcase" | "enable_testcase"
        | "disable_abort_on_error" | "enable_abort_on_error"
        | "disable_parsing" | "enable_parsing"
        | "disable_async_client" | "enable_async_client"
        | "disable_prepare_warnings" | "enable_prepare_warnings"
        | "disable_cursor_protocol" | "disable_non_blocking_api"
        | "disable_ps2_protocol" | "disable_service_connection"
        | "disable_view_protocol" | "disable_column_names"
        | "enable_non_blocking_api"
        | "enable_ps2_protocol" | "enable_cursor_protocol"
        | "enable_service_connection" | "enable_view_protocol"
        | "enable_column_names" => {
            parse_toggle(name, args, span)
        }

        // No-arg commands
        "query_vertical" => Ok(Statement::QueryVertical(QueryVerticalCmd { span, sql: take_rest(&mut s).into() })),
        "save_master_pos" => Ok(Statement::SaveMasterPos(SaveMasterPosCmd { span })),
        "wait_for_slave_to_stop" => Ok(Statement::WaitForSlaveToStop(WaitForSlaveToStopCmd { span })),
        "dirty_close" => Ok(Statement::DirtyClose(DirtyCloseCmd { span })),
        "ping" => Ok(Statement::Ping(PingCmd { span })),
        "ps_prepare" => Ok(Statement::PsPrepare(PsPrepareCmd { span, sql: take_rest(&mut s).into() })),
        "ps_bind" => Ok(Statement::PsBind(PsBindCmd { span, name: take_rest(&mut s).into() })),
        "ps_execute" => Ok(Statement::PsExecute(PsExecuteCmd { span })),
        "ps_close" => Ok(Statement::PsClose(PsCloseCmd { span })),
        "optimizer_trace" => Ok(Statement::OptimizerTrace(OptimizerTraceCmd { span })),

        // Commands with arguments
        "result_format" => Ok(Statement::ResultFormat(ResultFormatCmd { span, version: take_rest(&mut s).to_string() })),
        "query_attributes" => Ok(Statement::QueryAttributes(QueryAttributesCmd { span, attributes: take_rest(&mut s).into() })),
        "skip_if_hypergraph" => Ok(Statement::SkipIfHypergraph(SkipIfHypergraphCmd {
            span,
            message: take_rest_opt(&mut s).map(|m| m.into()),
        })),
        "sync_with_master" => Ok(Statement::SyncWithMaster(SyncWithMasterCmd {
            span,
            offset: take_rest_opt(&mut s).map(|a| a.to_string()),
        })),
        "evalp" => Ok(Statement::EvalP(EvalPCmd { span, sql: take_rest(&mut s).into() })),

        // File I/O commands
        "list_files_write_file" => {
            let tokens = run(ws_tokens, args, span, "list_files_write_file")?;
            Ok(Statement::ListFilesWriteFile(ListFilesWriteFileCmd {
                span,
                file: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
                dir_pattern: tokens.get(1).map(|t| strip_quotes(t).into()).unwrap_or_else(|| "*".into()),
            }))
        }
        "list_files_append_file" => {
            let tokens = run(ws_tokens, args, span, "list_files_append_file")?;
            Ok(Statement::ListFilesAppendFile(ListFilesAppendFileCmd {
                span,
                file: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
                dir_pattern: tokens.get(1).map(|t| strip_quotes(t).into()).unwrap_or_else(|| "*".into()),
            }))
        }
        "force-rmdir" => Ok(Statement::ForceRmdir(ForceRmdirCmd { span, dir: strip_quotes(take_rest(&mut s)).into() })),
        "force-cpdir" => {
            let tokens = run(ws_tokens, args, span, "force-cpdir")?;
            Ok(Statement::ForceCpdir(ForceCpdirCmd {
                span,
                source: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
                dest: strip_quotes(tokens.get(1).copied().unwrap_or("")).into(),
            }))
        }
        "force-restart" => Ok(Statement::DirtyClose(DirtyCloseCmd { span })),
        "write_line" => {
            let tokens = run(ws_tokens, args, span, "write_line")?;
            Ok(Statement::WriteLine(WriteLineCmd {
                span,
                text: tokens.first().copied().unwrap_or("").into(),
                file: tokens.get(1).map(|t| strip_quotes(t).into()).unwrap_or_else(|| "".into()),
            }))
        }

        "delimiter" => {
            let new_delim = take_rest(&mut s).to_string();
            if new_delim.is_empty() {
                return Err(ParseError::Syntax { message: "delimiter requires an argument".to_string(), span });
            }
            stream.state.delimiter = new_delim.clone();
            Ok(Statement::Delimiter(DelimiterCmd { span, new_delimiter: new_delim }))
        }

        "write_file" => {
            let _ = ws(&mut s);
            let filename = single_token(&mut s).unwrap_or("");
            Ok(Statement::WriteFile(WriteFileCmd { span, filename: filename.into(), end_marker: String::new(), content: String::new().into() }))
        }
        "append_file" => {
            let _ = ws(&mut s);
            let filename = single_token(&mut s).unwrap_or("");
            Ok(Statement::AppendFile(AppendFileCmd { span, filename: filename.into(), end_marker: String::new(), content: String::new().into() }))
        }
        "remove_file" => {
            let tokens = run(ws_tokens, args, span, "remove_file")?;
            Ok(Statement::RemoveFile(RemoveFileCmd {
                span,
                file: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
                timeout: tokens.get(1).map(|t| t.to_string()),
            }))
        }
        "remove_files_wildcard" => {
            let tokens = run(ws_tokens, args, span, "remove_files_wildcard")?;
            Ok(Statement::RemoveFilesWildcard(RemoveFilesWildcardCmd {
                span,
                dir: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
                pattern: tokens.get(1).copied().unwrap_or("").into(),
                timeout: tokens.get(2).map(|t| t.to_string()),
            }))
        }
        "copy_file" => {
            let tokens = run(ws_tokens, args, span, "copy_file")?;
            Ok(Statement::CopyFile(CopyFileCmd {
                span,
                source: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
                dest: strip_quotes(tokens.get(1).copied().unwrap_or("")).into(),
                retry: tokens.get(2).map(|t| t.to_string()),
            }))
        }
        "move_file" => {
            let tokens = run(ws_tokens, args, span, "move_file")?;
            Ok(Statement::MoveFile(MoveFileCmd {
                span,
                source: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
                dest: strip_quotes(tokens.get(1).copied().unwrap_or("")).into(),
                timeout: tokens.get(2).map(|t| t.to_string()),
            }))
        }
        "mkdir" => {
            let _ = ws(&mut s);
            let dir = single_token(&mut s).unwrap_or("");
            Ok(Statement::Mkdir(MkdirCmd { span, dir: dir.into() }))
        }
        "rmdir" => {
            let _ = ws(&mut s);
            let dir = single_token(&mut s).unwrap_or("");
            Ok(Statement::Rmdir(RmdirCmd { span, dir: dir.into() }))
        }
        "chmod" => {
            let tokens = run(ws_tokens, args, span, "chmod")?;
            Ok(Statement::Chmod(ChmodCmd {
                span,
                mode: tokens.first().copied().unwrap_or("").to_string(),
                file: strip_quotes(tokens.get(1).copied().unwrap_or("")).into(),
            }))
        }
        "diff_files" => {
            let tokens = run(ws_tokens, args, span, "diff_files")?;
            Ok(Statement::DiffFiles(DiffFilesCmd {
                span,
                file1: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
                file2: strip_quotes(tokens.get(1).copied().unwrap_or("")).into(),
            }))
        }
        "file_exists" => {
            let _ = ws(&mut s);
            let file = single_token(&mut s).unwrap_or("");
            Ok(Statement::FileExists(FileExistsCmd { span, file: file.into() }))
        }
        "cat_file" => {
            let _ = ws(&mut s);
            let file = single_token(&mut s).unwrap_or("");
            Ok(Statement::CatFile(CatFileCmd { span, file: file.into() }))
        }
        "list_files" => {
            let tokens = run(ws_tokens, args, span, "list_files")?;
            let dir = tokens.first().filter(|t| !t.is_empty()).map(|t| strip_quotes(t).into());
            let pattern = tokens.get(1).filter(|t| !t.is_empty()).copied().map(|t| t.into());
            Ok(Statement::ListFiles(ListFilesCmd { span, dir, pattern }))
        }

        "shutdown_server" => Ok(Statement::ShutdownServer(ShutdownServerCmd { span })),
        "send_quit" => Ok(Statement::SendQuit(SendQuitCmd { span })),
        "send_shutdown" => Ok(Statement::SendShutdown(SendShutdownCmd { span })),

        "if" | "while" | "perl" => Ok(Statement::Sql(SqlStatement { span, sql: input.trim().into() })),
        "end" => Ok(Statement::End(EndCmd { span })),

        "assert" => {
            if !stream.state.version.has_assert() {
                return Err(ParseError::VersionMismatch { command: "assert".to_string(), version: format!("{:?}", stream.state.version), span });
            }
            Ok(Statement::Sql(SqlStatement { span, sql: input.trim().into() }))
        }

        "character_set" => Ok(Statement::CharacterSet(CharacterSetCmd { span, charset: take_rest(&mut s).to_string() })),
        "system" => Ok(Statement::System(SystemCmd { span, command: take_rest(&mut s).into() })),
        "real_sleep" => Ok(Statement::RealSleep(RealSleepCmd { span, seconds: take_rest(&mut s).to_string() })),
        "require" => Ok(Statement::Require(RequireCmd { span, file: take_rest(&mut s).into() })),
        "lowercase_result" => Ok(Statement::LowercaseResult(LowercaseResultCmd { span })),
        "sync_slave_with_master" => Ok(Statement::SyncSlaveWithMaster(SyncSlaveWithMasterCmd { span })),
        "copy_files_wildcard" => {
            let tokens = run(ws_tokens, args, span, "copy_files_wildcard")?;
            Ok(Statement::CopyFilesWildcard(CopyFilesWildcardCmd {
                span,
                source: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
                dest: strip_quotes(tokens.get(1).copied().unwrap_or("")).into(),
                retry: tokens.get(2).map(|t| t.to_string()),
            }))
        }

        _ => Err(ParseError::UnknownCommand { command: name.to_string(), span, suggestion: None }),
    }
}

/// Skip past the command name in the input string, returning the rest.
/// Case-insensitive: `name` is always lowercase (from classify_command),
/// but `input` may have any case (e.g., `LET`, `Echo`).
fn skip_command_name<'a>(name: &'a str, input: &'a str) -> &'a str {
    let end = name.len().min(input.len());
    if input[..end].eq_ignore_ascii_case(name) {
        input[end..].trim_start()
    } else {
        input
    }
}

/// Strip surrounding quotes from an ARG_STRING argument.
pub(crate) fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if s.is_empty() {
        return s;
    }
    let first = s.as_bytes()[0];
    if (first == b'"' || first == b'\'' || first == b'`')
        && let Some(pos) = s[1..].find(char::from(first))
    {
        return &s[1..pos + 1];
    }
    s
}

// ---------------------------------------------------------------------------
// Regex parsers (hand-written — byte-level escape/delimiter state machines)
// ---------------------------------------------------------------------------

/// Parse `--replace_regex` with version-aware syntax dispatch.
fn parse_replace_regex(stream: &Stream, args: &str, span: Span) -> Result<Statement, ParseError> {
    parse_replace_regex_versioned(args, span, &stream.state.version)
}

/// Parse `--replace_regex` with version-aware syntax.
fn parse_replace_regex_versioned(args: &str, span: Span, version: &MysqlVersion) -> Result<Statement, ParseError> {
    let trimmed = args.trim();

    // $var form: store the pattern for later variable expansion
    if trimmed.starts_with('$') {
        return Ok(Statement::ReplaceRegex(ReplaceRegexCmd {
            span,
            pattern: trimmed.to_string(),
            replacement: String::new(),
            flags: None,
        }));
    }

    if trimmed.is_empty() {
        return Err(ParseError::Syntax { message: "empty replace_regex".to_string(), span });
    }

    if version.is_mariadb() {
        parse_replace_regex_mariadb(trimmed, span)
    } else {
        parse_replace_regex_mysql(trimmed, span)
    }
}

/// MySQL replace_regex: only `s/pattern/replacement/flags` or `/pattern/replacement/flags`.
fn parse_replace_regex_mysql(trimmed: &str, span: Span) -> Result<Statement, ParseError> {
    let mut stream: &str = trimmed;

    // Match prefix: "s/" or "/"
    let _prefix: &str = parse_regex_prefix(&mut stream)
        .map_err(|_| ParseError::Syntax { message: format!("invalid replace_regex syntax: {}", trimmed), span })?;

    let sep = '/';
    // Pattern: scan until next unescaped '/'
    let pattern = scan_until_char(&mut stream, sep).to_string();
    // Replacement: scan until next unescaped '/'
    let replacement = scan_until_char(&mut stream, sep).to_string();
    // Flags
    let flags = stream.trim();
    let flags = if flags.is_empty() { None } else { Some(flags.to_string()) };

    Ok(Statement::ReplaceRegex(ReplaceRegexCmd { span, pattern, replacement, flags }))
}

/// MariaDB replace_regex: supports paired delimiters, arbitrary single-char delimiters,
/// and backslash escaping. Mirrors `parse_re_part` in MariaDB's mysqltest.cc.
fn parse_replace_regex_mariadb(trimmed: &str, span: Span) -> Result<Statement, ParseError> {
    let first = trimmed.as_bytes()[0];

    // Determine delimiter pair from first character
    let (pattern_close, rest_after_pattern) = match first {
        b'(' => (b')', &trimmed[1..]),
        b'[' => (b']', &trimmed[1..]),
        b'{' => (b'}', &trimmed[1..]),
        b'<' => (b'>', &trimmed[1..]),
        _ => (first, &trimmed[1..]),
    };

    let (pattern, rest) = scan_regex_part(rest_after_pattern, pattern_close);
    let rest = rest.trim_start();

    // Determine replacement delimiter:
    // If pattern used paired delimiters (first != pattern_close), the first char of rest
    // determines a new delimiter pair. Otherwise reuse the same character.
    let (replacement_close, rest_after_replacement) = if rest.is_empty() {
        (pattern_close, "")
    } else if first != pattern_close {
        let re_first = rest.as_bytes()[0];
        let re_close = match re_first {
            b'(' => b')',
            b'[' => b']',
            b'{' => b'}',
            b'<' => b'>',
            _ => re_first,
        };
        (re_close, &rest[1..])
    } else {
        (pattern_close, rest)
    };

    let (replacement, after_replacement) = scan_regex_part(rest_after_replacement, replacement_close);

    // Check for flags (only 'i' recognized)
    let flags_str = after_replacement.trim_start();
    let flags = if flags_str.starts_with('i') {
        Some("i".to_string())
    } else if !flags_str.is_empty() {
        Some(flags_str.to_string())
    } else {
        None
    };

    Ok(Statement::ReplaceRegex(ReplaceRegexCmd { span, pattern, replacement, flags }))
}

/// Scan input until an unescaped occurrence of `delimiter` is found.
/// Backslash escapes the delimiter character (the backslash itself is consumed).
/// Returns (scanned_content, remaining_input).
fn scan_regex_part(input: &str, delimiter: u8) -> (String, &str) {
    let mut result = String::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == delimiter {
            i += 1; // skip backslash
            result.push(bytes[i] as char);
        } else if bytes[i] == delimiter {
            return (result, &input[i + 1..]);
        } else {
            result.push(bytes[i] as char);
        }
        i += 1;
    }
    // delimiter not found — return what we have with empty remaining
    (result, "")
}

/// Match the replace_regex prefix: "s/" or "/".
fn parse_regex_prefix<'s>(input: &mut &'s str) -> winnow::ModalResult<&'s str> {
    alt(("s/", "/")).parse_next(input)
}

/// Scan stream until an unescaped occurrence of `ch` is found.
/// Advances the stream past `ch`.
fn scan_until_char<'a>(stream: &mut &'a str, ch: char) -> &'a str {
    let bytes = stream.as_bytes();
    let target = ch as u8;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == target {
            i += 2; // skip escaped char
            continue;
        }
        if bytes[i] == target {
            let result = &stream[..i];
            *stream = &stream[i + 1..]; // advance past delimiter
            return result;
        }
        i += 1;
    }
    // delimiter not found — return rest
    let result = *stream;
    *stream = "";
    result
}

// ---------------------------------------------------------------------------
// Toggle commands
// ---------------------------------------------------------------------------

fn parse_toggle(name: &str, args: &str, span: Span) -> Result<Statement, ParseError> {
    let (enabled, kind) = if let Some(k) = name.strip_prefix("enable_") {
        (true, toggle_kind(k))
    } else if let Some(k) = name.strip_prefix("disable_") {
        (false, toggle_kind(k))
    } else {
        return Err(ParseError::UnknownCommand { command: name.to_string(), span, suggestion: None });
    };
    let once = args.trim().eq_ignore_ascii_case("ONCE");
    Ok(Statement::Toggle(ToggleCmd { span, kind, enabled, once }))
}

fn toggle_kind(name: &str) -> ToggleKind {
    match name {
        "warnings" => ToggleKind::Warnings,
        "query_log" => ToggleKind::QueryLog,
        "result_log" => ToggleKind::ResultLog,
        "info" => ToggleKind::Info,
        "metadata" => ToggleKind::Metadata,
        "ps_protocol" => ToggleKind::PsProtocol,
        "reconnect" => ToggleKind::Reconnect,
        "connect_log" => ToggleKind::ConnectLog,
        "session_track_info" => ToggleKind::SessionTrackInfo,
        "testcase" => ToggleKind::Testcase,
        "abort_on_error" => ToggleKind::AbortOnError,
        "parsing" => ToggleKind::Parsing,
        "async_client" => ToggleKind::AsyncClient,
        "prepare_warnings" => ToggleKind::PrepareWarnings,
        "cursor_protocol" => ToggleKind::CursorProtocol,
        "non_blocking_api" => ToggleKind::NonBlockingApi,
        "ps2_protocol" => ToggleKind::Ps2Protocol,
        "service_connection" => ToggleKind::ServiceConnection,
        "view_protocol" => ToggleKind::ViewProtocol,
        "column_names" => ToggleKind::ColumnNames,
        _ => ToggleKind::Warnings,
    }
}