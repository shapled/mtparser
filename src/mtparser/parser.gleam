//// Stages 2+3: classify each line, then parse command arguments.
////
//// Reading model (terminology per the official docs):
//// - The ROUTING UNIT is the statement's first segment. It is read BY each
////    statement parser (`read_unit`) for its own needs, not pre-read by the
////    caller: `--` lines end at the newline (comment-mode path in
////    mysqltest.cc), bare lines end at the delimiter.
//// - Each statement owns its full extent. Simple statements (echo, SQL,
////    cat_file, assert) end with the routing unit; block statements extend
////    via their own TERMINATOR — the close marker for append_file (and
////    write_file/perl later), `}` for if/while — read by the statement's
////    own parser.
//// - Command parsers strictly parse their own arguments (tokenization,
////    escapes, evaluation, quote rules are command grammar); the caller
////    only peeks the first word for routing.
////
//// `parse_statement` is the single hub:
//// 1. blank → no statement (blank lines are skipped in the main loop)
//// 2. `#…` → `Comment`
//// 3. `--` prefix → parse the dash command (`parse_dash_command_statement`)
////    in endline-delimiter mode; a `--` line IS a command, so unknown names
////    are real errors here
//// 4. anything else → route by the first word (`parse_command_statement`);
////    an unknown bare statement is just a query (`parse_sql_statement`).
////    Errors from known commands are real errors in both forms.
////

import gleam/int
import gleam/list
import gleam/option
import gleam/string
import mtparser/ast.{
  type ComparisonOp, type ComparisonRhs, type ConnectParams, type Expr,
  type InterpolatedText, type LetValue, type Line, type LogicalOp,
  type RegexRule, type Span, type Statement, type TextPart, And, AppendFile,
  Assert, CatFile, ChangeUser, CharacterSet, Chmod, Closure, Comment, Comparison,
  Connect, ConnectParams, Connection, ConvertError, CopyFile, CopyFilesWildcard,
  DashCommand, Dec, Delimiter, Die, DiffFiles, DirtyClose, DisableTestcase,
  Disconnect, Echo, EnableTestcase, EndTimer, Eq, Escape, Eval, Evalp, Exec,
  ExecInBackground, Execw, Exit, Expr, FileExists, ForceCpdir, ForceRmdir, Ge,
  Gt, HorizontalResults, If, Inc, Integer, Le, Let, Line, ListFiles,
  ListFilesAppendFile, ListFilesWriteFile, Literal, Logical, LowercaseResults,
  Lt, Marker, Mkdir, MoveFile, Negated, Neq, OptimizerTrace, Or, Output,
  PartiallySortedResult, Perl, Ping, PsBind, PsClose, PsExecute, PsPrepare,
  Query, QueryAttributes, QueryCommand, QueryExpr, QueryGetValue,
  QueryHorizontal, QueryValue, QueryVertical, RealSleep, Reap, RegexRule,
  RemoveFile, RemoveFilesWildcard, ReplaceColumn, ReplaceNumericRound,
  ReplaceRegex, ReplaceRegexVariable, ReplaceResult, Require, ResetConnection,
  ResultFormat, RhsInteger, RhsQuery, RhsString, RhsVariable, Rmdir,
  SaveMasterPos, Send, SendEval, SendQuit, SendShutdown, ShutdownServer, Skip,
  SkipIfHypergraph, Sleep, SortedResult, Source, Span, Sql, StartTimer,
  SyncSlaveWithMaster, SyncWithMaster, System, Text, Toggle, Var, Variable,
  VariableRef, VerticalResults, WaitForSlaveToStop, While, WriteFile, WriteLine,
}
import mtparser/error.{type ParserError}
import mtparser/helper.{
  all_but_last, arg_offset, comma_segments, drop_word, first_word, gs_len,
  is_alnum, is_digit, is_ident, leading_ws, line_span, nth, prefix_len, sub_span,
  take_till_any, take_till_list, take_token, take_while_list, trim_start_list,
  utf16_size, ws_tokens,
}

/// Parser state, threaded explicitly through the loop.
///
/// `next_endline_as_delimiter` reflects the `--` prefix: while a `--`
/// command's routing unit is being read, the endline (a newline, or the end
/// of file for a final line without one) acts as its delimiter ("A `--`
/// command is terminated by a newline, regardless of how many delimiters it
/// contains"). The flag is one-shot: set by `parse_dash_command_statement`,
/// consumed by the one-line read in `read_unit` — reaching the endline
/// consumes it, whoever reads there.
pub type ParserState {
  ParserState(delimiter: String, next_endline_as_delimiter: Bool)
}

/// Parse all lines into statements.
pub fn parse_statements(
  all: List(Line),
) -> Result(#(List(Statement), ParserState), ParserError) {
  loop(all, ParserState(delimiter: ";", next_endline_as_delimiter: False), [])
}

fn loop(
  lines: List(Line),
  state: ParserState,
  acc: List(Statement),
) -> Result(#(List(Statement), ParserState), ParserError) {
  case lines {
    [] -> Ok(#(list.reverse(acc), state))
    [line, ..rest] ->
      // Blank lines produce no statement.
      case string.trim(line.text) {
        "" -> loop(rest, state, acc)
        _ ->
          // Block terminators belong to an open if/while — a stray one at
          // top level is an error (mysqltest exits here too), including the
          // `-- }` / `-- end` forms.
          case block_terminator(line) {
            option.Some(#(sp, _)) ->
              Error(error.Syntax(
                message: "block terminator without an open block",
                span: sp,
              ))
            option.None ->
              case parse_statement(line, rest, state) {
                Ok(#(stmt, remaining, state)) ->
                  loop(remaining, state, [stmt, ..acc])
                Error(e) -> Error(e)
              }
          }
      }
  }
}

fn parse_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let trimmed = string.trim(line.text)
  case string.starts_with(trimmed, "#") {
    True ->
      Ok(#(
        Comment(span: line_span(line), text: string.remove_prefix(trimmed, "#")),
        rest,
        state,
      ))
    False ->
      case string.starts_with(trimmed, "--") {
        True -> parse_dash_command_statement(line, rest, state)
        False -> parse_command_statement(line, rest, state)
      }
  }
}

/// Parse a `--` command in endline-delimiter mode: the routing unit is one
/// line with delimiters inert ("A `--` command is terminated by a newline,
/// regardless of how many delimiters it contains"). The parsed statement is
/// wrapped in a `DashCommand` carrying the dash prefix's span, so the AST
/// marks the form and the prefix stays addressable for tooling.
fn parse_dash_command_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let stmt_state = ParserState(..state, next_endline_as_delimiter: True)
  case parse_command_statement(line, rest, stmt_state) {
    Ok(#(stmt, remaining, state)) -> {
      let leading = leading_ws(line)
      let dash_offset = line.offset + leading
      let base = line_span(line)
      // The wrapper spans the whole trimmed line: dash prefix through end.
      let len = base.offset + base.len - dash_offset
      Ok(#(
        DashCommand(
          span: Span(
            line: line.number,
            column: leading + 1,
            offset: dash_offset,
            len: len,
          ),
          dash: Span(
            line: line.number,
            column: leading + 1,
            offset: dash_offset,
            len: 2,
          ),
          statement: stmt,
        ),
        remaining,
        state,
      ))
    }
    Error(e) -> Error(e)
  }
}

/// Route by the first word (a peek — no arguments are parsed here) and hand
/// the raw reading position to the per-command parser, which reads its own
/// routing unit and strictly parses its own arguments.
///
/// Unknown name: a `--` line is definitely a command → `UnknownCommand`
/// error (mysqltest.cc dies here); a bare statement is just a query →
/// `parse_sql_statement`. Errors from known commands are real errors.
fn parse_command_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let name = peek_name(line, state)
  case command_kind(name) {
    Ok(KEcho) -> parse_echo_statement(line, rest, state)
    Ok(KAssert) -> parse_assert_statement(line, rest, state)
    Ok(KCatFile) -> parse_cat_file_statement(line, rest, state)
    Ok(KAppendFile) ->
      parse_write_append_statement(line, rest, state, KAppendFile)
    Ok(KChangeUser) -> parse_change_user_statement(line, rest, state)
    Ok(KCharacterSet) -> parse_character_set_statement(line, rest, state)
    Ok(KChmod) -> parse_chmod_statement(line, rest, state)
    Ok(KConnect) -> parse_connect_statement(line, rest, state)
    Ok(KConnection) -> parse_connection_statement(line, rest, state)
    Ok(KCopyFile) -> parse_copy_file_statement(line, rest, state, KCopyFile)
    Ok(KCopyFilesWildcard) ->
      parse_copy_files_wildcard_statement(line, rest, state)
    Ok(KDec) -> parse_inc_dec_statement(line, rest, state, KDec)
    Ok(KDelimiter) -> parse_delimiter_statement(line, rest, state)
    Ok(KDie) -> parse_die_statement(line, rest, state)
    Ok(KDiffFiles) -> parse_diff_files_statement(line, rest, state)
    Ok(KDirtyClose) -> parse_dirty_close_statement(line, rest, state)
    Ok(KDisconnect) -> parse_disconnect_statement(line, rest, state)
    Ok(KDisableTestcase) -> parse_disable_testcase_statement(line, rest, state)
    Ok(KEnableTestcase) -> parse_enable_testcase_statement(line, rest, state)
    Ok(KToggle(kind: kind, enable: enable)) ->
      parse_toggle_statement(line, rest, state, kind, enable)
    Ok(KEndTimer) -> parse_no_args_statement(line, rest, state, KEndTimer)
    Ok(KError) -> parse_error_statement(line, rest, state)
    Ok(KEval) -> parse_itext_arg_statement(line, rest, state, KEval)
    Ok(KExec) -> parse_itext_arg_statement(line, rest, state, KExec)
    Ok(KExecInBackground) ->
      parse_itext_arg_statement(line, rest, state, KExecInBackground)
    Ok(KExecw) -> parse_itext_arg_statement(line, rest, state, KExecw)
    Ok(KExit) -> parse_no_args_statement(line, rest, state, KExit)
    Ok(KExpr) -> parse_expr_cmd_statement(line, rest, state)
    Ok(KFileExists) ->
      parse_file_exists_statement(line, rest, state, KFileExists)
    Ok(KForceCpdir) -> parse_two_token_statement(line, rest, state, KForceCpdir)
    Ok(KForceRmdir) -> parse_one_token_statement(line, rest, state, KForceRmdir)
    Ok(KHorizontalResults) ->
      parse_no_args_statement(line, rest, state, KHorizontalResults)
    Ok(KIf) -> parse_flow_statement(line, rest, state, KIf)
    Ok(KInc) -> parse_inc_dec_statement(line, rest, state, KInc)
    Ok(KLet) -> parse_let_statement(line, rest, state)
    Ok(KListFiles) -> parse_list_files_statement(line, rest, state)
    Ok(KListFilesAppendFile) ->
      parse_list_files_file_statement(line, rest, state, KListFilesAppendFile)
    Ok(KListFilesWriteFile) ->
      parse_list_files_file_statement(line, rest, state, KListFilesWriteFile)
    Ok(KLowercaseResults) ->
      parse_no_args_statement(line, rest, state, KLowercaseResults)
    Ok(KMkdir) -> parse_one_token_statement(line, rest, state, KMkdir)
    Ok(KMoveFile) -> parse_copy_file_statement(line, rest, state, KMoveFile)
    Ok(KOptimizerTrace) ->
      parse_no_args_statement(line, rest, state, KOptimizerTrace)
    Ok(KOutput) -> parse_one_token_statement(line, rest, state, KOutput)
    Ok(KEvalp) -> parse_itext_arg_statement(line, rest, state, KEvalp)
    Ok(KPsPrepare) -> parse_itext_arg_statement(line, rest, state, KPsPrepare)
    Ok(KPsBind) -> parse_ps_bind_statement(line, rest, state)
    Ok(KPsExecute) -> parse_no_args_statement(line, rest, state, KPsExecute)
    Ok(KPsClose) -> parse_no_args_statement(line, rest, state, KPsClose)
    Ok(KSkipIfHypergraph) ->
      parse_no_args_statement(line, rest, state, KSkipIfHypergraph)
    Ok(KWriteLine) -> parse_write_line_statement(line, rest, state)
    Ok(KPerl) -> parse_perl_statement(line, rest, state)
    Ok(KPing) -> parse_no_args_statement(line, rest, state, KPing)
    Ok(KQuery) -> parse_itext_arg_statement(line, rest, state, KQuery)
    Ok(KQueryAttributes) -> parse_query_attributes_statement(line, rest, state)
    Ok(KQueryHorizontal) ->
      parse_itext_arg_statement(line, rest, state, KQueryHorizontal)
    Ok(KQueryVertical) ->
      parse_itext_arg_statement(line, rest, state, KQueryVertical)
    Ok(KReap) -> parse_no_args_statement(line, rest, state, KReap)
    Ok(KRealSleep) -> parse_sleep_statement(line, rest, state, KRealSleep)
    Ok(KRequire) -> parse_require_statement(line, rest, state)
    Ok(KResetConnection) ->
      parse_no_args_statement(line, rest, state, KResetConnection)
    Ok(KRemoveFile) ->
      parse_file_exists_statement(line, rest, state, KRemoveFile)
    Ok(KRemoveFilesWildcard) ->
      parse_remove_files_wildcard_statement(line, rest, state)
    Ok(KReplaceColumn) -> parse_replace_column_statement(line, rest, state)
    Ok(KReplaceNumericRound) ->
      parse_replace_numeric_round_statement(line, rest, state)
    Ok(KReplaceRegex) -> parse_replace_regex_statement(line, rest, state)
    Ok(KReplaceResult) -> parse_replace_result_statement(line, rest, state)
    Ok(KResultFormat) -> parse_result_format_statement(line, rest, state)
    Ok(KPartiallySortedResult) ->
      parse_partially_sorted_statement(line, rest, state)
    Ok(KRmdir) -> parse_one_token_statement(line, rest, state, KRmdir)
    Ok(KSaveMasterPos) ->
      parse_no_args_statement(line, rest, state, KSaveMasterPos)
    Ok(KSend) -> parse_send_statement(line, rest, state, KSend)
    Ok(KSendEval) -> parse_send_statement(line, rest, state, KSendEval)
    Ok(KSendQuit) -> parse_itext_arg_statement(line, rest, state, KSendQuit)
    Ok(KSendShutdown) ->
      parse_no_args_statement(line, rest, state, KSendShutdown)
    Ok(KShutdownServer) -> parse_shutdown_server_statement(line, rest, state)
    Ok(KSkip) -> parse_skip_statement(line, rest, state)
    Ok(KSleep) -> parse_sleep_statement(line, rest, state, KSleep)
    Ok(KSortedResult) ->
      parse_no_args_statement(line, rest, state, KSortedResult)
    Ok(KSource) -> parse_source_statement(line, rest, state)
    Ok(KStartTimer) -> parse_no_args_statement(line, rest, state, KStartTimer)
    Ok(KSyncSlaveWithMaster) -> parse_sync_slave_statement(line, rest, state)
    Ok(KSyncWithMaster) -> parse_sync_with_master_statement(line, rest, state)
    Ok(KSystem) -> parse_itext_arg_statement(line, rest, state, KSystem)
    Ok(KVerticalResults) ->
      parse_no_args_statement(line, rest, state, KVerticalResults)
    Ok(KWaitForSlaveToStop) ->
      parse_no_args_statement(line, rest, state, KWaitForSlaveToStop)
    Ok(KWhile) -> parse_flow_statement(line, rest, state, KWhile)
    Ok(KWriteFile) ->
      parse_write_append_statement(line, rest, state, KWriteFile)
    Error(Nil) ->
      case state.next_endline_as_delimiter {
        True ->
          Error(error.UnknownCommand(
            command: name,
            span: after_dash_span(line),
            suggestion: option.None,
          ))
        False -> parse_sql_statement(line, rest, state)
      }
  }
}

/// The known-command routing table (single source).
type CmdKind {
  KEcho
  KAssert
  KCatFile
  KAppendFile
  KChangeUser
  KCharacterSet
  KChmod
  KConnect
  KConnection
  KCopyFile
  KCopyFilesWildcard
  KDec
  KDelimiter
  KDie
  KDiffFiles
  KDirtyClose
  KDisconnect
  KDisableTestcase
  KEnableTestcase
  KEndTimer
  KError
  KEval
  KExec
  KExecInBackground
  KExecw
  KExit
  KExpr
  KFileExists
  KForceCpdir
  KForceRmdir
  KHorizontalResults
  KIf
  KInc
  KLet
  KListFiles
  KListFilesAppendFile
  KListFilesWriteFile
  KLowercaseResults
  KMkdir
  KMoveFile
  KEvalp
  KOptimizerTrace
  KOutput
  KPerl
  KPsBind
  KPsClose
  KPsExecute
  KPsPrepare
  KPing
  KQuery
  KQueryAttributes
  KQueryHorizontal
  KQueryVertical
  KReap
  KRealSleep
  KRemoveFile
  KRequire
  KRemoveFilesWildcard
  KReplaceColumn
  KReplaceNumericRound
  KReplaceRegex
  KReplaceResult
  KResetConnection
  KResultFormat
  KPartiallySortedResult
  KRmdir
  KSaveMasterPos
  KSend
  KSendEval
  KSendQuit
  KSendShutdown
  KShutdownServer
  KSkip
  KSkipIfHypergraph
  KSleep
  KSortedResult
  KSource
  KStartTimer
  KSyncSlaveWithMaster
  KSyncWithMaster
  KSystem
  KVerticalResults
  KWaitForSlaveToStop
  KWhile
  KWriteLine
  KWriteFile
  KToggle(kind: ast.ToggleKind, enable: Bool)
}

fn command_kind(name: String) -> Result(CmdKind, Nil) {
  case name {
    "echo" -> Ok(KEcho)
    "assert" -> Ok(KAssert)
    "cat_file" -> Ok(KCatFile)
    "append_file" -> Ok(KAppendFile)
    "change_user" -> Ok(KChangeUser)
    "character_set" -> Ok(KCharacterSet)
    "chmod" -> Ok(KChmod)
    "connect" -> Ok(KConnect)
    "connection" -> Ok(KConnection)
    "copy_file" -> Ok(KCopyFile)
    "copy_files_wildcard" -> Ok(KCopyFilesWildcard)
    "dec" -> Ok(KDec)
    "delimiter" -> Ok(KDelimiter)
    "die" -> Ok(KDie)
    "diff_files" -> Ok(KDiffFiles)
    "dirty_close" -> Ok(KDirtyClose)
    "disconnect" -> Ok(KDisconnect)
    "enable_testcase" -> Ok(KEnableTestcase)
    "disable_testcase" -> Ok(KDisableTestcase)
    "end_timer" -> Ok(KEndTimer)
    "error" -> Ok(KError)
    "eval" -> Ok(KEval)
    "exec" -> Ok(KExec)
    "exec_in_background" -> Ok(KExecInBackground)
    "execw" -> Ok(KExecw)
    "exit" -> Ok(KExit)
    "expr" -> Ok(KExpr)
    "file_exists" -> Ok(KFileExists)
    "force-cpdir" -> Ok(KForceCpdir)
    "force-rmdir" -> Ok(KForceRmdir)
    "horizontal_results" -> Ok(KHorizontalResults)
    "if" -> Ok(KIf)
    "inc" -> Ok(KInc)
    "let" -> Ok(KLet)
    "list_files" -> Ok(KListFiles)
    "list_files_append_file" -> Ok(KListFilesAppendFile)
    "list_files_write_file" -> Ok(KListFilesWriteFile)
    "lowercase_result" -> Ok(KLowercaseResults)
    "mkdir" -> Ok(KMkdir)
    "move_file" -> Ok(KMoveFile)
    "optimizer_trace" -> Ok(KOptimizerTrace)
    "output" -> Ok(KOutput)
    "perl" -> Ok(KPerl)
    "ps_prepare" -> Ok(KPsPrepare)
    "ps_bind" -> Ok(KPsBind)
    "ps_execute" -> Ok(KPsExecute)
    "ps_close" -> Ok(KPsClose)
    "ping" -> Ok(KPing)
    "query" -> Ok(KQuery)
    "query_attributes" -> Ok(KQueryAttributes)
    "query_horizontal" -> Ok(KQueryHorizontal)
    "query_vertical" -> Ok(KQueryVertical)
    "reap" -> Ok(KReap)
    "real_sleep" -> Ok(KRealSleep)
    "require" -> Ok(KRequire)
    "remove_file" -> Ok(KRemoveFile)
    "remove_files_wildcard" -> Ok(KRemoveFilesWildcard)
    "replace_column" -> Ok(KReplaceColumn)
    "replace_numeric_round" -> Ok(KReplaceNumericRound)
    "replace_regex" -> Ok(KReplaceRegex)
    "replace_result" -> Ok(KReplaceResult)
    "reset_connection" -> Ok(KResetConnection)
    "result_format" -> Ok(KResultFormat)
    "partially_sorted_result" -> Ok(KPartiallySortedResult)
    "rmdir" -> Ok(KRmdir)
    "save_master_pos" -> Ok(KSaveMasterPos)
    "send" -> Ok(KSend)
    "send_eval" -> Ok(KSendEval)
    "send_quit" -> Ok(KSendQuit)
    "send_shutdown" -> Ok(KSendShutdown)
    "shutdown_server" -> Ok(KShutdownServer)
    "skip" -> Ok(KSkip)
    "skip_if_hypergraph" -> Ok(KSkipIfHypergraph)
    "sleep" -> Ok(KSleep)
    "sorted_result" -> Ok(KSortedResult)
    "source" -> Ok(KSource)
    "start_timer" -> Ok(KStartTimer)
    "sync_slave_with_master" -> Ok(KSyncSlaveWithMaster)
    "sync_with_master" -> Ok(KSyncWithMaster)
    "system" -> Ok(KSystem)
    "vertical_results" -> Ok(KVerticalResults)
    "wait_for_slave_to_stop" -> Ok(KWaitForSlaveToStop)
    "while" -> Ok(KWhile)
    "write_line" -> Ok(KWriteLine)
    "evalp" -> Ok(KEvalp)
    "write_file" -> Ok(KWriteFile)
    // Toggles: enable_X / disable_X for the known settings.
    _ ->
      case
        string.starts_with(name, "enable_"),
        string.starts_with(name, "disable_")
      {
        True, _ -> toggle_kind(string.remove_prefix(name, "enable_"), True)
        _, True -> toggle_kind(string.remove_prefix(name, "disable_"), False)
        _, _ -> Error(Nil)
      }
  }
}

/// The known `enable_X`/`disable_X` settings.
fn toggle_kind(setting: String, enable: Bool) -> Result(CmdKind, Nil) {
  let kind = case setting {
    "abort_on_error" -> Ok(ast.AbortOnError)
    "async_client" -> Ok(ast.AsyncClient)
    "connect_log" -> Ok(ast.ConnectLog)
    "info" -> Ok(ast.Info)
    "metadata" -> Ok(ast.Metadata)
    "ps_protocol" -> Ok(ast.PsProtocol)
    "query_log" -> Ok(ast.QueryLog)
    "result_log" -> Ok(ast.ResultLog)
    "rpl_parse" -> Ok(ast.RplParse)
    "session_track_info" -> Ok(ast.SessionTrackInfo)
    "parsing" -> Ok(ast.Parsing)
    "reconnect" -> Ok(ast.Reconnect)
    "prepare_warnings" -> Ok(ast.PrepareWarnings)
    "cursor_protocol" -> Ok(ast.CursorProtocol)
    "non_blocking_api" -> Ok(ast.NonBlockingApi)
    "ps2_protocol" -> Ok(ast.Ps2Protocol)
    "service_connection" -> Ok(ast.ServiceConnection)
    "view_protocol" -> Ok(ast.ViewProtocol)
    "column_names" -> Ok(ast.ColumnNames)
    "warnings" -> Ok(ast.Warnings)
    _ -> Error(Nil)
  }
  case kind {
    Ok(k) -> Ok(KToggle(kind: k, enable: enable))
    Error(Nil) -> Error(Nil)
  }
}

/// Peek the command name from the raw line: after the `--` prefix in
/// endline mode, otherwise from the line start.
fn peek_name(line: Line, state: ParserState) -> String {
  let text = string.trim(line.text)
  let text = case state.next_endline_as_delimiter {
    True -> text |> string.remove_prefix("--") |> string.trim_start
    False -> text
  }
  first_word(text) |> string.lowercase
}

/// Span of a `--` line's command text (after the prefix).
fn after_dash_span(line: Line) -> Span {
  let base = line_span(line)
  let stripped = string.remove_prefix(string.trim(line.text), "--")
  let full = string.trim_start(stripped)
  let ws = utf16_size(stripped) - utf16_size(full)
  Span(
    line: base.line,
    column: base.column + 2 + ws,
    offset: base.offset + 2 + ws,
    len: utf16_size(full),
  )
}

/// Read the ROUTING UNIT — the statement's first segment — the way the
/// current state says statements read:
///
/// - endline mode (`--`): the physical line minus the prefix — one line,
///   delimiters inert. This read consumes the mode.
/// - delimiter mode: lines accumulate until the state delimiter terminates
///   the statement — quote-aware (`"…"`, `'…'`, `` `…` `` suppress the
///   delimiter inside; the quote state carries across lines), may span
///   lines.
///
/// Returns the unit text (trimmed), its span (covering exactly that text),
/// the remaining lines, and the (possibly consumed) state.
fn read_unit(
  first: Line,
  rest: List(Line),
  state: ParserState,
) -> #(String, Span, List(Line), ParserState) {
  case state.next_endline_as_delimiter {
    True -> {
      let base = line_span(first)
      let stripped = string.remove_prefix(string.trim(first.text), "--")
      let full = string.trim_start(stripped)
      let ws = utf16_size(stripped) - utf16_size(full)
      #(
        full,
        Span(
          line: base.line,
          column: base.column + 2 + ws,
          offset: base.offset + 2 + ws,
          len: utf16_size(full),
        ),
        rest,
        ParserState(..state, next_endline_as_delimiter: False),
      )
    }
    False -> {
      let #(seen, termination, remaining) =
        accumulate([], [first, ..rest], state.delimiter)
      let ordered = list.reverse(seen)
      let full = string.trim(join_body(ordered, termination))
      #(
        full,
        Span(..body_span(ordered, termination), len: utf16_size(full)),
        remaining,
        state,
      )
    }
  }
}

/// Parse a SQL statement: the routing unit IS the statement — no further
/// structure is imposed on the text.
fn parse_sql_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  Ok(#(Sql(span: span, sql: full), remaining, state))
}

// ── per-command parsers ─────────────────────────────────────────────────────

/// `--echo text` / `echo text;`
///
/// Echo's argument grammar: the command name, whitespace, then the REST of
/// the routing unit verbatim — no token rules, no quote rules; `$var`
/// references are interpolated. Echo reads no block and changes no state.
fn parse_echo_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let arg_offset = span.offset + utf16_size(full) - utf16_size(arg)
  Ok(#(
    Echo(span: span, text: parse_interpolated_text_with_offset(arg, arg_offset)),
    remaining,
    state,
  ))
}

/// `assert (expr)` — the Rust version mis-parsed this as SQL; fixed here.
///
/// Assert's argument grammar: the command name, optional whitespace, then
/// the expression (optional parens, optional `!`, comparison / backtick
/// query / closure forms — see `expr`).
fn parse_assert_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let arg_offset = span.offset + utf16_size(full) - utf16_size(arg)
  case parse_expr(arg, arg_offset) {
    Ok(expression) ->
      Ok(#(Assert(span: span, expression: expression), remaining, state))
    Error(Nil) ->
      Error(error.InvalidExpression(
        message: "assert: invalid expression '" <> arg <> "'",
        span: span,
      ))
  }
}

/// `cat_file file_name` — cat_file's argument grammar: the command name,
/// whitespace, then a single whitespace-delimited token; anything after the
/// token is not part of the file name.
fn parse_cat_file_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let arg_start = utf16_size(full) - utf16_size(arg)
  let arg_span = sub_span(span, full, arg_start, 0)
  // The token is a PREFIX of the arg (the arg starts at the token).
  let token = take_token(arg)
  Ok(#(
    CatFile(
      span: sub_span(arg_span, arg, 0, utf16_size(token)),
      file: parse_interpolated_text_with_offset(token, arg_span.offset),
    ),
    remaining,
    state,
  ))
}

/// `append_file file_name [marker]` … content … `marker`
///
/// Append_file's argument grammar: the command name, whitespace, a single
/// filename token, then optionally a block marker. Beyond the routing unit,
/// the body is read until a line whose trimmed text equals the marker (the
/// statement's TERMINATOR); `#`-prefixed lines inside the body are content,
/// not comments.
/// First ARG_STRING argument with quote support: a quote at the start
/// opens and the matching quote closes (mysqltest.cc check_command_args);
/// anything after it (the optional block marker) is left alone. Returns
/// #(content, bytes consumed including quotes, was quoted).
fn quoted_filename(arg: String) -> #(String, Int, Bool) {
  let gs = string.to_graphemes(arg)
  case gs {
    [q, ..rest] ->
      case q == "'" || q == "\"" || q == "`" {
        True ->
          case take_till_list(rest, q) {
            Ok(#(inner, _)) -> {
              let content = string.concat(inner)
              #(content, 1 + utf16_size(content) + 1, True)
            }
            // Unterminated quote: the rest of the string is the argument
            // (ARG_STRING scans to the end).
            Error(Nil) -> {
              let content = string.concat(rest)
              #(content, 1 + utf16_size(content), True)
            }
          }
        False -> #(take_token(arg), utf16_size(take_token(arg)), False)
      }
    [] -> #("", 0, False)
  }
}

fn parse_write_append_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
  kind: CmdKind,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let arg_start = utf16_size(full) - utf16_size(arg)
  let arg = cut_inline_comment(arg)
  let arg_span = sub_span(span, full, arg_start, 0)
  // The filename may be quoted — filenames can contain spaces and special
  // characters such as `;` (mysqltest.cc ARG_STRING opens a quote at the
  // argument start and reads to the matching one).
  let #(filename, consumed, quoted) = quoted_filename(arg)
  let marker = string.trim(string.drop_start(arg, consumed))
  case filename {
    "" ->
      Error(error.Syntax(
        message: case kind {
          KWriteFile -> "write_file"
          _ -> "append_file"
        }
          <> ": filename must not be empty",
        span: span,
      ))
    _ -> {
      let marker_name = case marker {
        "" -> "EOF"
        _ -> marker
      }
      // Open marker: declared on the command line; zero-length when implicit.
      let open = case marker {
        "" ->
          Marker(
            name: marker_name,
            span: sub_span(arg_span, arg, utf16_size(arg), 0),
          )
        _ -> {
          let start = utf16_size(arg) - utf16_size(marker)
          Marker(
            name: marker_name,
            span: sub_span(arg_span, arg, start, utf16_size(marker)),
          )
        }
      }
      case read_block(remaining, marker_name) {
        Ok(#(content_lines, marker_line, remaining)) -> {
          let content =
            string.join(list.map(content_lines, fn(l) { l.text }), "\n")
          let leading = leading_ws(marker_line)
          let close =
            Marker(
              name: marker_name,
              span: Span(
                line: marker_line.number,
                column: leading + 1,
                offset: marker_line.offset + leading,
                len: utf16_size(marker_name),
              ),
            )
          // Whole statement: command line start .. end of the marker text.
          let end = marker_line.offset + leading + utf16_size(marker_name)
          let stmt_span =
            Span(
              line: span.line,
              column: span.column,
              offset: span.offset,
              len: end - span.offset,
            )
          let filename_itext =
            parse_interpolated_text_with_offset(
              filename,
              arg_span.offset
                + case quoted {
                True -> 1
                False -> 0
              },
            )
          // mysqltest.cc writes the block verbatim (read_until_delimiter →
          // str_to_file2) — no $variable evaluation.
          let stmt = case kind {
            KWriteFile ->
              WriteFile(
                span: stmt_span,
                filename: filename_itext,
                open: open,
                close: close,
                content: content,
              )
            _ ->
              AppendFile(
                span: stmt_span,
                filename: filename_itext,
                open: open,
                close: close,
                content: content,
              )
          }
          Ok(#(stmt, remaining, state))
        }
        Error(Nil) ->
          Error(error.UnterminatedBlock(
            command: case kind {
              KWriteFile -> "write_file"
              _ -> "append_file"
            },
            marker: marker_name,
            span: span,
          ))
      }
    }
  }
}

/// `change_user [user], [password], [db]` — change_user's argument grammar:
/// the command name, whitespace, then up to three comma-separated segments,
/// each trimmed; empty segments are None.
fn parse_change_user_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let arg_offset = arg_offset(span, full, arg)
  let segments = comma_segments(arg)
  Ok(#(
    ChangeUser(
      span: span,
      user: opt_itext(nth(segments, 0), arg_offset),
      password: opt_itext(nth(segments, 1), arg_offset),
      database: opt_itext(nth(segments, 2), arg_offset),
    ),
    remaining,
    state,
  ))
}

/// `character_set charset` — the rest of the unit, verbatim.
fn parse_character_set_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  Ok(#(
    CharacterSet(span: span, charset: full |> drop_word |> string.trim_start),
    remaining,
    state,
  ))
}

/// `chmod octal_mode file_name` — chmod's argument grammar: the command
/// name, whitespace, a four-digit octal mode token, then a filename token.
/// The mode is kept as text ("0644" preserves the leading zero).
fn parse_chmod_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, arg_offset) = command_arg(full, span)
  let tokens = ws_tokens(arg)
  Ok(#(
    Chmod(
      span: span,
      mode: case nth(tokens, 0) {
        Ok(#(mode, _)) -> mode
        Error(Nil) -> ""
      },
      file: case nth(tokens, 1) {
        Ok(#(tok, start)) ->
          parse_interpolated_text_with_offset(tok, arg_offset + start)
        Error(Nil) -> parse_interpolated_text("")
      },
    ),
    remaining,
    state,
  ))
}

/// `connect (name, host, user, password, db [,port [,socket [,charset]]])`
/// or `connect name` or bare `connect` (reconnect the current connection).
/// Paren form: comma-separated positional segments, empty → None. Bare
/// form: the name only, default params.
fn parse_connect_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, arg_offset) = command_arg(full, span)
  let #(name, params) = case
    string.starts_with(arg, "(") && string.ends_with(arg, ")")
  {
    True -> {
      // Segments are relative to the inner text, which starts one byte
      // after the opening paren.
      let inner = arg |> string.remove_prefix("(") |> string.remove_suffix(")")
      let base = arg_offset + 1
      let segments = comma_segments(inner)
      #(
        opt_itext(nth(segments, 0), base),
        ConnectParams(
          host: opt_itext(nth(segments, 1), base),
          user: opt_itext(nth(segments, 2), base),
          password: opt_itext(nth(segments, 3), base),
          database: opt_itext(nth(segments, 4), base),
          port: opt_itext(nth(segments, 5), base),
          socket: opt_itext(nth(segments, 6), base),
          default_charset: opt_itext(nth(segments, 7), base),
        ),
      )
    }
    False ->
      case arg {
        "" -> #(option.None, default_connect_params())
        _ -> #(
          option.Some(parse_interpolated_text_with_offset(arg, arg_offset)),
          default_connect_params(),
        )
      }
  }
  Ok(#(Connect(span: span, name: name, params: params), remaining, state))
}

fn default_connect_params() -> ConnectParams {
  ConnectParams(
    host: option.None,
    user: option.None,
    password: option.None,
    database: option.None,
    port: option.None,
    socket: option.None,
    default_charset: option.None,
  )
}

/// `connection name` — the rest of the unit.
fn parse_connection_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, arg_offset) = command_arg(full, span)
  Ok(#(
    Connection(
      span: span,
      name: parse_interpolated_text_with_offset(arg, arg_offset),
    ),
    remaining,
    state,
  ))
}

/// `copy_file from to [retry]` / `move_file from to [retry]` — two filename
/// tokens, then an optional retry-count token (routed by `kind`).
fn parse_copy_file_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
  kind: CmdKind,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, off) = command_arg(full, span)
  let tokens = ws_tokens(arg)
  let at = fn(i: Int) {
    case nth(tokens, i) {
      Ok(#(tok, start)) -> parse_interpolated_text_with_offset(tok, off + start)
      Error(Nil) -> parse_interpolated_text("")
    }
  }
  let retry = case nth(tokens, 2) {
    Ok(#(tok, _)) -> option.Some(tok)
    Error(Nil) -> option.None
  }
  let stmt = case kind {
    KMoveFile -> MoveFile(span: span, source: at(0), dest: at(1), retry: retry)
    _ -> CopyFile(span: span, source: at(0), dest: at(1), retry: retry)
  }
  Ok(#(stmt, remaining, state))
}

/// `copy_files_wildcard src_dir dst_dir pattern [retry]` — two directory
/// tokens, a pattern token, then an optional retry-count token. (The Rust
/// version dropped the pattern — token[2] landed in `retry` — fixed here.)
fn parse_copy_files_wildcard_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, arg_offset) = command_arg(full, span)
  let tokens = ws_tokens(arg)
  let at = fn(i: Int) {
    case nth(tokens, i) {
      Ok(#(tok, start)) ->
        parse_interpolated_text_with_offset(tok, arg_offset + start)
      Error(Nil) -> parse_interpolated_text("")
    }
  }
  Ok(#(
    CopyFilesWildcard(
      span: span,
      source_dir: at(0),
      dest_dir: at(1),
      pattern: at(2),
      retry: case nth(tokens, 3) {
        Ok(#(tok, _)) -> option.Some(tok)
        Error(Nil) -> option.None
      },
    ),
    remaining,
    state,
  ))
}

/// Span of a variable-name token on the statement's first line: `off` is the
/// token's absolute byte offset, `len` its byte length INCLUDING a leading
/// `$` (matching TextPart.Variable). The token always sits on the routing
/// unit's first line, so the line/column are real.
fn variable_token_span(base: Span, off: Int, len: Int) -> Span {
  Span(
    line: base.line,
    column: base.column + { off - base.offset },
    offset: off,
    len: len,
  )
}

/// `dec $var` / `inc $var` — a variable name (leading `$` stripped); the span
/// covers the `$name` token (register variables `$0`-`$9` included; the name
/// truncates at the first_word charset, so `--inc $i;` stops at `;`).
fn parse_inc_dec_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
  kind: CmdKind,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let off = arg_offset(span, full, arg)
  let variable =
    arg
    |> string.remove_prefix("$")
    |> first_word
  let dollar = case string.starts_with(arg, "$") {
    True -> 1
    False -> 0
  }
  let variable_span =
    variable_token_span(span, off, utf16_size(variable) + dollar)
  case variable {
    "" ->
      Error(error.Syntax(
        message: case kind {
          KInc -> "inc"
          _ -> "dec"
        }
          <> ": variable name must not be empty",
        span: span,
      ))
    _ -> {
      let stmt = case kind {
        KInc ->
          Inc(span: span, variable: variable, variable_span: variable_span)
        _ -> Dec(span: span, variable: variable, variable_span: variable_span)
      }
      Ok(#(stmt, remaining, state))
    }
  }
}

/// `delimiter str` — the ONLY command that changes parser state. The unit
/// was read under the OLD delimiter (bare `delimiter /;` terminates at the
/// `;`); the new one applies from the next statement on.
fn parse_delimiter_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let new_delimiter = full |> drop_word |> string.trim_start
  case new_delimiter {
    "" ->
      Error(error.Syntax(message: "delimiter requires an argument", span: span))
    _ ->
      Ok(#(
        Delimiter(span: span, new_delimiter: new_delimiter),
        remaining,
        ParserState(..state, delimiter: new_delimiter),
      ))
  }
}

/// `die [message]` — optional message, subject to variable substitution.
fn parse_die_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  // mysqltest.cc prints the message verbatim (`die("%s", first_argument)`)
  // — no $variable evaluation.
  let message = case arg {
    "" -> option.None
    _ -> option.Some(arg)
  }
  Ok(#(Die(span: span, message: message), remaining, state))
}

// ── argument helpers ────────────────────────────────────────────────────────

/// Read block content lines until a line whose trimmed text equals `marker`.
fn read_block(
  lines: List(Line),
  marker: String,
) -> Result(#(List(Line), Line, List(Line)), Nil) {
  read_block_acc(lines, marker, [])
}

fn read_block_acc(
  lines: List(Line),
  marker: String,
  acc: List(Line),
) -> Result(#(List(Line), Line, List(Line)), Nil) {
  case lines {
    [] -> Error(Nil)
    [line, ..rest] ->
      case string.trim(line.text) == marker {
        True -> Ok(#(list.reverse(acc), line, rest))
        False -> read_block_acc(rest, marker, [line, ..acc])
      }
  }
}

// ── delimiter-mode unit reading ─────────────────────────────────────────────

/// How a delimiter-terminated read actually ended.
type Terminator {
  Eof
  Delimited(before: String)
}

fn accumulate(
  seen: List(Line),
  lines: List(Line),
  delimiter: String,
) -> #(List(Line), Terminator, List(Line)) {
  accumulate_quote(seen, lines, string.to_graphemes(delimiter), NoQuote)
}

fn accumulate_quote(
  seen: List(Line),
  lines: List(Line),
  delimiter: List(String),
  quote: Quote,
) -> #(List(Line), Terminator, List(Line)) {
  case lines {
    [] -> #(seen, Eof, [])
    [line, ..rest] -> {
      let text = string.trim_end(line.text)
      let #(found, quote) =
        scan_line(string.to_graphemes(text), delimiter, quote, 0, "")
      case found {
        Ok(before) -> {
          // Whatever follows the delimiter continues as the next reading
          // unit (` }` closes a block, ` # cmt` is a comment, ` SELECT…`
          // the next statement) — mysqltest.cc resumes mid-line there.
          let consumed = before + gs_len(delimiter)
          let residue = string.drop_start(text, consumed)
          let remaining = case string.trim(residue) {
            "" -> rest
            _ -> [
              Line(
                text: residue,
                offset: line.offset + consumed,
                number: line.number,
              ),
              ..rest
            ]
          }
          #([line, ..seen], Delimited(prefix_len(text, before)), remaining)
        }
        Error(Nil) -> accumulate_quote([line, ..seen], rest, delimiter, quote)
      }
    }
  }
}

/// Quote context while scanning for a delimiter.
type Quote {
  NoQuote
  Quoted(q: String)
}

/// Scan a line for the terminating delimiter outside quotes. Returns the
/// byte length of the content before the delimiter (Ok) or — when the
/// line does not terminate — the quote state at end of line, carried into
/// the next line. A delimiter match terminates unconditionally: mysqltest.cc
/// match_delimiter is a pure character match with no lookahead (verified
/// identical in 5.7, 26.7 and MariaDB); whatever follows the delimiter
/// becomes the next reading unit.
///
/// Mirrors mysqltest.cc's read_line state machine: inside `'…'`/`"…"`/
/// `` `…` `` a backslash escapes the next character (all quote kinds,
/// backticks included); outside quotes a backslash also prevents the NEXT
/// character from opening a quote (`\'` stays literal) — the delimiter
/// itself is never backslash-escaped.
fn scan_line(
  gs: List(String),
  delimiter: List(String),
  quote: Quote,
  offset: Int,
  prev: String,
) -> #(Result(Int, Nil), Quote) {
  case quote, gs {
    Quoted(q), ["\\", next, ..rest] ->
      scan_line(rest, delimiter, Quoted(q), offset + 1 + utf16_size(next), next)
    Quoted(q), [g, ..rest] ->
      case g == q {
        True -> scan_line(rest, delimiter, NoQuote, offset + utf16_size(g), g)
        False ->
          scan_line(rest, delimiter, Quoted(q), offset + utf16_size(g), g)
      }
    NoQuote, [g, ..rest] ->
      case is_quote(g) && prev != "\\" {
        True -> scan_line(rest, delimiter, Quoted(g), offset + utf16_size(g), g)
        False ->
          case matches_delim(gs, delimiter) {
            True -> #(Ok(offset), NoQuote)
            False ->
              scan_line(rest, delimiter, NoQuote, offset + utf16_size(g), g)
          }
      }
    _, [] -> #(Error(Nil), quote)
  }
}

fn is_quote(g: String) -> Bool {
  g == "'" || g == "\"" || g == "`"
}

/// Do the graphemes starting at `gs` equal the delimiter?
fn matches_delim(gs: List(String), delimiter: List(String)) -> Bool {
  case delimiter {
    [] -> True
    [d, ..ds] ->
      case gs {
        [g, ..rest] -> g == d && matches_delim(rest, ds)
        [] -> False
      }
  }
}

/// Statement text: the accumulated lines joined with `\n`, with the
/// terminator line contributing only its content before the delimiter.
fn join_body(ordered: List(Line), termination: Terminator) -> String {
  case termination {
    Eof -> string.join(list.map(ordered, fn(line) { line.text }), "\n")
    Delimited(before) ->
      string.join(
        list.append(list.map(all_but_last(ordered), fn(line) { line.text }), [
          before,
        ]),
        "\n",
      )
  }
}

/// Span from the first line's first non-whitespace byte to the end of the
/// statement text.
fn body_span(ordered: List(Line), termination: Terminator) -> Span {
  let assert Ok(first) = list.first(ordered)
  let assert Ok(last) = list.last(ordered)
  let start = line_span(first)
  let end = case termination {
    Eof -> last.offset + utf16_size(string.trim_end(last.text))
    Delimited(before) -> last.offset + utf16_size(before)
  }
  Span(
    line: start.line,
    column: start.column,
    offset: start.offset,
    len: end - start.offset,
  )
}

// ── span & text helpers ─────────────────────────────────────────────────────

/// `diff_files file_name1 file_name2` — two filename tokens.
fn parse_diff_files_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, off) = command_arg(full, span)
  let tokens = ws_tokens(arg)
  let at = fn(i: Int) {
    case nth(tokens, i) {
      Ok(#(tok, start)) -> parse_interpolated_text_with_offset(tok, off + start)
      Error(Nil) -> parse_interpolated_text("")
    }
  }
  Ok(#(DiffFiles(span: span, file1: at(0), file2: at(1)), remaining, state))
}

/// `dirty_close connection_name` — the rest of the unit (a variable may
/// specify the name).
fn parse_dirty_close_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let off = arg_offset(span, full, arg)
  Ok(#(
    DirtyClose(span: span, name: parse_interpolated_text_with_offset(arg, off)),
    remaining,
    state,
  ))
}

/// `disable_testcase BUG#NNNN` — the bug number is mandatory (format not
/// validated here).
fn parse_disable_testcase_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let bug = full |> drop_word |> string.trim_start
  case bug {
    "" ->
      Error(error.Syntax(
        message: "disable_testcase requires a bug number (BUG#NNNN)",
        span: span,
      ))
    _ -> Ok(#(DisableTestcase(span: span, bug: bug), remaining, state))
  }
}

/// `enable_testcase` — no arguments.
fn parse_enable_testcase_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(_full, span, remaining, state) = read_unit(line, rest, state)
  Ok(#(EnableTestcase(span: span), remaining, state))
}

/// `enable_X` / `disable_X` — argument-less boolean-setting toggle.
fn parse_toggle_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
  kind: ast.ToggleKind,
  enable: Bool,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(_full, span, remaining, state) = read_unit(line, rest, state)
  Ok(#(Toggle(span: span, kind: kind, enable: enable), remaining, state))
}

/// `disconnect connection_name` — the rest of the unit.
fn parse_disconnect_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let off = arg_offset(span, full, arg)
  Ok(#(
    Disconnect(span: span, name: parse_interpolated_text_with_offset(arg, off)),
    remaining,
    state,
  ))
}

/// Argument-less commands routed by `kind`.
fn parse_no_args_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
  kind: CmdKind,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(_full, span, remaining, state) = read_unit(line, rest, state)
  let stmt = case kind {
    KEndTimer -> EndTimer(span: span)
    KExit -> Exit(span: span)
    KHorizontalResults -> HorizontalResults(span: span)
    KLowercaseResults -> LowercaseResults(span: span)
    KPing -> Ping(span: span)
    KReap -> Reap(span: span)
    KSaveMasterPos -> SaveMasterPos(span: span)
    KSendShutdown -> SendShutdown(span: span)
    KSortedResult -> SortedResult(span: span)
    KStartTimer -> StartTimer(span: span)
    KVerticalResults -> VerticalResults(span: span)
    KResetConnection -> ResetConnection(span: span)
    KOptimizerTrace -> OptimizerTrace(span: span)
    KPsExecute -> PsExecute(span: span)
    KPsClose -> PsClose(span: span)
    KSkipIfHypergraph -> SkipIfHypergraph(span: span)
    _ -> WaitForSlaveToStop(span: span)
  }
  Ok(#(stmt, remaining, state))
}

/// `error code[, code]...` — comma-separated, trimmed, empty segments
/// dropped; numbers, S-SQLSTATE, or symbolic names, all as text.
/// The argument region of a routing unit: drop the command name, trim
/// leading whitespace, and cut a trailing `#` comment. mysqltest.cc
/// tolerates a `#` comment after a complete table-driven argument list
/// (check_command_args stops its extra-argument scan at `#`;
/// check_eol_junk_line and 26.7's do_error allow one after the value). The
/// returned offset is the argument's absolute byte position — valid because
/// the cut only truncates the tail.
fn command_arg(full: String, span: Span) -> #(String, Int) {
  let raw = full |> drop_word |> string.trim_start
  #(cut_inline_comment(raw), arg_offset(span, full, raw))
}

/// Cut at the first `#` that begins a token (at the start or after
/// whitespace) and is not inside quotes. A quote only opens at a token
/// start (as in mysqltest.cc ARG_STRING), and a mid-token `#` — as in a
/// filename `a#b` — is a literal character.
fn cut_inline_comment(s: String) -> String {
  cut_comment_scan(string.to_graphemes(s), True, option.None, [])
}

fn cut_comment_scan(
  gs: List(String),
  token_start: Bool,
  quote: option.Option(String),
  acc: List(String),
) -> String {
  case gs {
    [] -> string.concat(list.reverse(acc))
    ["#", ..rest] ->
      case quote, token_start {
        option.None, True -> string.trim_end(string.concat(list.reverse(acc)))
        _, _ -> cut_comment_scan(rest, False, quote, ["#", ..acc])
      }
    [g, ..rest] ->
      case quote {
        option.Some(q) ->
          cut_comment_scan(
            rest,
            False,
            case g == q {
              True -> option.None
              False -> quote
            },
            [g, ..acc],
          )
        option.None ->
          case token_start {
            True ->
              case g == "'" || g == "\"" || g == "`" {
                True ->
                  cut_comment_scan(rest, False, option.Some(g), [g, ..acc])
                False -> cut_comment_scan(rest, False, option.None, [g, ..acc])
              }
            False ->
              cut_comment_scan(rest, g == " " || g == "\t", option.None, [
                g,
                ..acc
              ])
          }
      }
  }
}

fn parse_error_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, off) = command_arg(full, span)
  let codes =
    comma_segments(arg)
    |> list.filter_map(fn(seg) {
      let #(t, start) = seg
      case t {
        "" -> Error(Nil)
        _ -> Ok(parse_interpolated_text_with_offset(t, off + start))
      }
    })
  Ok(#(ast.Error(span: span, error_codes: codes), remaining, state))
}

/// Rest-of-unit interpolated-text commands routed by `kind`.
fn parse_itext_arg_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
  kind: CmdKind,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let off = arg_offset(span, full, arg)
  let text = parse_interpolated_text_with_offset(arg, off)
  let stmt = case kind {
    KEval -> Eval(span: span, statement: text)
    KExec -> Exec(span: span, command: text)
    KExecInBackground -> ExecInBackground(span: span, command: text)
    KQuery -> QueryCommand(span: span, sql: text)
    KQueryHorizontal -> QueryHorizontal(span: span, sql: text)
    KQueryVertical -> QueryVertical(span: span, sql: text)
    KSendQuit -> SendQuit(span: span, connection: text)
    KSystem -> System(span: span, command: text)
    KEvalp -> Evalp(span: span, statement: text)
    KPsPrepare -> PsPrepare(span: span, sql: text)
    _ -> Execw(span: span, command: text)
  }
  Ok(#(stmt, remaining, state))
}

/// `expr $var= expression` — the expression is kept raw (operands and
/// operators are a runtime concern).
fn parse_expr_cmd_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let off = arg_offset(span, full, arg)
  case string.split_once(arg, "=") {
    Ok(#(lhs, rhs)) -> {
      let trimmed = string.trim(lhs)
      let lead = utf16_size(lhs) - utf16_size(string.trim_start(lhs))
      let variable = string.remove_prefix(trimmed, "$")
      let variable_span =
        variable_token_span(span, off + lead, utf16_size(trimmed))
      case variable {
        "" ->
          Error(error.Syntax(
            message: "expr: variable name must not be empty",
            span: span,
          ))
        _ ->
          Ok(#(
            Expr(
              span: span,
              variable: variable,
              variable_span: variable_span,
              expression: string.trim_start(rhs),
            ),
            remaining,
            state,
          ))
      }
    }
    Error(Nil) ->
      Error(error.Syntax(
        message: "expr: expected '$var= expression'",
        span: span,
      ))
  }
}

/// `file_exists file [retry]` / `remove_file file [retry]`
fn parse_file_exists_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
  kind: CmdKind,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, off) = command_arg(full, span)
  let tokens = ws_tokens(arg)
  let file = case nth(tokens, 0) {
    Ok(#(tok, start)) -> parse_interpolated_text_with_offset(tok, off + start)
    Error(Nil) -> parse_interpolated_text("")
  }
  let retry = case nth(tokens, 1) {
    Ok(#(tok, _)) -> option.Some(tok)
    Error(Nil) -> option.None
  }
  let stmt = case kind {
    KRemoveFile -> RemoveFile(span: span, file: file, retry: retry)
    _ -> FileExists(span: span, file: file, retry: retry)
  }
  Ok(#(stmt, remaining, state))
}

/// Two-token commands routed by `kind`.
fn parse_two_token_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
  kind: CmdKind,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, off) = command_arg(full, span)
  let tokens = ws_tokens(arg)
  let at = fn(i: Int) {
    case nth(tokens, i) {
      Ok(#(tok, start)) -> parse_interpolated_text_with_offset(tok, off + start)
      Error(Nil) -> parse_interpolated_text("")
    }
  }
  let stmt = case kind {
    KForceCpdir -> ForceCpdir(span: span, source_dir: at(0), dest_dir: at(1))
    _ -> ForceCpdir(span: span, source_dir: at(0), dest_dir: at(1))
  }
  Ok(#(stmt, remaining, state))
}

/// `force-rmdir dir_name`
fn parse_one_token_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
  kind: CmdKind,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, off) = command_arg(full, span)
  let dir = case ws_tokens(arg) {
    [#(tok, start), ..] -> parse_interpolated_text_with_offset(tok, off + start)
    [] -> parse_interpolated_text("")
  }
  let stmt = case kind {
    KMkdir -> Mkdir(span: span, dir: dir)
    KOutput -> Output(span: span, file: dir)
    KForceRmdir -> ForceRmdir(span: span, dir: dir)
    _ -> Rmdir(span: span, dir: dir)
  }
  Ok(#(stmt, remaining, state))
}

/// `let $var= value` / `let ENV_VAR= value` — the variable name may not
/// contain whitespace or `=`; without `$` it names an environment variable.
/// The value is one of: plain text (variables substituted), a backtick
/// query, `query_get_value(...)`, `escape(...)`, or `convert_error(...)`.
/// In bare form the value spans lines up to the delimiter.
fn parse_let_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let off = arg_offset(span, full, arg)
  case string.split_once(arg, "=") {
    Error(Nil) ->
      Error(error.Syntax(message: "let: expected '$var= value'", span: span))
    Ok(#(lhs, rhs)) -> {
      // The token is the trimmed lhs (`$name`, or a bare environment-variable
      // name); the reported variable is it minus an optional `$` — prefix
      // arithmetic, so the span covers exactly the token.
      let trimmed = string.trim(lhs)
      let lead = utf16_size(lhs) - utf16_size(string.trim_start(lhs))
      let variable = string.remove_prefix(trimmed, "$")
      let variable_span =
        variable_token_span(span, off + lead, utf16_size(trimmed))
      case variable {
        "" ->
          Error(error.Syntax(
            message: "let: variable name must not be empty",
            span: span,
          ))
        _ -> {
          let value = string.trim_start(rhs)
          let value_off = off + utf16_size(arg) - utf16_size(value)
          case parse_let_value(value, value_off) {
            Ok(v) ->
              Ok(#(
                Let(
                  span: span,
                  variable: variable,
                  variable_span: variable_span,
                  value: v,
                ),
                remaining,
                state,
              ))
            Error(Nil) ->
              Error(error.Syntax(
                message: "let: invalid value '" <> value <> "'",
                span: span,
              ))
          }
        }
      }
    }
  }
}

/// Dispatch a let value by its leading form.
fn parse_let_value(value: String, off: Int) -> Result(LetValue, Nil) {
  let len = utf16_size(value)
  let whole = Span(line: 1, column: 1, offset: off, len: len)
  case
    function_body(value, "query_get_value("),
    string.starts_with(value, "escape("),
    function_body(value, "convert_error("),
    string.starts_with(value, "`")
  {
    Ok(inner), _, _, _ -> {
      // Args are relative to the text after `query_get_value(`.
      let base = off + 17
      let args =
        comma_segments(inner)
        |> list.filter_map(fn(seg) {
          let #(t, start) = seg
          case t {
            "" -> Error(Nil)
            _ ->
              Ok(parse_interpolated_text_with_offset(unquote(t), base + start))
          }
        })
      // mysqltest.cc requires three args (query, column, row); fewer occur
      // in real suite files inside branches that never execute — accept any
      // non-empty count.
      case args {
        [] -> Error(Nil)
        _ -> Ok(QueryGetValue(span: whole, args: args))
      }
    }
    _, True, _, _ -> escape_value(value, off, whole)
    _, _, Ok(inner), _ -> {
      let leading = utf16_size(inner) - utf16_size(string.trim_start(inner))
      // An empty argument is legal — it evaluates to 0 ("If the argument
      // is 0 or an empty string, it returns 0").
      let argument = string.trim(inner)
      let argument_off = case argument {
        "" -> off + 14
        _ -> off + 14 + leading
      }
      Ok(ConvertError(
        span: whole,
        argument: parse_interpolated_text_with_offset(argument, argument_off),
      ))
    }
    _, _, _, True -> {
      // A closed backtick pair, or — leniently — an unclosed one: real
      // suite files contain `let $v= \`query` with no closing backtick
      // inside branches that never execute (mysqltest only validates at
      // run time), so the rest of the value is taken as the query.
      let inner = case string.ends_with(value, "`") && len > 1 {
        True -> value |> string.remove_prefix("`") |> string.remove_suffix("`")
        False -> string.drop_start(value, 1)
      }
      // do_eval runs on the content — $var references carry spans; the
      // inner text starts after the opening backtick (off + 1).
      Ok(
        QueryValue(query: QueryExpr(
          span: whole,
          query: parse_interpolated_text_with_offset(inner, off + 1),
        )),
      )
    }
    _, _, _, _ ->
      Ok(Text(value: parse_interpolated_text_with_offset(value, off)))
  }
}

/// `escape(CHARACTERS,TEXT)` — mysqltest.cc var_set_escape parses
/// `^\([^\n\r][^,\n\r]*,.*\)$`: CHARACTERS is one character (which may
/// itself be a comma) followed by comma-free characters, then `,`, then
/// TEXT runs to the very end of the value whose FINAL character must be
/// the closing paren — so `escape(), hihi (haha))` has CHARACTERS `)` and
/// TEXT ` hihi (haha)` (a `)` in first position is an argument, not the
/// close).
fn escape_value(value: String, off: Int, whole: Span) -> Result(LetValue, Nil) {
  let body = string.drop_start(value, 7)
  case string.ends_with(value, ")") {
    False -> Error(Nil)
    True ->
      case string.to_graphemes(body) {
        // CHARACTERS: one char unconditionally, then until the first comma.
        [first, ..rest] -> {
          let #(chars_tail, after_chars) =
            take_while_list(rest, fn(g) { g != "," })
          case after_chars {
            [",", ..] -> {
              let chars = string.concat([first, ..chars_tail])
              let text =
                string.drop_end(
                  string.drop_start(body, utf16_size(chars) + 1),
                  1,
                )
              let chars_off = off + 7
              let str_off = off + 7 + utf16_size(chars) + 1
              Ok(Escape(
                span: whole,
                characters: parse_interpolated_text_with_offset(
                  chars,
                  chars_off,
                ),
                string: parse_interpolated_text_with_offset(text, str_off),
              ))
            }
            _ -> Error(Nil)
          }
        }
        [] -> Error(Nil)
      }
  }
}

/// The text between `prefix(` and the FIRST `)` after it — anything past
/// that paren is ignored (mysqltest.cc reads these forms with strchr(')'),
/// so trailing text, including a `#` comment, is tolerated).
fn function_body(value: String, prefix: String) -> Result(String, Nil) {
  case string.starts_with(value, prefix) {
    False -> Error(Nil)
    True ->
      case
        string.split_once(string.drop_start(value, utf16_size(prefix)), ")")
      {
        Ok(#(inner, _)) -> Ok(inner)
        Error(Nil) -> Error(Nil)
      }
  }
}

/// Strip one layer of matching outer quotes, if present.
fn unquote(s: String) -> String {
  case string.starts_with(s, "\"") && string.ends_with(s, "\"") {
    True -> s |> string.remove_prefix("\"") |> string.remove_suffix("\"")
    False ->
      case string.starts_with(s, "'") && string.ends_with(s, "'") {
        True -> s |> string.remove_prefix("'") |> string.remove_suffix("'")
        False -> s
      }
  }
}

/// `list_files dir [pattern]`
fn parse_list_files_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, off) = command_arg(full, span)
  let tokens = ws_tokens(arg)
  let opt_at = fn(i: Int) {
    case nth(tokens, i) {
      Ok(#(tok, start)) ->
        option.Some(parse_interpolated_text_with_offset(tok, off + start))
      Error(Nil) -> option.None
    }
  }
  Ok(#(
    ListFiles(span: span, dir: opt_at(0), pattern: opt_at(1)),
    remaining,
    state,
  ))
}

/// `list_files_append_file file dir [pattern]` /
/// `list_files_write_file file dir [pattern]`
fn parse_list_files_file_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
  kind: CmdKind,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, off) = command_arg(full, span)
  let tokens = ws_tokens(arg)
  let at = fn(i: Int) {
    case nth(tokens, i) {
      Ok(#(tok, start)) -> parse_interpolated_text_with_offset(tok, off + start)
      Error(Nil) -> parse_interpolated_text("")
    }
  }
  let pattern = case nth(tokens, 2) {
    Ok(#(tok, start)) ->
      option.Some(parse_interpolated_text_with_offset(tok, off + start))
    Error(Nil) -> option.None
  }
  let stmt = case kind {
    KListFilesAppendFile ->
      ListFilesAppendFile(span: span, file: at(0), dir: at(1), pattern: pattern)
    _ ->
      ListFilesWriteFile(span: span, file: at(0), dir: at(1), pattern: pattern)
  }
  Ok(#(stmt, remaining, state))
}

/// `perl [terminator]` … code … `terminator` — the code is raw text until
/// the terminator line (default `EOF`); `#` lines inside are code.
fn parse_perl_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let arg_span = sub_span(span, full, utf16_size(full) - utf16_size(arg), 0)
  let marker_name = case arg {
    "" -> "EOF"
    _ -> arg
  }
  let open = case arg {
    "" ->
      Marker(
        name: marker_name,
        span: Span(line: 1, column: 1, offset: arg_span.offset, len: 0),
      )
    _ ->
      Marker(
        name: marker_name,
        span: Span(
          line: arg_span.line,
          column: arg_span.column,
          offset: arg_span.offset,
          len: utf16_size(arg),
        ),
      )
  }
  case read_block(remaining, marker_name) {
    Ok(#(content_lines, marker_line, remaining)) -> {
      let content =
        string.join(
          list.map(content_lines, fn(l) { l.text }),
          "
",
        )
      let leading = leading_ws(marker_line)
      let close =
        Marker(
          name: marker_name,
          span: Span(
            line: marker_line.number,
            column: leading + 1,
            offset: marker_line.offset + leading,
            len: utf16_size(marker_name),
          ),
        )
      let end = marker_line.offset + leading + utf16_size(marker_name)
      Ok(#(
        Perl(
          span: Span(
            line: span.line,
            column: span.column,
            offset: span.offset,
            len: end - span.offset,
          ),
          open: open,
          close: close,
          content: content,
        ),
        remaining,
        state,
      ))
    }
    Error(Nil) ->
      Error(error.UnterminatedBlock(
        command: "perl",
        marker: marker_name,
        span: span,
      ))
  }
}

/// `remove_files_wildcard dir pattern [retry]`
fn parse_remove_files_wildcard_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, off) = command_arg(full, span)
  let tokens = ws_tokens(arg)
  let at = fn(i: Int) {
    case nth(tokens, i) {
      Ok(#(tok, start)) -> parse_interpolated_text_with_offset(tok, off + start)
      Error(Nil) -> parse_interpolated_text("")
    }
  }
  let retry = case nth(tokens, 2) {
    Ok(#(tok, _)) -> option.Some(tok)
    Error(Nil) -> option.None
  }
  Ok(#(
    RemoveFilesWildcard(span: span, dir: at(0), pattern: at(1), retry: retry),
    remaining,
    state,
  ))
}

/// `replace_column col value [col value]...` — alternating pairs; a value
/// may be double-quoted (then it may contain spaces, and variables are NOT
/// expanded); `""` inside quotes is a literal quote.
fn parse_replace_column_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let off = arg_offset(span, full, arg)
  case parse_column_pairs(quoted_ws_tokens(arg), off, []) {
    Ok(pairs) ->
      Ok(#(ReplaceColumn(span: span, pairs: pairs), remaining, state))
    Error(Nil) ->
      Error(error.Syntax(
        message: "replace_column: expected col_num value pairs",
        span: span,
      ))
  }
}

fn parse_column_pairs(
  tokens: List(#(String, Int)),
  off: Int,
  acc: List(#(Int, InterpolatedText)),
) -> Result(List(#(Int, InterpolatedText)), Nil) {
  case tokens {
    [] -> Ok(list.reverse(acc))
    [#(col, _col_start), ..more] ->
      case int.parse(col) {
        Ok(n) ->
          case more {
            [#(value, v_start), ..more2] ->
              parse_column_pairs(
                more2,
                off,
                #(n, parse_interpolated_text_with_offset(value, off + v_start))
                  |> fn(p) { [p, ..acc] },
              )
            [] -> Error(Nil)
          }
        Error(Nil) -> Error(Nil)
      }
  }
}

/// `replace_numeric_round precision` — digits after the decimal point.
fn parse_replace_numeric_round_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start |> cut_inline_comment
  case int.parse(arg) {
    Ok(precision) ->
      Ok(#(
        ReplaceNumericRound(span: span, precision: precision),
        remaining,
        state,
      ))
    Error(Nil) ->
      Error(error.Syntax(
        message: "replace_numeric_round: expected a number",
        span: span,
      ))
  }
}

/// `replace_regex /pattern/replacement/[i] ...` — slash-delimited rules;
/// `\/` escapes a slash inside pattern or replacement.
fn parse_replace_regex_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  // `--replace_regex $var` — the variable holds the entire rule list at run
  // time (mysqltest.cc do_get_replace_regex: "Allow variable for the
  // *entire* list of replacements").
  case string.starts_with(arg, "$") {
    True -> {
      let off = arg_offset(span, full, arg)
      Ok(#(
        ReplaceRegexVariable(
          span: span,
          variable: parse_interpolated_text_with_offset(arg, off),
        ),
        remaining,
        state,
      ))
    }
    False ->
      case arg {
        // Bare `replace_regex` with no argument legally clears the rules
        // (mysqltest.cc: free_replace_regex() then a falsy expr check).
        "" -> Ok(#(ReplaceRegex(span: span, rules: []), remaining, state))
        _ ->
          case parse_regex_rules(string.to_graphemes(arg), []) {
            Ok(rules) ->
              Ok(#(ReplaceRegex(span: span, rules: rules), remaining, state))
            Error(Nil) ->
              Error(error.Syntax(
                message: "replace_regex: expected /pattern/replacement/[i] rules",
                span: span,
              ))
          }
      }
  }
}

fn parse_regex_rules(
  gs: List(String),
  acc: List(RegexRule),
) -> Result(List(RegexRule), Nil) {
  case gs {
    // Non-empty input that produced no rule at all is an error; an empty
    // argument never reaches here (handled as the clear form).
    [] ->
      case acc {
        [] -> Error(Nil)
        _ -> Ok(list.reverse(acc))
      }
    [g, ..more] ->
      case g == "/" {
        True -> {
          let #(pattern, after_p) = take_till_slash(more, [])
          let #(replacement, after_r) = take_till_slash(after_p, [])
          // take_till_slash's sentinel for "no closing slash".
          case pattern == "\u{0}" || replacement == "\u{0}" {
            True -> Error(Nil)
            False ->
              // Only `i` right after the closing '/' sets icase.
              case after_r {
                ["i", ..more2] ->
                  parse_regex_rules(more2, [
                    RegexRule(
                      pattern: pattern,
                      replacement: replacement,
                      insensitive: True,
                    ),
                    ..acc
                  ])
                more2 ->
                  parse_regex_rules(more2, [
                    RegexRule(
                      pattern: pattern,
                      replacement: replacement,
                      insensitive: False,
                    ),
                    ..acc
                  ])
              }
          }
        }
        // Junk between/before/after rules is skipped: mysqltest.cc
        // init_replace_regex scans for the next '/', tolerating the
        // sed-style `s/` prefix, multi-char flags (gso), `,` separators,
        // newlines in multi-line bare commands, trailing text.
        False -> parse_regex_rules(more, acc)
      }
  }
}

/// Graphemes until the closing `/` (with `\/` escapes); also returns what
/// follows. Error when no closing slash — expressed as an empty rest marker
/// checked by the caller via sentinel: we return (#("", [])) only on
/// exhausting input without a close, detected by pattern being nil-safe.
fn take_till_slash(
  gs: List(String),
  acc: List(String),
) -> #(String, List(String)) {
  case gs {
    [] -> #("\u{0}", [])
    ["\\", "/", ..rest] -> take_till_slash_continue(rest, ["/", ..acc])
    ["/", ..rest] -> #(string.concat(list.reverse(acc)), rest)
    [g, ..rest] -> take_till_slash(rest, [g, ..acc])
  }
}

fn take_till_slash_continue(
  gs: List(String),
  acc: List(String),
) -> #(String, List(String)) {
  case gs {
    [] -> #("\u{0}", [])
    ["\\", "/", ..rest] -> take_till_slash(rest, ["/", ..acc])
    ["/", ..rest] -> #(string.concat(list.reverse(acc)), rest)
    [g, ..rest] -> take_till_slash(rest, [g, ..acc])
  }
}

/// `replace_result from to [from to]...` — single/double-quoted or bare
/// values, variables expanded, matched literally.
fn parse_replace_result_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let off = arg_offset(span, full, arg)
  let tokens = quoted_ws_tokens(arg)
  case tokens |> list.length |> fn(n) { n % 2 } {
    0 ->
      case value_pairs(tokens, off, []) {
        Ok(pairs) ->
          Ok(#(ReplaceResult(span: span, pairs: pairs), remaining, state))
        Error(Nil) ->
          Error(error.Syntax(
            message: "replace_result: expected from/to pairs",
            span: span,
          ))
      }
    _ ->
      Error(error.Syntax(
        message: "replace_result: expected from/to pairs",
        span: span,
      ))
  }
}

/// A replace_result word: mysqltest.cc get_string substitutes only when the
/// WHOLE word is `$name` (or a `$0`-`$9` register) — a `$` anywhere else in
/// the word is literal text.
fn whole_word_text(word: String, off: Int) -> InterpolatedText {
  case is_whole_variable(word) {
    True -> [
      Variable(
        name: string.drop_start(word, 1),
        span: Span(
          line: 1,
          column: 1,
          offset: off + 1,
          len: utf16_size(word) - 1,
        ),
      ),
    ]
    False -> [
      Literal(
        text: word,
        span: Span(line: 1, column: 1, offset: off, len: utf16_size(word)),
      ),
    ]
  }
}

fn is_whole_variable(word: String) -> Bool {
  case string.to_graphemes(word) {
    ["$", digit] -> is_digit(digit)
    ["$", first, ..rest] ->
      string.contains(var_start_chars, first)
      && list.all(rest, fn(g) { string.contains(var_cont_chars, g) })
    _ -> False
  }
}

fn value_pairs(
  tokens: List(#(String, Int)),
  off: Int,
  acc: List(#(InterpolatedText, InterpolatedText)),
) -> Result(List(#(InterpolatedText, InterpolatedText)), Nil) {
  case tokens {
    [] -> Ok(list.reverse(acc))
    [#(a, a_start), #(b, b_start), ..more] ->
      value_pairs(more, off, [
        #(whole_word_text(a, off + a_start), whole_word_text(b, off + b_start)),
        ..acc
      ])
    [_] -> Error(Nil)
  }
}

/// `result_format version` — 1 (default) or 2.
fn parse_result_format_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start |> cut_inline_comment
  case int.parse(arg) {
    Ok(version) ->
      Ok(#(ResultFormat(span: span, version: version), remaining, state))
    Error(Nil) ->
      Error(error.Syntax(
        message: "result_format: expected a version number",
        span: span,
      ))
  }
}

/// Whitespace-separated tokens where `"…"` (with `""` as a literal quote)
/// and `'…'` group content including spaces; the quotes are stripped.
/// Each token carries its byte offset within the arg.
fn quoted_ws_tokens(arg: String) -> List(#(String, Int)) {
  quoted_scan(string.to_graphemes(arg), 0, [], [], 0, Plain, False)
}

type QuoteMode {
  Plain
  InDquote
  InSquote
}

fn quoted_scan(
  gs: List(String),
  offset: Int,
  acc: List(#(String, Int)),
  cur: List(String),
  cur_start: Int,
  mode: QuoteMode,
  in_token: Bool,
) -> List(#(String, Int)) {
  case gs {
    [] -> list.reverse(flush_quoted(cur, cur_start, acc, in_token))
    [g, ..rest] ->
      case mode {
        Plain ->
          case g == " " || g == "\t" {
            True ->
              quoted_scan(
                rest,
                offset + utf16_size(g),
                flush_quoted(cur, cur_start, acc, in_token),
                [],
                0,
                Plain,
                False,
              )
            False ->
              case g {
                // A quote opens a token only at its start (mysqltest.cc
                // ARG_STRING); mid-token quotes are literal characters.
                "\"" if cur == [] ->
                  quoted_scan(rest, offset + 1, acc, [], offset, InDquote, True)
                "'" if cur == [] ->
                  quoted_scan(rest, offset + 1, acc, [], offset, InSquote, True)
                _ ->
                  quoted_scan(
                    rest,
                    offset + utf16_size(g),
                    acc,
                    [g, ..cur],
                    case cur {
                      [] -> offset
                      _ -> cur_start
                    },
                    Plain,
                    True,
                  )
              }
          }
        InDquote ->
          case g {
            "\"" ->
              case rest {
                ["\"", ..more] ->
                  quoted_scan(
                    more,
                    offset + 2,
                    acc,
                    ["\"", ..cur],
                    cur_start,
                    InDquote,
                    True,
                  )
                _ ->
                  quoted_scan(
                    rest,
                    offset + 1,
                    acc,
                    cur,
                    cur_start,
                    Plain,
                    True,
                  )
              }
            _ ->
              quoted_scan(
                rest,
                offset + utf16_size(g),
                acc,
                [g, ..cur],
                cur_start,
                InDquote,
                True,
              )
          }
        InSquote ->
          case g {
            "'" ->
              quoted_scan(rest, offset + 1, acc, cur, cur_start, Plain, True)
            _ ->
              quoted_scan(
                rest,
                offset + utf16_size(g),
                acc,
                [g, ..cur],
                cur_start,
                InSquote,
                True,
              )
          }
      }
  }
}

fn flush_quoted(
  cur: List(String),
  cur_start: Int,
  acc: List(#(String, Int)),
  in_token: Bool,
) -> List(#(String, Int)) {
  case cur, in_token {
    // An empty token only exists for an empty quoted string (`''` / `""`).
    [], False -> acc
    _, _ -> [#(string.concat(list.reverse(cur)), cur_start), ..acc]
  }
}

/// `send [statement]` / `send_eval [statement]` — omitted statement ⇒ the
/// command applies to the next executed statement.
fn parse_send_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
  kind: CmdKind,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let off = arg_offset(span, full, arg)
  // send_eval evaluates $variables (do_eval); send passes the statement to
  // the server verbatim — that distinction is why send_eval exists.
  let stmt = case kind {
    KSendEval -> {
      let sql = case arg {
        "" -> option.None
        _ -> option.Some(parse_interpolated_text_with_offset(arg, off))
      }
      SendEval(span: span, sql: sql)
    }
    _ -> {
      let sql = case arg {
        "" -> option.None
        _ -> option.Some(arg)
      }
      Send(span: span, sql: sql)
    }
  }
  Ok(#(stmt, remaining, state))
}

/// `require file_name` — (5.7) a single token; missing argument is an
/// error ("Missing file name argument" in mysqltest.cc).
fn parse_require_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  case take_token(arg) {
    "" ->
      Error(error.Syntax(
        message: "require: missing file name argument",
        span: span,
      ))
    token -> {
      let off = arg_offset(span, full, arg)
      Ok(#(
        Require(
          span: span,
          file: parse_interpolated_text_with_offset(token, off),
        ),
        remaining,
        state,
      ))
    }
  }
}

/// `query_attributes name value [name value]...` — (8.0+) name/value pairs.
fn parse_query_attributes_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let off = arg_offset(span, full, arg)
  let tokens = ws_tokens(arg)
  let pairs =
    tokens
    |> pair_up(off, [])
    |> list.map(fn(p) {
      #(parse_interpolated_text_with_offset(p.0, off + p.1), p.2)
    })
  Ok(#(QueryAttributes(span: span, attributes: pairs), remaining, state))
}

/// Split a token list into (token, offset, itext-of-next-token) pairs.
fn pair_up(
  tokens: List(#(String, Int)),
  off: Int,
  acc: List(#(String, Int, InterpolatedText)),
) -> List(#(String, Int, InterpolatedText)) {
  case tokens {
    [] -> list.reverse(acc)
    [#(name, n_start), #(value, v_start), ..more] ->
      pair_up(
        more,
        off,
        #(
          name,
          n_start,
          parse_interpolated_text_with_offset(value, off + v_start),
        )
          |> fn(p) { [p, ..acc] },
      )
    [_] -> list.reverse(acc)
  }
}

/// `ps_bind value [value]...` — positional parameter values.
fn parse_ps_bind_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let off = arg_offset(span, full, arg)
  let params =
    ws_tokens(arg)
    |> list.map(fn(tok) {
      let #(t, start) = tok
      parse_interpolated_text_with_offset(t, off + start)
    })
  Ok(#(PsBind(span: span, params: params), remaining, state))
}

/// `write_line line file_name` — two tokens; a newline is appended to the
/// line at runtime.
fn parse_write_line_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let off = arg_offset(span, full, arg)
  let tokens = ws_tokens(arg)
  let at = fn(i: Int) {
    case nth(tokens, i) {
      Ok(#(tok, start)) -> parse_interpolated_text_with_offset(tok, off + start)
      Error(Nil) -> parse_interpolated_text("")
    }
  }
  Ok(#(WriteLine(span: span, line: at(0), file: at(1)), remaining, state))
}

/// `shutdown_server [timeout]` — seconds.
/// `shutdown_server [timeout]` — the timeout may be a number or a
/// `$variable` (lenient: no type validation); a trailing `#` comment is cut.
fn parse_shutdown_server_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, off) = command_arg(full, span)
  let timeout = case arg {
    "" -> option.None
    _ -> option.Some(parse_interpolated_text_with_offset(arg, off))
  }
  Ok(#(ShutdownServer(span: span, timeout: timeout), remaining, state))
}

/// `skip [message]`
fn parse_skip_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  // mysqltest.cc prints the message verbatim (abort_not_supported_test) —
  // no $variable evaluation.
  let message = case arg {
    "" -> option.None
    _ -> option.Some(arg)
  }
  Ok(#(Skip(span: span, message: message), remaining, state))
}

/// `sleep num` / `real_sleep num` — fractional seconds (routed by `kind`).
/// The argument may also be a `$variable`; we do no type inference, so the
/// argument type is deliberately not validated (mysqltest.cc would die on a
/// non-digit argument — we stay lenient on purpose). A trailing `#` comment
/// after the value is cut (mysqltest.cc check_eol_junk_line allows one).
fn parse_sleep_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
  kind: CmdKind,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, off) = command_arg(full, span)
  case arg {
    "" ->
      Error(error.Syntax(
        message: "sleep: missing argument (number of seconds or $variable)",
        span: span,
      ))
    _ -> {
      let seconds = parse_interpolated_text_with_offset(arg, off)
      let stmt = case kind {
        KRealSleep -> RealSleep(span: span, seconds: seconds)
        _ -> Sleep(span: span, seconds: seconds)
      }
      Ok(#(stmt, remaining, state))
    }
  }
}

/// `source file_name` — a single token; the span is the token itself.
fn parse_source_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let arg_start = utf16_size(full) - utf16_size(arg)
  let arg_span = sub_span(span, full, arg_start, 0)
  let token = take_token(arg)
  Ok(#(
    Source(
      span: sub_span(arg_span, arg, 0, utf16_size(token)),
      file: parse_interpolated_text_with_offset(token, arg_span.offset),
    ),
    remaining,
    state,
  ))
}

/// `sync_slave_with_master [connection]`
fn parse_sync_slave_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start
  let off = arg_offset(span, full, arg)
  let connection = case arg {
    "" -> option.None
    _ -> option.Some(parse_interpolated_text_with_offset(arg, off))
  }
  Ok(#(
    SyncSlaveWithMaster(span: span, connection: connection),
    remaining,
    state,
  ))
}

/// `sync_with_master [offset]` — the no-argument form is legal (mysqltest.cc
/// initializes offset 0 and parses digits only when present); the offset may
/// also be a `$variable` (lenient: no type validation).
fn parse_sync_with_master_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let #(arg, off) = command_arg(full, span)
  let offset = case arg {
    "" -> option.None
    _ -> option.Some(parse_interpolated_text_with_offset(arg, off))
  }
  Ok(#(SyncWithMaster(span: span, offset: offset), remaining, state))
}

/// `partially_sorted_result start_column`
fn parse_partially_sorted_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  let #(full, span, remaining, state) = read_unit(line, rest, state)
  let arg = full |> drop_word |> string.trim_start |> cut_inline_comment
  case int.parse(arg) {
    Ok(n) ->
      Ok(#(PartiallySortedResult(span: span, start_column: n), remaining, state))
    Error(Nil) ->
      Error(error.Syntax(
        message: "expected a number, got '" <> arg <> "'",
        span: span,
      ))
  }
}

// ── if blocks ───────────────────────────────────────────────────────────────

/// `if (expr) { body }` — the routing head reads raw lines until the first
/// `{` (the open brace may share the `if` line or sit on its own line; in
/// mysqltest.cc the reading engine terminates if/while units at `{`). The
/// body is parsed as nested statements until a block terminator — a `}` at
/// line start or an `end` line — which the if consumes (never a standalone
/// statement; a stray terminator at top level is an error).
fn parse_flow_statement(
  line: Line,
  rest: List(Line),
  state: ParserState,
  kind: CmdKind,
) -> Result(#(Statement, List(Line), ParserState), ParserError) {
  // Reading the head to `{` supersedes the endline mode — consume the flag.
  let state = ParserState(..state, next_endline_as_delimiter: False)
  case read_if_head(line, rest, state.next_endline_as_delimiter) {
    Error(Nil) ->
      Error(error.UnterminatedFlowControl(
        kind: case kind {
          KWhile -> "while"
          _ -> "if"
        },
        span: line_span(line),
      ))
    Ok(#(head_full, head_start, brace, remaining)) -> {
      let condition_arg = head_full |> drop_word |> string.trim_start
      let cond_offset =
        head_start + utf16_size(head_full) - utf16_size(condition_arg)
      // mysqltest.cc reads a bare if/while unit until the delimiter; the
      // head-to-`{` read keeps that delimiter as a trailing artifact
      // (partition_alter_1.inc's `if (`SELECT …)`);` — drop it.
      let condition_arg =
        drop_trailing_delimiter(condition_arg, state.delimiter)
      case parse_expr(condition_arg, cond_offset) {
        Ok(condition) ->
          case parse_block_body(remaining, state, line_span(line), kind, []) {
            Ok(#(body, close_brace, remaining, state)) -> {
              let start = line_span(line)
              let span =
                Span(
                  line: start.line,
                  column: start.column,
                  offset: start.offset,
                  len: close_brace.offset + close_brace.len - start.offset,
                )
              let stmt = case kind {
                KWhile ->
                  While(
                    span: span,
                    condition: condition,
                    body: body,
                    open_brace: brace,
                    close_brace: close_brace,
                  )
                _ ->
                  If(
                    span: span,
                    condition: condition,
                    body: body,
                    open_brace: brace,
                    close_brace: close_brace,
                  )
              }
              Ok(#(stmt, remaining, state))
            }
            Error(e) -> Error(e)
          }
        Error(Nil) ->
          Error(error.InvalidExpression(
            message: case kind {
              KWhile -> "while"
              _ -> "if"
            }
              <> ": invalid expression '"
              <> condition_arg
              <> "'",
            span: line_span(line),
          ))
      }
    }
  }
}

/// Read the if head: raw lines joined until the first `{`. Returns the head
/// text (dash prefix stripped, trimmed), its start offset, the open-brace
/// span, and the remaining lines.
fn read_if_head(
  line: Line,
  rest: List(Line),
  _was_dash: Bool,
) -> Result(#(String, Int, Span, List(Line)), Nil) {
  let base = line_span(line)
  let first =
    string.remove_prefix(string.trim(line.text), "--")
    |> string.trim_start
  let dash_skip = utf16_size(string.trim(line.text)) - utf16_size(first)
  let head_start = base.offset + dash_skip
  read_if_head_acc(first, line, rest, head_start, [first])
}

fn read_if_head_acc(
  text: String,
  line: Line,
  rest: List(Line),
  head_start: Int,
  _acc: List(String),
) -> Result(#(String, Int, Span, List(Line)), Nil) {
  case find_brace(text) {
    Ok(idx) -> {
      let head = string.trim(string.slice(text, 0, idx))
      // `if (…) { body }` on one line: the text after `{` continues as the
      // next reading unit, exactly like mysqltest.cc resuming mid-line.
      let residue = string.drop_start(text, idx + 1)
      let rest = case string.trim(residue) {
        "" -> rest
        _ -> [
          Line(text: residue, offset: head_start + idx + 1, number: line.number),
          ..rest
        ]
      }
      Ok(#(head, head_start, brace_span_at(text, idx, head_start), rest))
    }
    Error(Nil) ->
      case rest {
        [] -> Error(Nil)
        [next, ..more] ->
          read_if_head_acc(
            text <> "\n" <> next.text,
            next,
            more,
            head_start,
            [],
          )
      }
  }
}

/// Byte index of the first `{` in `text`.
fn find_brace(text: String) -> Result(Int, Nil) {
  find_brace_loop(string.to_graphemes(text), 0)
}

fn find_brace_loop(gs: List(String), offset: Int) -> Result(Int, Nil) {
  case gs {
    ["{", ..] -> Ok(offset)
    [g, ..rest] -> find_brace_loop(rest, offset + utf16_size(g))
    [] -> Error(Nil)
  }
}

/// Span of the `{` at `idx` within `text` starting at `head_start` — with
/// real line/column via newline counting.
fn brace_span_at(text: String, idx: Int, head_start: Int) -> Span {
  sub_span(Span(line: 1, column: 1, offset: head_start, len: 0), text, idx, 1)
}

/// Parse nested statements until a block terminator (`}` at line start or
/// an `end` line); the terminator line is consumed and its span returned.
fn parse_block_body(
  lines: List(Line),
  state: ParserState,
  open_span: Span,
  kind: CmdKind,
  acc: List(Statement),
) -> Result(#(List(Statement), Span, List(Line), ParserState), ParserError) {
  case lines {
    [] ->
      Error(error.UnterminatedFlowControl(
        kind: case kind {
          KWhile -> "while"
          _ -> "if"
        },
        span: open_span,
      ))
    [line, ..rest] ->
      case block_terminator(line) {
        option.Some(#(close, residue)) ->
          Ok(#(list.reverse(acc), close, list.append(residue, rest), state))
        option.None ->
          case string.trim(line.text) {
            "" -> parse_block_body(rest, state, open_span, kind, acc)
            _ ->
              case parse_statement(line, rest, state) {
                Ok(#(stmt, remaining, state)) ->
                  parse_block_body(remaining, state, open_span, kind, [
                    stmt,
                    ..acc
                  ])
                Error(e) -> Error(e)
              }
          }
      }
  }
}

/// A block terminator line: `}` or `end` at line start, or the same behind
/// a `--` prefix — mysqltest.cc get_command_type routes any command line
/// whose first character is `}` to Q_END_BLOCK and `end` is a real command,
/// so `--}`, `-- }` and `-- end` all close a block. Returns the terminator
/// token's span.
fn block_terminator(line: Line) -> option.Option(#(Span, List(Line))) {
  let leading = leading_ws(line)
  let trimmed = string.trim(line.text)
  // A `}` closes the block wherever it starts the reading unit — trailing
  // text (`} #eof comment`) continues as the next unit. `end` matches on its
  // first word, as mysqltest.cc's command lookup does.
  case
    string.starts_with(trimmed, "}"),
    first_word(trimmed) == "end",
    string.starts_with(trimmed, "--")
  {
    True, _, _ ->
      option.Some(#(
        terminator_span(line, leading, 0, 1),
        residue_after(line, leading + 1),
      ))
    _, True, _ ->
      option.Some(#(
        terminator_span(line, leading, 0, 3),
        residue_after(line, leading + 3),
      ))
    _, _, True -> {
      let after = string.trim_start(string.drop_start(trimmed, 2))
      let skipped = utf16_size(trimmed) - 2 - utf16_size(after)
      let rel = leading + 2 + skipped
      case string.starts_with(after, "}") {
        True ->
          option.Some(#(
            terminator_span(line, leading, rel, 1),
            residue_after(line, rel + 1),
          ))
        False ->
          case first_word(after) == "end" {
            True ->
              option.Some(#(
                terminator_span(line, leading, rel, 3),
                residue_after(line, rel + 3),
              ))
            False -> option.None
          }
      }
    }
    _, _, False -> option.None
  }
}

/// The rest of `line` from byte `from` on, as a reading unit — empty when
/// only whitespace remains.
fn residue_after(line: Line, from: Int) -> List(Line) {
  let text = string.drop_start(line.text, from)
  case string.trim(text) {
    "" -> []
    _ -> [Line(text: text, offset: line.offset + from, number: line.number)]
  }
}

fn terminator_span(line: Line, leading: Int, rel: Int, len: Int) -> Span {
  Span(
    line: line.number,
    column: leading + rel + 1,
    offset: line.offset + leading + rel,
    len: len,
  )
}

// ── interpolated text ───────────────────────────────────────────────────────

/// Parse with base byte offset 0.
pub fn parse_interpolated_text(text: String) -> InterpolatedText {
  parse_interpolated_text_with_offset(text, 0)
}

/// Parse, adding `base_offset` to every part's span offset.
///
/// Scanning rules:
/// - `\$` → literal `$`
/// - `$` followed by a digit → register variable `$0`-`$9` (mysqltest.cc
///   var_get routes digits to var_reg; a single-character name)
/// - `$` followed by `[a-zA-Z_]` then `[a-zA-Z0-9_]*` → variable reference
///   (mysqltest.cc my_isvar = alnum | '_' — a `-` ENDS the name, so `$i-1`
///   is `$i` followed by the literal `-1`)
/// - `$` followed by anything else (or end of text) → literal `$`
/// - everything else accumulates into literals
pub fn parse_interpolated_text_with_offset(
  text: String,
  base_offset: Int,
) -> InterpolatedText {
  scan_text(string.to_graphemes(text), 0, base_offset, "", 0, [])
}

const var_start_chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_"

const var_cont_chars =
  "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"

fn scan_text(
  graphemes: List(String),
  offset: Int,
  base: Int,
  literal: String,
  literal_start: Int,
  acc: List(TextPart),
) -> InterpolatedText {
  case graphemes {
    [] -> list.reverse(flush_into(literal, literal_start, base, acc))
    ["\\", "$", ..rest] -> {
      let #(lit, start) = lit_push(literal, literal_start, offset, "$")
      scan_text(rest, offset + 2, base, lit, start, acc)
    }
    ["\\", ..rest] -> {
      let #(lit, start) = lit_push(literal, literal_start, offset, "\\")
      scan_text(rest, offset + 1, base, lit, start, acc)
    }
    ["$", ..more] ->
      case more {
        [first, ..] ->
          case string.contains(var_start_chars, first) {
            True -> {
              let #(name, rest, end) = take_while_var(more, offset + 1, "")
              let acc2 = [
                Variable(
                  name: name,
                  span: Span(
                    line: 1,
                    column: 1,
                    offset: base + offset,
                    len: utf16_size(name) + 1,
                  ),
                ),
                ..flush_into(literal, literal_start, base, acc)
              ]
              scan_text(rest, end, base, "", 0, acc2)
            }
            False ->
              // Register variable: `$0`-`$9` — a single-digit name
              // (mysqltest.cc var_get: `v = var_reg + digit`).
              case is_digit(first) {
                True -> {
                  let acc2 = [
                    Variable(
                      name: first,
                      span: Span(
                        line: 1,
                        column: 1,
                        offset: base + offset,
                        len: 2,
                      ),
                    ),
                    ..flush_into(literal, literal_start, base, acc)
                  ]
                  scan_text(list.drop(more, 1), offset + 2, base, "", 0, acc2)
                }
                False -> {
                  let #(lit, start) =
                    lit_push(literal, literal_start, offset, "$")
                  scan_text(more, offset + 1, base, lit, start, acc)
                }
              }
          }
        [] -> {
          let #(lit, start) = lit_push(literal, literal_start, offset, "$")
          scan_text(more, offset + 1, base, lit, start, acc)
        }
      }
    [g, ..rest] -> {
      let #(lit, start) = lit_push(literal, literal_start, offset, g)
      scan_text(rest, offset + utf16_size(g), base, lit, start, acc)
    }
  }
}

/// Append one grapheme to the literal, remembering where it started.
fn lit_push(
  literal: String,
  start: Int,
  offset: Int,
  g: String,
) -> #(String, Int) {
  case literal {
    "" -> #(literal <> g, offset)
    _ -> #(literal <> g, start)
  }
}

/// Push the accumulated literal onto `acc` (kept in reverse order).
fn flush_into(
  literal: String,
  start: Int,
  base: Int,
  acc: List(TextPart),
) -> List(TextPart) {
  case literal {
    "" -> acc
    _ -> [
      Literal(
        text: literal,
        span: Span(
          line: 1,
          column: 1,
          offset: base + start,
          len: utf16_size(literal),
        ),
      ),
      ..acc
    ]
  }
}

fn take_while_var(
  graphemes: List(String),
  offset: Int,
  name: String,
) -> #(String, List(String), Int) {
  case graphemes {
    [g, ..rest] ->
      case string.contains(var_cont_chars, g) {
        True -> take_while_var(rest, offset + utf16_size(g), name <> g)
        False -> #(name, graphemes, offset)
      }
    [] -> #(name, graphemes, offset)
  }
}

/// A non-empty segment as interpolated text at `base + segment offset`.
fn opt_itext(
  segment: Result(#(String, Int), Nil),
  base: Int,
) -> option.Option(InterpolatedText) {
  case segment {
    Ok(#(t, start)) ->
      case t {
        "" -> option.None
        _ -> option.Some(parse_interpolated_text_with_offset(t, base + start))
      }
    Error(Nil) -> option.None
  }
}

// ── condition expressions ───────────────────────────────────────────────────

/// Parse a condition expression. `input` is the argument text (e.g. `"(1)"`),
/// `base_offset` the byte offset of its first byte in the source.
pub fn parse_expr(input: String, base_offset: Int) -> Result(Expr, Nil) {
  let graphemes = string.to_graphemes(string.trim(input))
  let #(inner, leading) = trim_start_list(graphemes)
  case inner {
    [] -> Error(Nil)
    [_, ..] -> parse_operand(inner, base_offset + leading)
  }
}

fn parse_negated(gs: List(String), base: Int) -> Result(Expr, Nil) {
  case gs {
    ["!", ..rest] -> {
      let #(rest, ws) = trim_start_list(rest)
      case parse_positive(rest, base + 1 + ws) {
        Ok(expr) -> Ok(Negated(span: Span(1, 1, base, 1), expr: expr))
        Error(Nil) -> Error(Nil)
      }
    }
    _ -> parse_positive(gs, base)
  }
}

fn parse_positive(gs: List(String), base: Int) -> Result(Expr, Nil) {
  case try_closure(gs, base) {
    Ok(expr) -> Ok(expr)
    Error(Nil) ->
      case try_logical(gs, base) {
        Ok(expr) -> Ok(expr)
        Error(Nil) ->
          case parse_comparison(gs, base) {
            Ok(expr) -> Ok(expr)
            Error(Nil) ->
              case try_query(gs, base) {
                Ok(expr) -> Ok(expr)
                Error(Nil) -> try_integer(gs, base)
              }
          }
      }
  }
}

fn try_closure(gs: List(String), base: Int) -> Result(Expr, Nil) {
  case gs {
    ["$", "(", ..rest] ->
      case take_till_list(rest, ")") {
        Ok(#(content, _after)) ->
          Ok(Closure(
            span: Span(
              line: 1,
              column: 1,
              offset: base,
              len: 3 + gs_len(content),
            ),
            expression: string.trim(string.concat(content)),
          ))
        Error(Nil) -> Error(Nil)
      }
    _ -> Error(Nil)
  }
}

/// Top-level `&&`/`||` split: `||` (the lower precedence) is found first,
/// each side re-parsed as a full operand — so variables on both sides carry
/// their own spans.
fn try_logical(gs: List(String), base: Int) -> Result(Expr, Nil) {
  case find_logical_op(gs, "|", "|", 0, False, "", 0) {
    Ok(idx) -> build_logical(gs, idx, base, Or)
    Error(Nil) ->
      case find_logical_op(gs, "&", "&", 0, False, "", 0) {
        Ok(idx) -> build_logical(gs, idx, base, And)
        Error(Nil) -> Error(Nil)
      }
  }
}

/// Split `gs` at the top-level operator starting at grapheme `idx` and parse
/// both sides as operands.
fn build_logical(
  gs: List(String),
  idx: Int,
  base: Int,
  op: LogicalOp,
) -> Result(Expr, Nil) {
  // Trim both ends: the whitespace around the operator would otherwise
  // defeat strip_parens' closing-paren-is-last guard on `($a) && $b`.
  let #(left, left_ws) = trim_start_list(list.take(gs, idx))
  let left = trim_end_list(left)
  let right_at = base + gs_len(list.take(gs, idx + 2))
  let #(right, right_ws) = trim_start_list(list.drop(gs, idx + 2))
  let right = trim_end_list(right)
  case
    parse_operand(left, base + left_ws),
    parse_operand(right, right_at + right_ws)
  {
    Ok(l), Ok(r) ->
      Ok(Logical(
        span: Span(line: 1, column: 1, offset: base, len: gs_len(gs)),
        left: l,
        op: op,
        right: r,
      ))
    _, _ -> Error(Nil)
  }
}

fn trim_end_list(gs: List(String)) -> List(String) {
  let #(reversed, _) = trim_start_list(list.reverse(gs))
  list.reverse(reversed)
}

/// A logical operand: strip any number of wrapping paren pairs, then parse
/// with negation allowed (`!$b` may be the right side of `&&`).
fn parse_operand(gs: List(String), base: Int) -> Result(Expr, Nil) {
  // Re-trim inside each stripped paren: `( $x )` leaves a leading space the
  // sub-parsers would reject (`$`/backtick/digit must be the first grapheme).
  let #(gs, ws) = trim_start_list(gs)
  let gs = trim_end_list(gs)
  case strip_parens(gs) {
    #(_, 0) -> parse_negated(gs, base + ws)
    #(inner, skipped) -> parse_operand(inner, base + ws + skipped)
  }
}

/// Strip one trailing occurrence of the delimiter (plus the whitespace it
/// hid) from a condition — the residue of a bare if/while head.
fn drop_trailing_delimiter(s: String, delimiter: String) -> String {
  case string.ends_with(s, delimiter) && s != delimiter {
    True -> string.drop_end(s, utf16_size(delimiter)) |> string.trim_end
    False -> s
  }
}

/// `$var [op rhs]`, or just `$var` when no operator follows.
fn parse_comparison(gs: List(String), base: Int) -> Result(Expr, Nil) {
  case gs {
    ["$", ..rest] ->
      case take_while_list(rest, is_ident) {
        #([], _) -> Error(Nil)
        #(name, after) -> {
          let var =
            VariableRef(
              span: Span(
                line: 1,
                column: 1,
                offset: base,
                len: 1 + gs_len(name),
              ),
              name: string.concat(name),
            )
          let #(after, ws) = trim_start_list(after)
          case parse_op(after) {
            Ok(#(op, op_len, rest)) -> {
              let op_base = base + 1 + gs_len(name) + ws
              let #(rest, ws2) = trim_start_list(rest)
              let rhs_base = op_base + op_len + ws2
              case parse_rhs(rest, rhs_base) {
                Ok(rhs) ->
                  Ok(Comparison(
                    left: var,
                    operator: op,
                    operator_span: Span(
                      line: 1,
                      column: 1,
                      offset: op_base,
                      len: op_len,
                    ),
                    right: rhs,
                  ))
                Error(Nil) -> Error(Nil)
              }
            }
            // No operator — plain truthy variable reference.
            Error(Nil) -> Ok(Var(variable: var))
          }
        }
      }
    _ -> Error(Nil)
  }
}

fn parse_op(
  gs: List(String),
) -> Result(#(ComparisonOp, Int, List(String)), Nil) {
  case gs {
    ["=", "=", ..rest] -> Ok(#(Eq, 2, rest))
    ["!", "=", ..rest] -> Ok(#(Neq, 2, rest))
    ["<", "=", ..rest] -> Ok(#(Le, 2, rest))
    [">", "=", ..rest] -> Ok(#(Ge, 2, rest))
    ["<", ..rest] -> Ok(#(Lt, 1, rest))
    [">", ..rest] -> Ok(#(Gt, 1, rest))
    _ -> Error(Nil)
  }
}

/// Right-hand side: integer prefix, `$var`, backtick query, quoted string,
/// or bare-string fallback. `$` with an empty name is a hard error; an
/// unterminated backtick/quote falls back to the bare string.
fn parse_rhs(gs: List(String), base: Int) -> Result(ComparisonRhs, Nil) {
  let #(digits, _) = take_while_list(gs, is_digit)
  case digits {
    [_, ..] ->
      case int.parse(string.concat(digits)) {
        Ok(value) ->
          Ok(RhsInteger(
            value: value,
            span: Span(line: 1, column: 1, offset: base, len: gs_len(digits)),
          ))
        Error(Nil) -> Error(Nil)
      }
    [] ->
      case gs {
        ["$", ..rest] ->
          case take_while_list(rest, is_ident) {
            #([], _) -> Error(Nil)
            #(name, _) ->
              Ok(
                RhsVariable(VariableRef(
                  span: Span(
                    line: 1,
                    column: 1,
                    offset: base,
                    len: 1 + gs_len(name),
                  ),
                  name: string.concat(name),
                )),
              )
          }
        ["`", ..rest] ->
          case take_till_list(rest, "`") {
            Ok(#(content, _)) ->
              Ok(
                RhsQuery(QueryExpr(
                  span: Span(
                    line: 1,
                    column: 1,
                    offset: base,
                    len: 2 + gs_len(content),
                  ),
                  query: parse_interpolated_text_with_offset(
                    string.concat(content),
                    base + 1,
                  ),
                )),
              )
            Error(Nil) -> Ok(rhs_bare(gs, base))
          }
        [quote, ..rest] ->
          case quote == "\"" || quote == "'" {
            True ->
              case take_till_list(rest, quote) {
                // Span covers the content only, not the quotes.
                Ok(#(content, _)) ->
                  Ok(RhsString(
                    value: string.concat(content),
                    span: Span(
                      line: 1,
                      column: 1,
                      offset: base + 1,
                      len: gs_len(content),
                    ),
                  ))
                Error(Nil) -> Ok(rhs_bare(gs, base))
              }
            False -> Ok(rhs_bare(gs, base))
          }
        _ -> Ok(rhs_bare(gs, base))
      }
  }
}

fn rhs_bare(gs: List(String), base: Int) -> ComparisonRhs {
  let taken = take_till_any(gs, ["\r", "\n", ";", ")"])
  RhsString(
    value: string.concat(taken),
    span: Span(line: 1, column: 1, offset: base, len: gs_len(taken)),
  )
}

fn try_query(gs: List(String), base: Int) -> Result(Expr, Nil) {
  case gs {
    ["`", ..rest] ->
      case take_till_list(rest, "`") {
        Ok(#(content, _)) ->
          Ok(
            Query(QueryExpr(
              span: Span(
                line: 1,
                column: 1,
                offset: base,
                len: 2 + gs_len(content),
              ),
              // The content is do_eval'd at run time — $var references
              // carry spans; the inner text starts after the opening
              // backtick (base + 1).
              query: parse_interpolated_text_with_offset(
                string.concat(content),
                base + 1,
              ),
            )),
          )
        Error(Nil) -> Error(Nil)
      }
    _ -> Error(Nil)
  }
}

fn try_integer(gs: List(String), base: Int) -> Result(Expr, Nil) {
  let #(digits, _) = take_while_list(gs, is_digit)
  case digits {
    [] -> Error(Nil)
    [_, ..] ->
      case int.parse(string.concat(digits)) {
        Ok(value) ->
          Ok(Integer(
            span: Span(line: 1, column: 1, offset: base, len: gs_len(digits)),
            value: value,
          ))
        Error(Nil) -> Error(Nil)
      }
  }
}

/// Strip one pair of surrounding parens — only when the leading `(` matches
/// the FINAL grapheme: in `($a) && $b` the leading paren does not wrap the
/// whole expression and must not be stripped. Returns the inner graphemes and
/// the bytes skipped before them (0 or 1).
fn strip_parens(gs: List(String)) -> #(List(String), Int) {
  case gs {
    ["(", ..] -> {
      let last = list.length(gs) - 1
      case find_matching_paren(gs) {
        Ok(close) if close == last -> #(
          list.take(list.drop(gs, 1), close - 1),
          1,
        )
        _ -> #(gs, 0)
      }
    }
    _ -> #(gs, 0)
  }
}

type ParenResult {
  ParenResult(found: Result(Int, Nil), open_backtick: Result(Int, Nil))
}

/// Find the grapheme index of the closing paren matching a leading `(`,
/// handling nesting and backtick-quoted spans.
fn find_matching_paren(gs: List(String)) -> Result(Int, Nil) {
  let result = scan_parens(gs, 0, 0, False, 0)
  case result.found, result.open_backtick {
    Ok(i), _ -> Ok(i)
    // Unclosed backtick: retry ignoring it (it was probably a literal `).
    Error(Nil), Ok(open) -> scan_parens_no_backticks(gs, 0, 0, open)
    Error(Nil), Error(Nil) -> Error(Nil)
  }
}

fn scan_parens(
  gs: List(String),
  idx: Int,
  depth: Int,
  backtick: Bool,
  open_at: Int,
) -> ParenResult {
  case gs {
    [] ->
      ParenResult(found: Error(Nil), open_backtick: case backtick {
        True -> Ok(open_at)
        False -> Error(Nil)
      })
    ["`", ..rest] ->
      case backtick {
        True -> scan_parens(rest, idx + 1, depth, False, open_at)
        False -> scan_parens(rest, idx + 1, depth, True, idx)
      }
    [_, ..rest] ->
      case backtick {
        True -> scan_parens(rest, idx + 1, depth, True, open_at)
        False ->
          case gs {
            ["(", ..] -> scan_parens(rest, idx + 1, depth + 1, False, open_at)
            [")", ..] ->
              case depth == 1 {
                True -> ParenResult(found: Ok(idx), open_backtick: Error(Nil))
                False -> scan_parens(rest, idx + 1, depth - 1, False, open_at)
              }
            _ -> scan_parens(rest, idx + 1, depth, False, open_at)
          }
      }
  }
}

fn scan_parens_no_backticks(
  gs: List(String),
  idx: Int,
  depth: Int,
  skip_at: Int,
) -> Result(Int, Nil) {
  case gs {
    [] -> Error(Nil)
    [_, ..rest] ->
      case idx == skip_at {
        True -> scan_parens_no_backticks(rest, idx + 1, depth, skip_at)
        False ->
          case gs {
            ["(", ..] ->
              scan_parens_no_backticks(rest, idx + 1, depth + 1, skip_at)
            [")", ..] ->
              case depth == 1 {
                True -> Ok(idx)
                False ->
                  scan_parens_no_backticks(rest, idx + 1, depth - 1, skip_at)
              }
            _ -> scan_parens_no_backticks(rest, idx + 1, depth, skip_at)
          }
      }
  }
}

/// Grapheme index of the first top-level two-char operator — outside parens
/// and backticks, with alphanumeric word boundaries (`a&&b` is bitwise junk,
/// not a logical split). Continues scanning when a boundary check fails.
fn find_logical_op(
  gs: List(String),
  op1: String,
  op2: String,
  depth: Int,
  backtick: Bool,
  prev: String,
  idx: Int,
) -> Result(Int, Nil) {
  case gs {
    [] -> Error(Nil)
    ["`", ..rest] ->
      find_logical_op(rest, op1, op2, depth, !backtick, "`", idx + 1)
    ["(", ..rest] ->
      case backtick {
        True -> find_logical_op(rest, op1, op2, depth, backtick, "(", idx + 1)
        False ->
          find_logical_op(rest, op1, op2, depth + 1, backtick, "(", idx + 1)
      }
    [")", ..rest] ->
      case backtick {
        True -> find_logical_op(rest, op1, op2, depth, backtick, ")", idx + 1)
        False -> find_logical_op(rest, op1, op2, depth - 1, False, ")", idx + 1)
      }
    [g, ..rest] ->
      case !backtick && depth == 0 && g == op1 {
        True ->
          case rest {
            [g2, next, ..] if g2 == op2 ->
              case is_alnum(prev) || is_alnum(next) {
                True ->
                  find_logical_op(rest, op1, op2, depth, backtick, g, idx + 1)
                False -> Ok(idx)
              }
            [g2] if g2 == op2 ->
              case is_alnum(prev) {
                True ->
                  find_logical_op(rest, op1, op2, depth, backtick, g, idx + 1)
                False -> Ok(idx)
              }
            _ -> find_logical_op(rest, op1, op2, depth, backtick, g, idx + 1)
          }
        False -> find_logical_op(rest, op1, op2, depth, backtick, g, idx + 1)
      }
  }
}
