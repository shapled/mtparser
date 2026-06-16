//! Command-specific parsers using winnow combinators.
//!
//! All command argument parsing is implemented as winnow parser functions
//! following the signature `fn(&mut &str) -> ModalResult<T>`.

use winnow::combinator::{alt, cut_err, preceded, separated};
use winnow::token::{one_of, take_till, take_while};
use winnow::Parser;

use crate::ast::commands::*;
use crate::ast::statement::*;
use crate::ast::text::InterpolatedText;
use crate::ast::Span;
use crate::error::ParseError;
use crate::parser::ParseContext;
use crate::version::MysqlVersion;

// ---------------------------------------------------------------------------
// Shared primitive combinators
// ---------------------------------------------------------------------------

/// Parse a `$variable_name`, returning the variable name without the `$` prefix.
fn parse_dollar_var<'s>(input: &mut &'s str) -> winnow::ModalResult<&'s str> {
    preceded(
        '$',
        cut_err(take_while(1.., ('a'..='z', 'A'..='Z', '0'..='9', '_'))),
    )
    .parse_next(input)
}

/// Parse a possibly quote-wrapped argument, returning the inner content.
/// Quotes recognized: `'`, `"`, `` ` ``.
pub(crate) fn parse_quoted_arg<'s>(input: &mut &'s str) -> winnow::ModalResult<&'s str> {
    let mut open = one_of(['\'', '"', '`']).parse_next(input)?;
    let content = take_till(1.., [open]).parse_next(input)?;
    open.parse_next(input)?; // consume closing quote
    Ok(content)
}

/// Parse a single filename token: either a quoted string or non-space text.
fn parse_filename_token<'s>(input: &mut &'s str) -> winnow::ModalResult<&'s str> {
    alt((
        parse_quoted_arg,
        take_till(1.., [' ', '\t']),
    ))
    .parse_next(input)
}

// ---------------------------------------------------------------------------
// Command dispatcher
// ---------------------------------------------------------------------------

/// Parse a known command by name.
pub(crate) fn parse_known_command(
    ctx: &mut ParseContext,
    name: &str,
    input: &str,
    span: Span,
) -> Result<Statement, ParseError> {
    // Check version compatibility for version-specific commands
    if !ctx.config.version.has_command(name) {
        return Err(ParseError::VersionMismatch {
            command: name.to_string(),
            version: format!("{:?}", ctx.config.version),
            span,
        });
    }

    let args = skip_command_name(name, input);

    match name {
        "echo" => Ok(Statement::Echo(EchoCmd { span, text: args.trim().into() })),
        "source" => {
            let mut stream = args.trim();
            let file = parse_filename_token.parse_next(&mut stream).unwrap_or(args.trim());
            Ok(Statement::Source(SourceCmd { span, file: file.into() }))
        }
        "skip" => Ok(Statement::Skip(SkipCmd { span, message: { let m = args.trim(); if m.is_empty() { None } else { Some(m.into()) } } })),
        "die" => Ok(Statement::Die(DieCmd { span, message: { let m = args.trim(); if m.is_empty() { None } else { Some(m.into()) } } })),
        "exit" => Ok(Statement::Exit(ExitCmd { span })),
        "exec" => Ok(Statement::Exec(ExecCmd { span, command: args.trim().into() })),
        "execw" => Ok(Statement::Execw(ExecwCmd { span, command: args.trim().into() })),
        "exec_in_background" => Ok(Statement::ExecInBackground(ExecInBackgroundCmd { span, command: args.trim().into() })),
        "sleep" => Ok(Statement::Sleep(SleepCmd { span, seconds: args.trim().to_string() })),
        "reap" => Ok(Statement::Reap(ReapCmd { span })),
        "output" => parse_output_cmd(args, span),

        "let" => parse_let(args, span),
        "inc" => parse_inc(args, span),
        "dec" => parse_dec(args, span),
        "expr" => parse_expr(args, span),
        "error" => parse_error(args, span),

        "connect" => parse_connect(args, span),
        "connection" => Ok(Statement::Connection(ConnectionCmd { span, name: args.trim().into() })),
        "disconnect" => Ok(Statement::Disconnect(DisconnectCmd { span, name: args.trim().into() })),
        "change_user" => parse_change_user(args, span),
        "reset_connection" => Ok(Statement::ResetConnection(ResetConnectionCmd { span })),

        "query" => Ok(Statement::Query(QueryCmd { span, sql: args.trim().into() })),
        "eval" => Ok(Statement::Eval(EvalCmd { span, sql: args.trim().into() })),
        "send" => Ok(Statement::Send(SendCmd { span, sql: args.trim().into() })),
        "send_eval" => Ok(Statement::SendEval(SendEvalCmd { span, sql: args.trim().into() })),

        "horizontal_results" | "query_horizontal" => Ok(Statement::HorizontalResults(HorizontalResultsCmd { span })),
        "vertical_results" => Ok(Statement::VerticalResults(VerticalResultsCmd { span })),
        "sorted_result" => Ok(Statement::SortedResult(SortedResultCmd { span })),
        "replace_result" => parse_replace_result(args, span),
        "replace_column" => parse_replace_column(args, span),
        "replace_regex" => parse_replace_regex(ctx, args, span),
        "partially_sorted_result" => Ok(Statement::PartiallySortedResult(PartiallySortedResultCmd { span, columns: args.trim().to_string() })),
        "replace_numeric_round" => Ok(Statement::ReplaceNumericRound(ReplaceNumericRoundCmd { span, decimals: args.trim().to_string() })),

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
        "query_vertical" => Ok(Statement::QueryVertical(QueryVerticalCmd { span, sql: args.trim().into() })),
        "save_master_pos" => Ok(Statement::SaveMasterPos(SaveMasterPosCmd { span })),
        "wait_for_slave_to_stop" => Ok(Statement::WaitForSlaveToStop(WaitForSlaveToStopCmd { span })),
        "dirty_close" => Ok(Statement::DirtyClose(DirtyCloseCmd { span })),
        "ping" => Ok(Statement::Ping(PingCmd { span })),
        "ps_prepare" => Ok(Statement::PsPrepare(PsPrepareCmd { span, sql: args.trim().into() })),
        "ps_bind" => Ok(Statement::PsBind(PsBindCmd { span, name: args.trim().into() })),
        "ps_execute" => Ok(Statement::PsExecute(PsExecuteCmd { span })),
        "ps_close" => Ok(Statement::PsClose(PsCloseCmd { span })),
        "optimizer_trace" => Ok(Statement::OptimizerTrace(OptimizerTraceCmd { span })),

        // Commands with arguments
        "result_format" => Ok(Statement::ResultFormat(ResultFormatCmd { span, version: args.trim().to_string() })),
        "query_attributes" => Ok(Statement::QueryAttributes(QueryAttributesCmd { span, attributes: args.trim().into() })),
        "skip_if_hypergraph" => {
            let msg = args.trim();
            Ok(Statement::SkipIfHypergraph(SkipIfHypergraphCmd {
                span,
                message: if msg.is_empty() { None } else { Some(msg.into()) },
            }))
        }
        "sync_with_master" => Ok(Statement::SyncWithMaster(SyncWithMasterCmd {
            span,
            offset: { let a = args.trim(); if a.is_empty() { None } else { Some(a.to_string()) } },
        })),
        "evalp" => Ok(Statement::EvalP(EvalPCmd { span, sql: args.trim().into() })),

        // File I/O commands
        "list_files_write_file" => parse_list_files_write_file(args, span),
        "list_files_append_file" => parse_list_files_append_file(args, span),
        "force-rmdir" => Ok(Statement::ForceRmdir(ForceRmdirCmd { span, dir: strip_quotes(args.trim()).into() })),
        "force-cpdir" => parse_force_cpdir(args, span),
        "force-restart" => Ok(Statement::DirtyClose(DirtyCloseCmd { span })), // treat as no-op
        "write_line" => parse_write_line(args, span),

        "delimiter" => {
            let new_delim = args.trim().to_string();
            if new_delim.is_empty() {
                return Err(ParseError::Syntax { message: "delimiter requires an argument".to_string(), span });
            }
            ctx.delimiter = new_delim.clone();
            Ok(Statement::Delimiter(DelimiterCmd { span, new_delimiter: new_delim }))
        }

        "write_file" => {
            let mut stream = args.trim();
            let filename = parse_filename_token.parse_next(&mut stream).unwrap_or(args.trim());
            Ok(Statement::WriteFile(WriteFileCmd { span, filename: filename.into(), end_marker: String::new(), content: String::new().into() }))
        }
        "append_file" => {
            let mut stream = args.trim();
            let filename = parse_filename_token.parse_next(&mut stream).unwrap_or(args.trim());
            Ok(Statement::AppendFile(AppendFileCmd { span, filename: filename.into(), end_marker: String::new(), content: String::new().into() }))
        }
        "remove_file" => parse_remove_file(args, span),
        "remove_files_wildcard" => parse_remove_files_wildcard(args, span),
        "copy_file" => parse_copy_file(args, span),
        "move_file" => parse_move_file(args, span),
        "mkdir" => {
            let mut stream = args.trim();
            let dir = parse_filename_token.parse_next(&mut stream).unwrap_or(args.trim());
            Ok(Statement::Mkdir(MkdirCmd { span, dir: dir.into() }))
        }
        "rmdir" => {
            let mut stream = args.trim();
            let dir = parse_filename_token.parse_next(&mut stream).unwrap_or(args.trim());
            Ok(Statement::Rmdir(RmdirCmd { span, dir: dir.into() }))
        }
        "chmod" => parse_chmod(args, span),
        "diff_files" => parse_diff_files(args, span),
        "file_exists" => {
            let mut stream = args.trim();
            let file = parse_filename_token.parse_next(&mut stream).unwrap_or(args.trim());
            Ok(Statement::FileExists(FileExistsCmd { span, file: file.into() }))
        }
        "cat_file" => {
            let mut stream = args.trim();
            let file = parse_filename_token.parse_next(&mut stream).unwrap_or(args.trim());
            Ok(Statement::CatFile(CatFileCmd { span, file: file.into() }))
        }
        "list_files" => parse_list_files(args, span),

        "shutdown_server" => Ok(Statement::ShutdownServer(ShutdownServerCmd { span })),
        "send_quit" => Ok(Statement::SendQuit(SendQuitCmd { span })),
        "send_shutdown" => Ok(Statement::SendShutdown(SendShutdownCmd { span })),

        "if" | "while" | "perl" => Ok(Statement::Sql(SqlStatement { span, sql: input.trim().into() })),
        "end" => Ok(Statement::End(EndCmd { span })),

        "assert" => {
            if !ctx.config.version.has_assert() {
                return Err(ParseError::VersionMismatch { command: "assert".to_string(), version: format!("{:?}", ctx.config.version), span });
            }
            Ok(Statement::Sql(SqlStatement { span, sql: input.trim().into() }))
        }

        "character_set" => Ok(Statement::CharacterSet(CharacterSetCmd { span, charset: args.trim().to_string() })),
        "system" => Ok(Statement::System(SystemCmd { span, command: args.trim().into() })),
        "real_sleep" => Ok(Statement::RealSleep(RealSleepCmd { span, seconds: args.trim().to_string() })),
        "require" => Ok(Statement::Require(RequireCmd { span, file: args.trim().into() })),
        "lowercase_result" => Ok(Statement::LowercaseResult(LowercaseResultCmd { span })),
        "sync_slave_with_master" => Ok(Statement::SyncSlaveWithMaster(SyncSlaveWithMasterCmd { span })),
        "copy_files_wildcard" => parse_copy_files_wildcard(args, span),

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
    if first == b'"' || first == b'\'' || first == b'`' {
        if let Some(pos) = s[1..].find(char::from(first)) {
            return &s[1..pos + 1];
        }
    }
    s
}

// ---------------------------------------------------------------------------
// Complex command parsers
// ---------------------------------------------------------------------------

/// Parse `--let $var = value` or `let var = value` (bare form, no `$` prefix).
fn parse_let(args: &str, span: Span) -> Result<Statement, ParseError> {
    let trimmed = args.trim();

    // Variable name extends until '=' or whitespace (matches mtr's do_let).
    // '$' inside the name is a literal character, not a variable reference.
    let var_name_end = trimmed.find(|c| c == '=' || c == ' ' || c == '\t').unwrap_or(trimmed.len());
    let var_name = &trimmed[..var_name_end];

    // Strip leading '$' if present (matches mtr's var_set behavior)
    let var_name = var_name.strip_prefix('$').unwrap_or(var_name);

    if var_name.is_empty() {
        return Err(ParseError::Syntax { message: format!("invalid let syntax: {}", trimmed), span });
    }

    // Find '=' after variable name
    let rest = trimmed[var_name_end..].trim_start();
    let value_str = rest.strip_prefix('=')
        .ok_or_else(|| ParseError::Syntax { message: format!("invalid let syntax: {}", trimmed), span })?
        .trim();

    // Check for backtick-enclosed query
    if let Some(query) = value_str.strip_prefix('`') {
        if let Some(query) = query.strip_suffix('`') {
            return Ok(Statement::Let(LetCmd {
                span,
                variable: var_name.to_string(),
                value: LetValue::Query(crate::ast::expr::QueryExpr::new(
                    crate::ast::Span::dummy(),
                    query.to_string(),
                )),
            }));
        }
    }

    Ok(Statement::Let(LetCmd { span, variable: var_name.to_string(), value: LetValue::Literal(value_str.to_string()) }))
}

/// Parse `--inc $var`.
fn parse_inc(args: &str, span: Span) -> Result<Statement, ParseError> {
    let mut stream = args.trim();
    let var = parse_dollar_var.parse_next(&mut stream).unwrap_or("");
    if var.is_empty() {
        return Err(ParseError::Syntax { message: "inc requires a variable name ($var)".to_string(), span });
    }
    Ok(Statement::Inc(IncCmd { span, variable: var.to_string() }))
}

/// Parse `--dec $var`.
fn parse_dec(args: &str, span: Span) -> Result<Statement, ParseError> {
    let mut stream = args.trim();
    let var = parse_dollar_var.parse_next(&mut stream).unwrap_or("");
    if var.is_empty() {
        return Err(ParseError::Syntax { message: "dec requires a variable name ($var)".to_string(), span });
    }
    Ok(Statement::Dec(DecCmd { span, variable: var.to_string() }))
}

/// Parse `--expr $var = expression`.
fn parse_expr(args: &str, span: Span) -> Result<Statement, ParseError> {
    let mut stream = args.trim();
    let var_name = parse_dollar_var.parse_next(&mut stream)
        .map_err(|_| ParseError::Syntax { message: format!("invalid expr syntax: {}", args.trim()), span })?;
    let stream = stream.strip_prefix('=').unwrap_or(stream);
    let expression = stream.trim().to_string();
    if var_name.is_empty() || expression.is_empty() {
        return Err(ParseError::Syntax { message: format!("invalid expr syntax: {}", args.trim()), span });
    }
    Ok(Statement::Expr(ExprCmd { span, variable: var_name.to_string(), expression }))
}

/// Parse `--error [code1, code2, ...]`.
fn parse_error(args: &str, span: Span) -> Result<Statement, ParseError> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return Ok(Statement::Error(ErrorCmd { span, error_codes: vec![] }));
    }
    let codes: Vec<InterpolatedText> = trimmed
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(Into::into)
        .collect();
    Ok(Statement::Error(ErrorCmd { span, error_codes: codes }))
}

/// Parse `--connect(name, host, user, pass, db, port, socket)`.
fn parse_connect(args: &str, span: Span) -> Result<Statement, ParseError> {
    let trimmed = args.trim();

    // Try parenthesized form: (name, host, user, pass, db, port, socket)
    if let Some(inner) = trimmed.strip_prefix('(') {
        if let Some(inner) = inner.strip_suffix(')') {
            let parts: Vec<&str> = inner.split(',').collect();
            let name = parts.first().map(|s| s.trim().into());
            let mut params = ConnectParams::default();
            if parts.len() > 1 { params.host = Some(parts[1].trim().into()); }
            if parts.len() > 2 { params.user = Some(parts[2].trim().into()); }
            if parts.len() > 3 { params.password = Some(parts[3].trim().into()); }
            if parts.len() > 4 { params.database = Some(parts[4].trim().into()); }
            if parts.len() > 5 { params.port = Some(parts[5].trim().into()); }
            if parts.len() > 6 { params.socket = Some(parts[6].trim().into()); }
            return Ok(Statement::Connect(ConnectCmd { span, name, params }));
        }
    }

    Ok(Statement::Connect(ConnectCmd { span, name: Some(trimmed.into()), params: ConnectParams::default() }))
}

/// Parse `--change_user [user, password, database]`.
fn parse_change_user(args: &str, span: Span) -> Result<Statement, ParseError> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return Ok(Statement::ChangeUser(ChangeUserCmd { span, user: None, password: None, database: None }));
    }
    let parts: Vec<&str> = trimmed.split(',').collect();
    Ok(Statement::ChangeUser(ChangeUserCmd {
        span,
        user: parts.first().map(|s| s.trim()).filter(|s| !s.is_empty()).map(|s| (*s).into()),
        password: parts.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()).map(|s| (*s).into()),
        database: parts.get(2).map(|s| s.trim()).filter(|s| !s.is_empty()).map(|s| (*s).into()),
    }))
}

/// Parse `--replace_regex` with version-aware syntax dispatch.
fn parse_replace_regex(ctx: &ParseContext, args: &str, span: Span) -> Result<Statement, ParseError> {
    parse_replace_regex_versioned(args, span, &ctx.config.version)
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

/// Parse `--replace_result old new [old2 new2 ...]`.
fn parse_replace_result(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens: Vec<&str> = args.split_whitespace().collect();
    let mut replacements = Vec::new();
    let mut i = 0;
    while i + 1 < tokens.len() {
        replacements.push((tokens[i].into(), tokens[i + 1].into()));
        i += 2;
    }
    Ok(Statement::ReplaceResult(ReplaceResultCmd { span, replacements }))
}

/// Parse `--replace_column col_num old new [col_num2 old2 new2 ...]`.
fn parse_replace_column(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens: Vec<&str> = args.split_whitespace().collect();
    let mut replacements = Vec::new();
    let mut i = 0;
    while i + 2 < tokens.len() {
        replacements.push(ReplaceColumnItem {
            column: tokens[i].to_string(),
            old_value: tokens[i + 1].into(),
            new_value: tokens[i + 2].into(),
        });
        i += 3;
    }
    Ok(Statement::ReplaceColumn(ReplaceColumnCmd { span, replacements }))
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

// ---------------------------------------------------------------------------
// File I/O command parsers
// ---------------------------------------------------------------------------

/// Parse whitespace-separated tokens, returning borrowed slices.
fn parse_file_tokens(args: &str) -> Vec<&str> {
    let mut stream: &str = args.trim();
    parse_ws_tokens_inner(&mut stream).unwrap_or_default()
}

/// Winnow parser for whitespace-separated tokens.
fn parse_ws_tokens_inner<'s>(input: &mut &'s str) -> winnow::ModalResult<Vec<&'s str>> {
    separated(0.., take_till(1.., [' ', '\t', '\n', '\r']), take_while(1.., [' ', '\t']))
        .parse_next(input)
}

/// `--remove_file file [timeout]`
fn parse_remove_file(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens = parse_file_tokens(args);
    Ok(Statement::RemoveFile(RemoveFileCmd {
        span,
        file: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
        timeout: tokens.get(1).map(|s| s.to_string()),
    }))
}

/// `--remove_files_wildcard dir pattern [timeout]`
fn parse_remove_files_wildcard(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens = parse_file_tokens(args);
    Ok(Statement::RemoveFilesWildcard(RemoveFilesWildcardCmd {
        span,
        dir: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
        pattern: tokens.get(1).copied().unwrap_or("").into(),
        timeout: tokens.get(2).map(|s| s.to_string()),
    }))
}

/// `--copy_file src dest [retry]`
fn parse_copy_file(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens = parse_file_tokens(args);
    Ok(Statement::CopyFile(CopyFileCmd {
        span,
        source: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
        dest: strip_quotes(tokens.get(1).copied().unwrap_or("")).into(),
        retry: tokens.get(2).map(|s| s.to_string()),
    }))
}

/// `--move_file src dest [timeout]`
fn parse_move_file(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens = parse_file_tokens(args);
    Ok(Statement::MoveFile(MoveFileCmd {
        span,
        source: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
        dest: strip_quotes(tokens.get(1).copied().unwrap_or("")).into(),
        timeout: tokens.get(2).map(|s| s.to_string()),
    }))
}

/// `--chmod mode file`
fn parse_chmod(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens = parse_file_tokens(args);
    Ok(Statement::Chmod(ChmodCmd {
        span,
        mode: tokens.first().copied().unwrap_or("").to_string(),
        file: strip_quotes(tokens.get(1).copied().unwrap_or("")).into(),
    }))
}

/// `--diff_files file1 file2`
fn parse_diff_files(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens = parse_file_tokens(args);
    Ok(Statement::DiffFiles(DiffFilesCmd {
        span,
        file1: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
        file2: strip_quotes(tokens.get(1).copied().unwrap_or("")).into(),
    }))
}

/// `--list_files [dir] [pattern]`
fn parse_list_files(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens = parse_file_tokens(args);
    let dir = tokens.first().filter(|s| !s.is_empty()).map(|s| strip_quotes(*s).into());
    let pattern = tokens.get(1).filter(|s| !s.is_empty()).copied().map(|s| s.into());
    Ok(Statement::ListFiles(ListFilesCmd { span, dir, pattern }))
}

/// `--copy_files_wildcard src_pattern dest [retry]`
fn parse_copy_files_wildcard(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens = parse_file_tokens(args);
    Ok(Statement::CopyFilesWildcard(CopyFilesWildcardCmd {
        span,
        source: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
        dest: strip_quotes(tokens.get(1).copied().unwrap_or("")).into(),
        retry: tokens.get(2).map(|s| s.to_string()),
    }))
}

/// `--output file`
fn parse_output_cmd(args: &str, span: Span) -> Result<Statement, ParseError> {
    Ok(Statement::Output(OutputCmd { span, file: strip_quotes(args.trim()).into() }))
}

/// `--list_files_write_file file [dir_pattern]`
fn parse_list_files_write_file(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens = parse_file_tokens(args);
    Ok(Statement::ListFilesWriteFile(ListFilesWriteFileCmd {
        span,
        file: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
        dir_pattern: tokens.get(1).map(|s| strip_quotes(*s).into()).unwrap_or_else(|| "*".into()),
    }))
}

/// `--list_files_append_file file [dir_pattern]`
fn parse_list_files_append_file(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens = parse_file_tokens(args);
    Ok(Statement::ListFilesAppendFile(ListFilesAppendFileCmd {
        span,
        file: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
        dir_pattern: tokens.get(1).map(|s| strip_quotes(*s).into()).unwrap_or_else(|| "*".into()),
    }))
}

/// `--force-cpdir from_dir to_dir`
fn parse_force_cpdir(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens = parse_file_tokens(args);
    Ok(Statement::ForceCpdir(ForceCpdirCmd {
        span,
        source: strip_quotes(tokens.first().copied().unwrap_or("")).into(),
        dest: strip_quotes(tokens.get(1).copied().unwrap_or("")).into(),
    }))
}

/// `--write_line text file`
fn parse_write_line(args: &str, span: Span) -> Result<Statement, ParseError> {
    let tokens = parse_file_tokens(args);
    Ok(Statement::WriteLine(WriteLineCmd {
        span,
        text: tokens.first().copied().unwrap_or("").into(),
        file: tokens.get(1).map(|s| strip_quotes(*s).into()).unwrap_or_else(|| "".into()),
    }))
}

// ---------------------------------------------------------------------------
// Toggle commands
