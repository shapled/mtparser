//// AST nodes and their supporting types — every type lives here; the
//// functions that construct and manipulate them live in `helper`.

import gleam/option.{type Option}

/// Source location: 1-based line/column, offset into the source and length,
/// both in UTF-16 code units — the unit VS Code/LSP positions and JS string
/// indexing use, so consumers compare against `document.offsetAt` directly.
pub type Span {
  Span(line: Int, column: Int, offset: Int, len: Int)
}

/// One source line: raw text without its line ending, UTF-16 offset of the
/// line's first code unit, 1-based line number.
pub type Line {
  Line(text: String, offset: Int, number: Int)
}

// ── interpolated text ───────────────────────────────────────────────────────

/// A segment: literal text, or a variable reference (name without `$`).
pub type TextPart {
  Literal(text: String, span: Span)
  Variable(name: String, span: Span)
}

/// Text that may contain `$variable_name` references: an ordered list of
/// parts. An empty list is empty text.
pub type InterpolatedText =
  List(TextPart)

// ── condition expressions ───────────────────────────────────────────────────

/// Comparison operators.
pub type ComparisonOp {
  Eq
  // ==
  Neq
  // !=
  Lt
  // <
  Le
  // <=
  Gt
  // >
  Ge
  // >=
}

/// `$variable_name` reference.
pub type VariableRef {
  VariableRef(span: Span, name: String)
}

/// Top-level `&&` / `||` operators.
pub type LogicalOp {
  And
  // &&
  Or
  // ||
}

/// Backtick-enclosed query: `` `SELECT ...` ``. The content passes through
/// do_eval before being sent (mysqltest.cc var_query_set: "Eval the query,
/// thus replacing all environment variables" — identical in 5.7/26.7 and
/// MariaDB), so `$var` references are parsed out and the rest is the SQL;
/// SQL quotes do NOT protect a `$`.
pub type QueryExpr {
  QueryExpr(span: Span, query: InterpolatedText)
}

/// Right-hand side of a comparison.
pub type ComparisonRhs {
  RhsInteger(value: Int, span: Span)
  RhsString(value: String, span: Span)
  RhsVariable(variable: VariableRef)
  RhsQuery(query: QueryExpr)
}

/// A condition expression, shared by `assert`/`if`/`while`.
pub type Expr {
  /// `$var` truthiness reference.
  Var(variable: VariableRef)
  /// `!expr` — single negation.
  Negated(span: Span, expr: Expr)
  Integer(span: Span, value: Int)
  Query(query: QueryExpr)
  /// `$var [op rhs]`; no overall span (sub-nodes carry their own).
  Comparison(
    left: VariableRef,
    operator: ComparisonOp,
    operator_span: Span,
    right: ComparisonRhs,
  )
  /// `$(expr)` closure, e.g. `$(1 + 2)` — the inner arithmetic expression is
  /// kept raw (the calculator grammar is a runtime concern).
  Closure(span: Span, expression: String)
  /// Top-level `&&` / `||` expression, e.g. `0 && $have_debug`. `||` splits
  /// before `&&` (precedence), each side is a full expression — variables
  /// inside carry their own spans.
  Logical(span: Span, left: Expr, op: LogicalOp, right: Expr)
}

// ── statements ──────────────────────────────────────────────────────────────

/// The root AST node: one mysqltest statement, command, or line.
///
/// Unlike the Rust version (enum variants wrapping per-command structs),
/// labelled variant fields hold the payload directly — no wrapper structs.
pub type Statement {
  /// `# comment`
  Comment(span: Span, text: String)
  /// `-- command`
  DashCommand(span: Span, dash: Span, statement: Statement)

  /// `append_file file_name [marker]` … content … `marker`
  AppendFile(
    span: Span,
    filename: InterpolatedText,
    open: Marker,
    close: Marker,
    content: String,
  )
  /// `assert (expr)`
  Assert(span: Span, expression: Expr)
  /// `cat_file file_name`
  CatFile(span: Span, file: InterpolatedText)
  /// `change_user [user], [password], [db]` — empty segments are None.
  ChangeUser(
    span: Span,
    user: Option(InterpolatedText),
    password: Option(InterpolatedText),
    database: Option(InterpolatedText),
  )
  /// `character_set charset`
  CharacterSet(span: Span, charset: String)
  /// `chmod octal_mode file_name` — mode kept as text ("0644" preserves
  /// the leading zero).
  Chmod(span: Span, mode: String, file: InterpolatedText)
  /// `connect (name, host, …)` / `connect name` / `connect` (reconnect the
  /// current connection).
  Connect(span: Span, name: Option(InterpolatedText), params: ConnectParams)
  /// `connection name`
  Connection(span: Span, name: InterpolatedText)
  /// `copy_file from_file to_file [retry]`
  CopyFile(
    span: Span,
    source: InterpolatedText,
    dest: InterpolatedText,
    retry: Option(String),
  )
  /// `copy_files_wildcard src_dir dst_dir pattern [retry]`
  CopyFilesWildcard(
    span: Span,
    source_dir: InterpolatedText,
    dest_dir: InterpolatedText,
    pattern: InterpolatedText,
    retry: Option(String),
  )
  /// `dec $var` — `variable_span` covers the `$name` token (register `$0`-`$9`
  /// included); real line/column, always on the statement's first line.
  Dec(span: Span, variable: String, variable_span: Span)
  /// `enable_X` / `disable_X` — a boolean-setting toggle.
  Toggle(span: Span, kind: ToggleKind, enable: Bool)
  /// `force-cpdir src_dir dst_dir` — recursive directory copy.
  ForceCpdir(
    span: Span,
    source_dir: InterpolatedText,
    dest_dir: InterpolatedText,
  )
  /// `force-rmdir dir_name`
  ForceRmdir(span: Span, dir: InterpolatedText)
  /// `horizontal_results` — default result display format.
  HorizontalResults(span: Span)
  /// `let $var= value` / `let ENV_VAR= value` — `variable_span` covers the
  /// `$name` (or bare environment-variable name) token; real line/column,
  /// always on the statement's first line.
  Let(span: Span, variable: String, variable_span: Span, value: LetValue)
  /// `list_files dir [pattern]`
  ListFiles(
    span: Span,
    dir: Option(InterpolatedText),
    pattern: Option(InterpolatedText),
  )
  /// `list_files_append_file file dir [pattern]`
  ListFilesAppendFile(
    span: Span,
    file: InterpolatedText,
    dir: InterpolatedText,
    pattern: Option(InterpolatedText),
  )
  /// `list_files_write_file file dir [pattern]`
  ListFilesWriteFile(
    span: Span,
    file: InterpolatedText,
    dir: InterpolatedText,
    pattern: Option(InterpolatedText),
  )
  /// `lowercase_result`
  LowercaseResults(span: Span)
  /// `mkdir dir_name`
  Mkdir(span: Span, dir: InterpolatedText)
  /// `move_file from to [retry]`
  MoveFile(
    span: Span,
    source: InterpolatedText,
    dest: InterpolatedText,
    retry: Option(String),
  )
  /// `output file_name`
  Output(span: Span, file: InterpolatedText)
  /// `optimizer_trace` — (MariaDB) enable optimizer tracing.
  OptimizerTrace(span: Span)
  /// `ps_prepare statement` — (MariaDB, Galera) prepare for later execution.
  PsPrepare(span: Span, sql: InterpolatedText)
  /// `ps_bind value [value]...` — (MariaDB) positional parameter values,
  /// typed at runtime (int / decimal / string).
  PsBind(span: Span, params: List(InterpolatedText))
  /// `ps_execute` — (MariaDB) execute the current prepared statement.
  PsExecute(span: Span)
  /// `ps_close` — (MariaDB) close the current prepared statement.
  PsClose(span: Span)
  /// `skip_if_hypergraph` — (MySQL 8.0+) one-shot flag: skip the next query
  /// when running with --hypergraph.
  SkipIfHypergraph(span: Span)
  /// `write_line line file_name` — (MariaDB) append one line (a newline is
  /// added) to the file.
  WriteLine(span: Span, line: InterpolatedText, file: InterpolatedText)
  /// `evalp statement` — (MariaDB) prepare-execute-deallocate; shares
  /// eval's form verbatim.
  Evalp(span: Span, statement: InterpolatedText)
  /// `perl [terminator]` … code … `terminator` — the code is raw text (no
  /// variable interpolation).
  Perl(span: Span, open: Marker, close: Marker, content: String)
  /// `ping`
  Ping(span: Span)
  /// `query statement` — force-send to the server even if it starts with a
  /// command keyword.
  QueryCommand(span: Span, sql: InterpolatedText)
  /// `query_horizontal statement` — execute and display horizontally (the
  /// Rust version collapsed this into argument-less HorizontalResults,
  /// dropping the statement — fixed here).
  QueryHorizontal(span: Span, sql: InterpolatedText)
  /// `query_vertical statement` — execute and display vertically.
  QueryVertical(span: Span, sql: InterpolatedText)
  /// `reap` — collect the result of the last `send`.
  Reap(span: Span)
  /// `real_sleep num` — always uses the argument's delay, unlike sleep
  /// (which honors the --sleep option).
  RealSleep(span: Span, seconds: InterpolatedText)
  /// `require file_name` — (5.7) remember the result file for the next
  /// statement.
  Require(span: Span, file: InterpolatedText)
  /// `query_attributes name value [name value]...` — (8.0+) attach
  /// attributes to the next statement.
  QueryAttributes(
    span: Span,
    attributes: List(#(InterpolatedText, InterpolatedText)),
  )
  /// `system command` — (5.7, deprecated at runtime) shell command; parse
  /// only.
  System(span: Span, command: InterpolatedText)
  /// `remove_file file_name [retry]`
  RemoveFile(span: Span, file: InterpolatedText, retry: Option(String))
  /// `remove_files_wildcard dir pattern [retry]`
  RemoveFilesWildcard(
    span: Span,
    dir: InterpolatedText,
    pattern: InterpolatedText,
    retry: Option(String),
  )
  /// `replace_column col value [col value]...` — column numbers from 1;
  /// values may be double-quoted (variables expanded when unquoted).
  ReplaceColumn(span: Span, pairs: List(#(Int, InterpolatedText)))
  /// `replace_numeric_round precision` — digits after the decimal point.
  ReplaceNumericRound(span: Span, precision: Int)
  /// `replace_regex /pattern/replacement/[i] ...`
  ReplaceRegex(span: Span, rules: List(RegexRule))
  ReplaceRegexVariable(span: Span, variable: InterpolatedText)
  /// `replace_result from to [from to]...` — quoted or bare values,
  /// variables expanded, matched literally.
  ReplaceResult(span: Span, pairs: List(#(InterpolatedText, InterpolatedText)))
  /// `reset_connection`
  ResetConnection(span: Span)
  /// `result_format version` — 1 (default) or 2.
  ResultFormat(span: Span, version: Int)
  /// `if (expr) { body }` — the body ends at a `}` or `end` line (the block
  /// terminator is consumed by the if, never a standalone statement).
  If(
    span: Span,
    condition: Expr,
    body: List(Statement),
    open_brace: Span,
    close_brace: Span,
  )
  /// `while (expr) { body }` — same block structure as if.
  While(
    span: Span,
    condition: Expr,
    body: List(Statement),
    open_brace: Span,
    close_brace: Span,
  )
  /// `write_file file [marker]` … content … `marker` — the file must not
  /// exist.
  WriteFile(
    span: Span,
    filename: InterpolatedText,
    open: Marker,
    close: Marker,
    content: String,
  )
  /// `partially_sorted_result start_column` — first N columns presorted.
  PartiallySortedResult(span: Span, start_column: Int)
  /// `rmdir dir_name`
  Rmdir(span: Span, dir: InterpolatedText)
  /// `save_master_pos`
  SaveMasterPos(span: Span)
  /// `send [statement]` — no statement ⇒ applies to the next one.
  Send(span: Span, sql: Option(String))
  /// `send_eval [statement]` — eval + send.
  SendEval(span: Span, sql: Option(InterpolatedText))
  /// `send_quit connection`
  SendQuit(span: Span, connection: InterpolatedText)
  /// `send_shutdown`
  SendShutdown(span: Span)
  /// `shutdown_server [timeout]` — seconds; default 60 at runtime.
  ShutdownServer(span: Span, timeout: Option(InterpolatedText))
  /// `skip [message]`
  Skip(span: Span, message: Option(String))
  /// `sleep num` — fractional seconds allowed.
  Sleep(span: Span, seconds: InterpolatedText)
  /// `sorted_result`
  SortedResult(span: Span)
  /// `source file_name`
  Source(span: Span, file: InterpolatedText)
  /// `start_timer`
  StartTimer(span: Span)
  /// `sync_slave_with_master [connection]`
  SyncSlaveWithMaster(span: Span, connection: Option(InterpolatedText))
  /// `sync_with_master offset`
  SyncWithMaster(span: Span, offset: Option(InterpolatedText))
  /// `vertical_results`
  VerticalResults(span: Span)
  /// `wait_for_slave_to_stop`
  WaitForSlaveToStop(span: Span)
  /// `inc $var` — `variable_span` covers the `$name` token (register `$0`-`$9`
  /// included); real line/column, always on the statement's first line.
  Inc(span: Span, variable: String, variable_span: Span)
  /// `delimiter str` — the only command that changes parser state.
  Delimiter(span: Span, new_delimiter: String)
  /// `die [message]`
  Die(span: Span, message: Option(String))
  /// `diff_files file_name1 file_name2`
  DiffFiles(span: Span, file1: InterpolatedText, file2: InterpolatedText)
  /// `dirty_close connection_name`
  DirtyClose(span: Span, name: InterpolatedText)
  /// `disconnect connection_name`
  Disconnect(span: Span, name: InterpolatedText)
  /// `disable_testcase BUG#NNNN` — disable a section due to a known bug.
  DisableTestcase(span: Span, bug: String)
  /// `enable_testcase` — re-enable the section.
  EnableTestcase(span: Span)
  /// `echo text`
  Echo(span: Span, text: InterpolatedText)
  /// `end_timer` — stop the timer.
  EndTimer(span: Span)
  /// `error code[, code]...` — expected errors for the next command
  /// (numbers, `S`-prefixed SQLSTATE, or symbolic names).
  Error(span: Span, error_codes: List(InterpolatedText))
  /// `eval statement` — variable expansion, then sent to the server.
  Eval(span: Span, statement: InterpolatedText)
  /// `exec command [arg]...` — run a shell command.
  Exec(span: Span, command: InterpolatedText)
  /// `exec_in_background command [arg]...`
  ExecInBackground(span: Span, command: InterpolatedText)
  /// `execw command [arg]...` — Windows non-ASCII variant of exec.
  Execw(span: Span, command: InterpolatedText)
  /// `exit` — normal termination of the test case.
  Exit(span: Span)
  /// `expr $var= operand operator operand` — `variable_span` covers the
  /// `$name` token; the right-hand expression is kept raw (the calculator
  /// grammar is a runtime concern).
  Expr(span: Span, variable: String, variable_span: Span, expression: String)
  /// `file_exists file_name [retry]`
  FileExists(span: Span, file: InterpolatedText, retry: Option(String))

  /// SQL statement, or unrecognized command text (degraded).
  Sql(span: Span, sql: String)
}

/// A block terminator marker occurrence. A block declares its marker on the
/// command line (open, zero-length span when implicit) and closes with the
/// same marker alone on a line (close).
pub type Marker {
  Marker(name: String, span: Span)
}

/// One `/pattern/replacement/[i]` rule of `replace_regex`.
pub type RegexRule {
  RegexRule(pattern: String, replacement: String, insensitive: Bool)
}

/// The right-hand side of a `let` assignment.
pub type LetValue {
  /// Plain value (variable references substituted).
  Text(value: InterpolatedText)
  /// `` `query` `` — the query's result assigned to the variable.
  QueryValue(query: QueryExpr)
  /// `query_get_value(query, col, row [, resultset])` — 3 required args.
  QueryGetValue(span: Span, args: List(InterpolatedText))
  /// `escape(characters, string)` — string kept verbatim (whitespace is
  /// significant).
  Escape(span: Span, characters: InterpolatedText, string: InterpolatedText)
  /// `convert_error(error)` — number ↔ name.
  ConvertError(span: Span, argument: InterpolatedText)
}

/// The boolean settings toggled by `enable_X` / `disable_X`.
pub type ToggleKind {
  AbortOnError
  AsyncClient
  ConnectLog
  Info
  Metadata
  PsProtocol
  QueryLog
  ResultLog
  RplParse
  SessionTrackInfo
  Parsing
  Reconnect
  PrepareWarnings
  CursorProtocol
  NonBlockingApi
  Ps2Protocol
  ServiceConnection
  ViewProtocol
  ColumnNames
  Warnings
}

/// Position-based connection parameters for `connect (…)` — empty segments
/// are None.
pub type ConnectParams {
  ConnectParams(
    host: Option(InterpolatedText),
    user: Option(InterpolatedText),
    password: Option(InterpolatedText),
    database: Option(InterpolatedText),
    port: Option(InterpolatedText),
    socket: Option(InterpolatedText),
    default_charset: Option(InterpolatedText),
  )
}

/// The span of a statement.
pub fn span(statement: Statement) -> Span {
  case statement {
    Echo(span, _) -> span
    Comment(span, _) -> span
    DashCommand(span, ..) -> span
    Sql(span, _) -> span
    Assert(span, _) -> span
    CatFile(span, _) -> span
    ChangeUser(span, ..) -> span
    CharacterSet(span, _) -> span
    Chmod(span, _, _) -> span
    Connect(span, ..) -> span
    Connection(span, _) -> span
    CopyFile(span, ..) -> span
    CopyFilesWildcard(span, ..) -> span
    Dec(span, ..) -> span
    DiffFiles(span, _, _) -> span
    DirtyClose(span, _) -> span
    Disconnect(span, _) -> span
    EndTimer(span) -> span
    Error(span, _) -> span
    Eval(span, _) -> span
    Exec(span, _) -> span
    ExecInBackground(span, _) -> span
    Execw(span, _) -> span
    Exit(span) -> span
    Expr(span, ..) -> span
    FileExists(span, ..) -> span
    DisableTestcase(span, _) -> span
    EnableTestcase(span) -> span
    Toggle(span, ..) -> span
    ForceCpdir(span, ..) -> span
    ForceRmdir(span, _) -> span
    HorizontalResults(span) -> span
    If(span, ..) -> span
    While(span, ..) -> span
    WriteFile(span, ..) -> span
    PartiallySortedResult(span, _) -> span
    Rmdir(span, _) -> span
    SaveMasterPos(span) -> span
    Send(span, _) -> span
    SendEval(span, _) -> span
    SendQuit(span, _) -> span
    SendShutdown(span) -> span
    ShutdownServer(span, _) -> span
    Skip(span, _) -> span
    Sleep(span, _) -> span
    SortedResult(span) -> span
    Source(span, _) -> span
    StartTimer(span) -> span
    SyncSlaveWithMaster(span, _) -> span
    SyncWithMaster(span, _) -> span
    System(span, _) -> span
    VerticalResults(span) -> span
    WaitForSlaveToStop(span) -> span
    Inc(span, ..) -> span
    Let(span, ..) -> span
    ListFiles(span, ..) -> span
    ListFilesAppendFile(span, ..) -> span
    ListFilesWriteFile(span, ..) -> span
    LowercaseResults(span) -> span
    Mkdir(span, _) -> span
    MoveFile(span, ..) -> span
    Output(span, _) -> span
    OptimizerTrace(span) -> span
    PsPrepare(span, _) -> span
    PsBind(span, _) -> span
    PsExecute(span) -> span
    PsClose(span) -> span
    SkipIfHypergraph(span) -> span
    WriteLine(span, ..) -> span
    Evalp(span, _) -> span
    Perl(span, ..) -> span
    Ping(span) -> span
    QueryCommand(span, _) -> span
    QueryHorizontal(span, _) -> span
    QueryAttributes(span, _) -> span
    QueryVertical(span, _) -> span
    Reap(span) -> span
    RealSleep(span, _) -> span
    Require(span, _) -> span
    RemoveFile(span, ..) -> span
    RemoveFilesWildcard(span, ..) -> span
    ReplaceColumn(span, _) -> span
    ReplaceNumericRound(span, _) -> span
    ReplaceRegex(span, _) -> span
    ReplaceRegexVariable(span, _) -> span
    ReplaceResult(span, _) -> span
    ResetConnection(span) -> span
    ResultFormat(span, _) -> span
    Delimiter(span, _) -> span
    Die(span, _) -> span
    AppendFile(span, ..) -> span
  }
}
