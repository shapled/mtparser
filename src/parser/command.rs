use winnow::token::take_while;
use winnow::Parser;

use crate::ast::commands::*;
use crate::ast::statement::*;
use crate::ast::text::InterpolatedText;
use crate::ast::Span;
use crate::error::ParseError;
use crate::parser::ParseContext;
use crate::version::MysqlVersion;

/// Strip surrounding quotes from an ARG_STRING argument.
/// mysqltest.cc treats `'`, `` ` ``, `"` as delimiters for ARG_STRING parameters:
/// if the argument starts with a quote, find the matching close and return the inner text.
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
        // No matching close quote — return as-is (mysqltest.cc does the same)
    }
    s
}

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
        "source" => Ok(Statement::Source(SourceCmd { span, file: strip_quotes(args).into() })),
        "skip" => Ok(Statement::Skip(SkipCmd { span, message: { let m = args.trim(); if m.is_empty() { None } else { Some(m.into()) } } })),
        "die" => Ok(Statement::Die(DieCmd { span, message: { let m = args.trim(); if m.is_empty() { None } else { Some(m.into()) } } })),
        "exit" => Ok(Statement::Exit(ExitCmd { span })),
        "exec" => Ok(Statement::Exec(ExecCmd { span, command: args.trim().into() })),
        "execw" => Ok(Statement::Execw(ExecwCmd { span, command: args.trim().into() })),
        "exec_in_background" => Ok(Statement::ExecInBackground(ExecInBackgroundCmd { span, command: args.trim().into() })),
        "sleep" => Ok(Statement::Sleep(SleepCmd { span, seconds: args.trim().to_string() })),
        "reap" => Ok(Statement::Reap(ReapCmd { span })),

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

        "horizontal_results" => Ok(Statement::HorizontalResults(HorizontalResultsCmd { span })),
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
        | "disable_parsing" | "enable_parsing"
        | "disable_async_client" | "enable_async_client"
        | "disable_prepare_warnings" | "enable_prepare_warnings" => {
            parse_toggle(name, args, span)
        }

        "delimiter" => {
            let new_delim = args.trim().to_string();
            if new_delim.is_empty() {
                return Err(ParseError::Syntax { message: "delimiter requires an argument".to_string(), span });
            }
            ctx.delimiter = new_delim.clone();
            Ok(Statement::Delimiter(DelimiterCmd { span, new_delimiter: new_delim }))
        }

        "write_file" => Ok(Statement::WriteFile(WriteFileCmd { span, filename: strip_quotes(args).into(), end_marker: String::new(), content: String::new().into() })),
        "append_file" => Ok(Statement::AppendFile(AppendFileCmd { span, filename: strip_quotes(args).into(), end_marker: String::new(), content: String::new().into() })),
        "remove_file" => parse_remove_file(args, span),
        "remove_files_wildcard" => parse_remove_files_wildcard(args, span),
        "copy_file" => parse_copy_file(args, span),
        "move_file" => parse_move_file(args, span),
        "mkdir" => Ok(Statement::Mkdir(MkdirCmd { span, dir: strip_quotes(args).into() })),
        "rmdir" => Ok(Statement::Rmdir(RmdirCmd { span, dir: strip_quotes(args).into() })),
        "chmod" => parse_chmod(args, span),
        "diff_files" => parse_diff_files(args, span),
        "file_exists" => Ok(Statement::FileExists(FileExistsCmd { span, file: strip_quotes(args).into() })),
        "cat_file" => Ok(Statement::CatFile(CatFileCmd { span, file: strip_quotes(args).into() })),
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
fn skip_command_name<'a>(name: &'a str, input: &'a str) -> &'a str {
    if input.len() <= name.len() { return ""; }
    input[name.len()..].trim_start()
}

// --- Command-specific parsers (using winnow combinators) ---

/// Parse `--let $var = value` or `--let $var = \`query\``.
fn parse_let(args: &str, span: Span) -> Result<Statement, ParseError> {
    let trimmed = args.trim();
    let rest = trimmed.strip_prefix('$').unwrap_or(trimmed);
    let eq_pos = rest.find('=');
    if let Some(eq_pos) = eq_pos {
        let var_name = rest[..eq_pos].trim().to_string();
        let value_str = rest[eq_pos + 1..].trim();
        if var_name.is_empty() {
            return Err(ParseError::Syntax { message: format!("invalid let syntax: {}", trimmed), span });
        }
        // Use winnow combinators to detect backtick-enclosed query
        if let Some(query) = value_str.strip_prefix('`') {
            if let Some(query) = query.strip_suffix('`') {
                return Ok(Statement::Let(LetCmd {
                    span,
                    variable: var_name,
                    value: LetValue::Query(crate::ast::expr::QueryExpr::new(crate::ast::Span::dummy(), query.to_string())),
                }));
            }
        }
        return Ok(Statement::Let(LetCmd { span, variable: var_name, value: LetValue::Literal(value_str.to_string()) }));
    }
    Err(ParseError::Syntax { message: format!("invalid let syntax: {}", trimmed), span })
}

/// Parse `--inc $var` using winnow `take_while` for the variable name.
fn parse_inc(args: &str, span: Span) -> Result<Statement, ParseError> {
    let var = parse_dollar_var(args)?;
    if var.is_empty() { return Err(ParseError::Syntax { message: "inc requires a variable name ($var)".to_string(), span }); }
    Ok(Statement::Inc(IncCmd { span, variable: var }))
}

/// Parse `--dec $var` using winnow `take_while` for the variable name.
fn parse_dec(args: &str, span: Span) -> Result<Statement, ParseError> {
    let var = parse_dollar_var(args)?;
    if var.is_empty() { return Err(ParseError::Syntax { message: "dec requires a variable name ($var)".to_string(), span }); }
    Ok(Statement::Dec(DecCmd { span, variable: var }))
}

/// Extract `$variable_name` from input using winnow combinators.
/// Returns the variable name without the `$` prefix, or empty string on failure.
fn parse_dollar_var(args: &str) -> Result<String, ParseError> {
    let trimmed = args.trim();
    let Some(rest) = trimmed.strip_prefix('$') else {
        return Ok(String::new());
    };
    let mut stream = rest;
    let name = take_while::<_, _, ()>(1.., ('a'..='z', 'A'..='Z', '0'..='9', '_'))
        .parse_next(&mut stream)
        .unwrap_or("");
    Ok(name.to_string())
}

/// Parse `--expr $var = expression`.
fn parse_expr(args: &str, span: Span) -> Result<Statement, ParseError> {
    let trimmed = args.trim();
    if let Some(rest) = trimmed.strip_prefix('$') {
        if let Some(eq_pos) = rest.find('=') {
            let var_name = rest[..eq_pos].trim().to_string();
            let expression = rest[eq_pos + 1..].trim().to_string();
            return Ok(Statement::Expr(ExprCmd { span, variable: var_name, expression }));
        }
    }
    Err(ParseError::Syntax { message: format!("invalid expr syntax: {}", trimmed), span })
}

/// Parse `--error [code1, code2, ...]`.
fn parse_error(args: &str, span: Span) -> Result<Statement, ParseError> {
    let trimmed = args.trim();
    if trimmed.is_empty() { return Ok(Statement::Error(ErrorCmd { span, error_codes: vec![] })); }
    let codes: Vec<InterpolatedText> = trimmed.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).map(Into::into).collect();
    Ok(Statement::Error(ErrorCmd { span, error_codes: codes }))
}

/// Parse `--connect(name, host, user, pass, db, port, socket)` using winnow combinators.
fn parse_connect(args: &str, span: Span) -> Result<Statement, ParseError> {
    let trimmed = args.trim();
    if let Some(inner) = trimmed.strip_prefix('(') {
        if let Some(inner) = inner.strip_suffix(')') {
            // Use winnow separated-like parsing for comma-separated args
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

fn parse_change_user(args: &str, span: Span) -> Result<Statement, ParseError> {
    let trimmed = args.trim();
    if trimmed.is_empty() { return Ok(Statement::ChangeUser(ChangeUserCmd { span, user: None, password: None, database: None })); }
    let parts: Vec<&str> = trimmed.split(',').collect();
    Ok(Statement::ChangeUser(ChangeUserCmd { span, user: parts.first().map(|s| s.trim().into()), password: parts.get(1).map(|s| s.trim().into()), database: parts.get(2).map(|s| s.trim().into()) }))
}

fn parse_replace_result(args: &str, span: Span) -> Result<Statement, ParseError> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut replacements = Vec::new();
    let mut i = 0;
    while i + 1 < parts.len() { replacements.push((parts[i].into(), parts[i + 1].into())); i += 2; }
    Ok(Statement::ReplaceResult(ReplaceResultCmd { span, replacements }))
}

fn parse_replace_column(args: &str, span: Span) -> Result<Statement, ParseError> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut replacements = Vec::new();
    let mut i = 0;
    while i + 2 < parts.len() { replacements.push(ReplaceColumnItem { column: parts[i].to_string(), old_value: parts[i + 1].into(), new_value: parts[i + 2].into() }); i += 3; }
    Ok(Statement::ReplaceColumn(ReplaceColumnCmd { span, replacements }))
}

/// Parse `--replace_regex s/pattern/replacement/flags` or `/pattern/replacement/flags`.
fn parse_replace_regex(ctx: &ParseContext, args: &str, span: Span) -> Result<Statement, ParseError> {
    parse_replace_regex_versioned(args, span, &ctx.config.version)
}

/// Parse `--replace_regex` with version-aware syntax.
///
/// MySQL: only `s/pattern/replacement/flags` or `/pattern/replacement/flags`.
/// MariaDB: any single-char delimiter, paired delimiters `()`, `[]`, `{}`, `<>`,
/// backslash escaping, and the `s/` prefix is treated as regular single-char delimiter.
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
    let (rest, sep) = if let Some(rest) = trimmed.strip_prefix("s/") {
        (rest, '/')
    } else if let Some(rest) = trimmed.strip_prefix('/') {
        (rest, '/')
    } else {
        return Err(ParseError::Syntax { message: format!("invalid replace_regex syntax: {}", trimmed), span });
    };

    let sep1 = rest.find(sep).unwrap_or(rest.len());
    let pattern = rest[..sep1].to_string();
    let after1 = if sep1 < rest.len() { &rest[sep1 + 1..] } else { "" };
    let sep2 = after1.find(sep);
    let (replacement, flags) = if let Some(sep2) = sep2 {
        let replacement = after1[..sep2].to_string();
        let flags_str = &after1[sep2 + 1..];
        let flags = if flags_str.is_empty() { None } else { Some(flags_str.to_string()) };
        (replacement, flags)
    } else {
        (after1.to_string(), None)
    };

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

fn parse_toggle(name: &str, args: &str, span: Span) -> Result<Statement, ParseError> {
    let (enabled, kind) = if let Some(k) = name.strip_prefix("enable_") { (true, toggle_kind(k)) } else if let Some(k) = name.strip_prefix("disable_") { (false, toggle_kind(k)) } else { return Err(ParseError::UnknownCommand { command: name.to_string(), span, suggestion: None }); };
    let once = args.trim().eq_ignore_ascii_case("ONCE");
    Ok(Statement::Toggle(ToggleCmd { span, kind, enabled, once }))
}

fn toggle_kind(name: &str) -> ToggleKind {
    match name {
        "warnings" => ToggleKind::Warnings, "query_log" => ToggleKind::QueryLog, "result_log" => ToggleKind::ResultLog,
        "info" => ToggleKind::Info, "metadata" => ToggleKind::Metadata, "ps_protocol" => ToggleKind::PsProtocol,
        "reconnect" => ToggleKind::Reconnect, "connect_log" => ToggleKind::ConnectLog,
        "session_track_info" => ToggleKind::SessionTrackInfo, "testcase" => ToggleKind::Testcase,
        "parsing" => ToggleKind::Parsing, "async_client" => ToggleKind::AsyncClient,
        "prepare_warnings" => ToggleKind::PrepareWarnings,
        _ => ToggleKind::Warnings,
    }
}

fn parse_remove_file(args: &str, span: Span) -> Result<Statement, ParseError> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    Ok(Statement::RemoveFile(RemoveFileCmd { span, file: strip_quotes(parts.first().unwrap_or(&"")).into(), timeout: parts.get(1).map(|s| s.to_string()) }))
}

fn parse_remove_files_wildcard(args: &str, span: Span) -> Result<Statement, ParseError> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    Ok(Statement::RemoveFilesWildcard(RemoveFilesWildcardCmd { span, dir: strip_quotes(parts.first().unwrap_or(&"")).into(), pattern: (*parts.get(1).unwrap_or(&"")).into(), timeout: parts.get(2).map(|s| s.to_string()) }))
}

fn parse_copy_file(args: &str, span: Span) -> Result<Statement, ParseError> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    Ok(Statement::CopyFile(CopyFileCmd { span, source: strip_quotes(parts.first().unwrap_or(&"")).into(), dest: strip_quotes(parts.get(1).unwrap_or(&"")).into(), retry: parts.get(2).map(|s| s.to_string()) }))
}

fn parse_move_file(args: &str, span: Span) -> Result<Statement, ParseError> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    Ok(Statement::MoveFile(MoveFileCmd { span, source: strip_quotes(parts.first().unwrap_or(&"")).into(), dest: strip_quotes(parts.get(1).unwrap_or(&"")).into(), timeout: parts.get(2).map(|s| s.to_string()) }))
}

fn parse_chmod(args: &str, span: Span) -> Result<Statement, ParseError> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    Ok(Statement::Chmod(ChmodCmd { span, mode: parts.first().unwrap_or(&"").to_string(), file: strip_quotes(parts.get(1).unwrap_or(&"")).into() }))
}

fn parse_diff_files(args: &str, span: Span) -> Result<Statement, ParseError> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    Ok(Statement::DiffFiles(DiffFilesCmd { span, file1: strip_quotes(parts.first().unwrap_or(&"")).into(), file2: strip_quotes(parts.get(1).unwrap_or(&"")).into() }))
}

fn parse_list_files(args: &str, span: Span) -> Result<Statement, ParseError> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    Ok(Statement::ListFiles(ListFilesCmd { span, dir: parts.first().map(|s| strip_quotes(s).into()), pattern: parts.get(1).map(|s| s.trim().into()) }))
}

fn parse_copy_files_wildcard(args: &str, span: Span) -> Result<Statement, ParseError> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    Ok(Statement::CopyFilesWildcard(CopyFilesWildcardCmd { span, source: strip_quotes(parts.first().unwrap_or(&"")).into(), dest: strip_quotes(parts.get(1).unwrap_or(&"")).into(), retry: parts.get(2).map(|s| s.to_string()) }))
}
