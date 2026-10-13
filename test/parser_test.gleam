import gleam/list
import gleam/option
import gleeunit/should
import mtparser
import mtparser/ast.{
  And, AppendFile, Assert, CatFile, ChangeUser, CharacterSet, Chmod, Closure,
  Comment, Comparison, Connect, Connection, ConvertError, CopyFile,
  CopyFilesWildcard, DashCommand, Dec, Delimiter, Die, DiffFiles, DirtyClose,
  DisableTestcase, Disconnect, Echo, EnableTestcase, EndTimer, Eq, Escape, Eval,
  Evalp, Exec, ExecInBackground, Execw, Exit, Expr, FileExists, ForceCpdir,
  ForceRmdir, Ge, HorizontalResults, If, Inc, Integer, Let, ListFiles,
  ListFilesAppendFile, ListFilesWriteFile, Literal, Logical, LowercaseResults,
  Lt, Marker, Mkdir, MoveFile, Negated, Neq, OptimizerTrace, Or, Output,
  PartiallySortedResult, Perl, Ping, PsBind, PsClose, PsExecute, PsPrepare,
  Query, QueryAttributes, QueryCommand, QueryGetValue, QueryHorizontal,
  QueryValue, QueryVertical, RealSleep, Reap, RemoveFile, RemoveFilesWildcard,
  ReplaceColumn, ReplaceNumericRound, ReplaceRegex, ReplaceRegexVariable,
  ReplaceResult, Require, ResetConnection, ResultFormat, RhsInteger, RhsQuery,
  RhsString, RhsVariable, Rmdir, SaveMasterPos, Send, SendEval, SendQuit,
  SendShutdown, ShutdownServer, Skip, SkipIfHypergraph, Sleep, SortedResult,
  Source, Span, Sql, StartTimer, SyncSlaveWithMaster, SyncWithMaster, System,
  Text, Toggle, Var, Variable, VariableRef, VerticalResults, WaitForSlaveToStop,
  While, WriteFile, WriteLine,
}
import mtparser/error
import mtparser/helper
import mtparser/parser.{
  parse_expr, parse_interpolated_text, parse_interpolated_text_with_offset,
}

fn parsed_text(input: String) -> List(ast.TextPart) {
  parse_interpolated_text(input)
}

fn parsed_text_with_offset(input: String, offset: Int) -> List(ast.TextPart) {
  parse_interpolated_text_with_offset(input, offset)
}

fn parsed_expr(input: String) -> ast.Expr {
  let assert Ok(e) = parse_expr(input, 0)
  e
}

fn parsed(input: String) -> List(ast.Statement) {
  let assert Ok(statements) = mtparser.parse(input)
  statements
}

// ── echo ────────────────────────────────────────────────────────────────────

pub fn echo_dash_form_test() {
  parsed("--echo hello\n")
  |> should.equal([
    DashCommand(
      span: Span(1, 1, 0, 12),
      dash: Span(1, 1, 0, 2),
      statement: Echo(span: Span(1, 3, 2, 10), text: [
        Literal(text: "hello", span: Span(1, 1, 7, 5)),
      ]),
    ),
  ])
}

pub fn echo_bare_form_test() {
  parsed("echo hello;\n")
  |> should.equal([
    Echo(span: Span(1, 1, 0, 10), text: [
      Literal(text: "hello", span: Span(1, 1, 5, 5)),
    ]),
  ])
}

pub fn echo_bare_multiline_test() {
  let assert [Echo(span: span, text: t)] = parsed("echo a\nb;\n")
  helper.to_raw_string(t) |> should.equal("a\nb")
  span |> should.equal(Span(1, 1, 0, 8))
}

pub fn echo_case_insensitive_test() {
  let assert [DashCommand(_, _, Echo(_, t))] = parsed("--ECHO hi\n")
  helper.to_raw_string(t) |> should.equal("hi")
}

pub fn echo_indented_test() {
  parsed("  --echo hi\n")
  |> should.equal([
    DashCommand(
      span: Span(1, 3, 2, 9),
      dash: Span(1, 3, 2, 2),
      statement: Echo(span: Span(1, 5, 4, 7), text: [
        Literal(text: "hi", span: Span(1, 1, 9, 2)),
      ]),
    ),
  ])
}

pub fn echo_with_variable_test() {
  parsed("--echo n=$n\n")
  |> should.equal([
    DashCommand(
      span: Span(1, 1, 0, 11),
      dash: Span(1, 1, 0, 2),
      statement: Echo(span: Span(1, 3, 2, 9), text: [
        Literal(text: "n=", span: Span(1, 1, 7, 2)),
        Variable(name: "n", span: Span(1, 1, 9, 2)),
      ]),
    ),
  ])
}

pub fn echo_empty_arg_test() {
  parsed("--echo\n")
  |> should.equal([
    DashCommand(
      span: Span(1, 1, 0, 6),
      dash: Span(1, 1, 0, 2),
      statement: Echo(span: Span(1, 3, 2, 4), text: []),
    ),
  ])
}

// ── comments & blanks ───────────────────────────────────────────────────────

pub fn comment_test() {
  parsed("# note\n")
  |> should.equal([Comment(span: Span(1, 1, 0, 6), text: " note")])
}

pub fn blank_line_test() {
  parsed("\n") |> should.equal([])
}

// ── SQL ─────────────────────────────────────────────────────────────────────

pub fn sql_single_line_test() {
  parsed("SELECT 1;\n")
  |> should.equal([Sql(span: Span(1, 1, 0, 8), sql: "SELECT 1")])
}

pub fn sql_multiline_test() {
  parsed("SELECT\n1;\n")
  |> should.equal([Sql(span: Span(1, 1, 0, 8), sql: "SELECT\n1")])
}

pub fn sql_eof_without_delimiter_test() {
  parsed("SELECT 1\n")
  |> should.equal([Sql(span: Span(1, 1, 0, 8), sql: "SELECT 1")])
}

pub fn sql_delimiter_midline_test() {
  // mysqltest.cc match_delimiter is a pure character match: the unit ends
  // at the delimiter and the residue continues as the next reading unit.
  parsed("SELECT 1 ; extra\n")
  |> should.equal([
    Sql(span: Span(1, 1, 0, 8), sql: "SELECT 1"),
    Sql(span: Span(1, 2, 11, 5), sql: "extra"),
  ])
}

pub fn sql_delimiter_trailing_comment_test() {
  // The `#` residue becomes its own comment statement (mysqltest reads it
  // as the next unit via R_LINE_START).
  parsed("SELECT 1; # done\n")
  |> should.equal([
    Sql(span: Span(1, 1, 0, 8), sql: "SELECT 1"),
    Comment(span: Span(1, 2, 10, 6), text: " done"),
  ])
}

// ── degradation ─────────────────────────────────────────────────────────────

pub fn unknown_dash_command_errors_test() {
  // A `--` line is definitely a command: unknown names are real errors
  // (the bare form falls back to SQL instead).
  let assert Error(error.UnknownCommand("frobnicate", _, _)) =
    mtparser.parse("--frobnicate now\n")
}

pub fn unknown_bare_command_degrades_test() {
  parsed("frobnicate now;\n")
  |> should.equal([Sql(span: Span(1, 1, 0, 14), sql: "frobnicate now")])
}

// ── flow ────────────────────────────────────────────────────────────────────

pub fn mixed_statements_test() {
  let statements = parsed("--echo a\n\nSELECT 1;\n")
  statements |> list.length |> should.equal(2)

  let assert [DashCommand(_, _, Echo(_, t)), Sql(_, sql)] = statements
  helper.to_raw_string(t) |> should.equal("a")
  sql |> should.equal("SELECT 1")
}

pub fn crlf_test() {
  let assert [DashCommand(_, _, Echo(span: span, text: t))] =
    parsed("--echo hi\r\n")
  helper.to_raw_string(t) |> should.equal("hi")
  span |> should.equal(Span(1, 3, 2, 7))
}

pub fn no_trailing_empty_test() {
  // Final newline does not produce an Empty statement.
  parsed("SELECT 1;\n") |> list.length |> should.equal(1)
}

// ── assert ──────────────────────────────────────────────────────────────────

pub fn assert_dash_integer_test() {
  parsed("--assert(1)\n")
  |> should.equal([
    DashCommand(
      span: Span(1, 1, 0, 11),
      dash: Span(1, 1, 0, 2),
      statement: Assert(
        span: Span(1, 3, 2, 9),
        expression: Integer(span: Span(1, 1, 9, 1), value: 1),
      ),
    ),
  ])
}

pub fn assert_bare_variable_test() {
  parsed("assert($x);\n")
  |> should.equal([
    Assert(
      span: Span(1, 1, 0, 10),
      expression: Var(variable: VariableRef(span: Span(1, 1, 7, 2), name: "x")),
    ),
  ])
}

pub fn assert_bare_negated_test() {
  let assert [Assert(_, Negated(_, inner))] = parsed("assert(!$x);\n")
  inner
  |> should.equal(Var(variable: VariableRef(span: Span(1, 1, 8, 2), name: "x")))
}

pub fn assert_query_test() {
  let assert [Assert(_, Query(q))] =
    parsed("assert(`select count(*) from t1`);\n")
  helper.to_raw_string(q.query) |> should.equal("select count(*) from t1")
}

pub fn assert_comparison_test() {
  let assert [Assert(_, Comparison(left, _, _, _))] =
    parsed("assert($x == 1);\n")
  left.name |> should.equal("x")
}

pub fn assert_invalid_expression_test() {
  let assert Error(error.InvalidExpression(_, _)) =
    mtparser.parse("--assert ??\n")
}

// ── cat_file ────────────────────────────────────────────────────────────────

pub fn cat_file_dash_test() {
  parsed("--cat_file $MYSQL_TMP_DIR/data01\n")
  |> should.equal([
    DashCommand(
      span: Span(1, 1, 0, 32),
      dash: Span(1, 1, 0, 2),
      statement: CatFile(span: Span(1, 12, 11, 21), file: [
        // $MYSQL_TMP_DIR is a variable reference inside the path
        Variable(name: "MYSQL_TMP_DIR", span: Span(1, 1, 11, 14)),
        Literal(text: "/data01", span: Span(1, 1, 25, 7)),
      ]),
    ),
  ])
}

pub fn cat_file_bare_test() {
  parsed("cat_file $dir/f;\n")
  |> should.equal([
    CatFile(span: Span(1, 10, 9, 6), file: [
      Variable(name: "dir", span: Span(1, 1, 9, 4)),
      Literal(text: "/f", span: Span(1, 1, 13, 2)),
    ]),
  ])
}

// ── append_file ─────────────────────────────────────────────────────────────

pub fn append_file_default_marker_test() {
  parsed("append_file $MYSQL_TMP_DIR/data01;\nline three for the file\nEOF\n")
  |> should.equal([
    AppendFile(
      span: Span(1, 1, 0, 62),
      filename: [
        Variable(name: "MYSQL_TMP_DIR", span: Span(1, 1, 12, 14)),
        Literal(text: "/data01", span: Span(1, 1, 26, 7)),
      ],
      open: Marker(name: "EOF", span: Span(1, 34, 33, 0)),
      close: Marker(name: "EOF", span: Span(3, 1, 59, 3)),
      content: "line three for the file",
    ),
  ])
}

pub fn append_file_custom_marker_test() {
  let assert [
    AppendFile(span: span, filename: f, open: om, close: cm, content: c),
  ] =
    parsed(
      "append_file $MYSQL_TMP_DIR/data02 END_OF_FILE;\nline three for the file\nEND_OF_FILE\n",
    )
  span |> should.equal(Span(1, 1, 0, 82))
  helper.to_raw_string(f) |> should.equal("$MYSQL_TMP_DIR/data02")
  om.name |> should.equal("END_OF_FILE")
  om.span |> should.equal(Span(1, 35, 34, 11))
  cm.name |> should.equal("END_OF_FILE")
  cm.span |> should.equal(Span(3, 1, 71, 11))
  c |> should.equal("line three for the file")
}

pub fn append_file_dash_form_test() {
  let assert [
    DashCommand(
      _,
      _,
      AppendFile(_, filename: f, open: om, close: _, content: c),
    ),
  ] = parsed("--append_file $f EOF\ncontent line\nEOF\n")
  helper.to_raw_string(f) |> should.equal("$f")
  om.name |> should.equal("EOF")
  c |> should.equal("content line")
}

pub fn append_file_empty_content_test() {
  let assert [AppendFile(_, _, _, _, content: c)] =
    parsed("append_file $f;\nEOF\n")
  c |> should.equal("")
}

pub fn append_file_unterminated_test() {
  // Errors from known commands are real in both forms.
  let assert Error(error.UnterminatedBlock("append_file", "EOF", _)) =
    mtparser.parse("--append_file $f\nline one\n")
  let assert Error(error.UnterminatedBlock("append_file", "EOF", _)) =
    mtparser.parse("append_file $f;\nline one\n")
}

pub fn append_file_empty_filename_test() {
  // Errors from known commands are real in both forms.
  let assert Error(error.Syntax(_, _)) = mtparser.parse("append_file ;\nEOF\n")
  let assert Error(error.Syntax(_, _)) = mtparser.parse("--append_file\nEOF\n")
}

pub fn append_file_statement_after_block_test() {
  let statements = parsed("append_file $f;\nline\nEOF\nSELECT 1;\n")
  statements |> list.length |> should.equal(2)
  let assert [AppendFile(..), Sql(_, sql)] = statements
  sql |> should.equal("SELECT 1")
}

// ── explicit vs auto termination semantics ──────────────────────────────────

pub fn dash_command_ignores_delimiter_test() {
  // "A -- command is terminated by a newline, regardless of how many
  // delimiters it contains."
  let assert [DashCommand(_, _, Echo(_, t))] = parsed("--echo a;b;c\n")
  helper.to_raw_string(t) |> should.equal("a;b;c")
}

pub fn dash_block_body_hash_lines_are_content_test() {
  // `#`-prefixed lines inside a block body are content, not comments.
  let assert [
    DashCommand(_, _, AppendFile(_, filename: f, open: _, close: _, content: c)),
  ] =
    parsed(
      "--append_file $infile\n#-------------- MYSQLTEST_VARDIR/run --------------\nEOF\n",
    )
  helper.to_raw_string(f) |> should.equal("$infile")
  c |> should.equal("#-------------- MYSQLTEST_VARDIR/run --------------")
}

pub fn bare_append_file_requires_delimiter_test() {
  // Bare block commands go through delimiter accumulation; without the
  // delimiter the body lines are absorbed into the command text and the
  // block never terminates — a real error (only unknown commands fall back
  // to SQL).
  let assert Error(error.UnterminatedBlock("append_file", "EOF", _)) =
    mtparser.parse("append_file $f\nEOF\n")
}

pub fn bare_multi_line_command_test() {
  // Auto form: the command text itself may span lines until the delimiter.
  let assert [Echo(_, t)] = parsed("echo hello\nworld;\n")
  helper.to_raw_string(t) |> should.equal("hello\nworld")
}

// ── trim vs span invariant ──────────────────────────────────────────────────
// `span` always points at the trimmed command text's first byte; offsets are
// computed relative to that text, never the raw line.

pub fn indented_auto_command_span_test() {
  parsed("  echo hi;\n")
  |> should.equal([
    Echo(span: Span(1, 3, 2, 7), text: [
      Literal(text: "hi", span: Span(1, 1, 7, 2)),
    ]),
  ])
}

pub fn whitespace_before_delimiter_span_test() {
  // Trailing whitespace before the delimiter is not part of the statement.
  parsed("echo hi  ;\n")
  |> should.equal([
    Echo(span: Span(1, 1, 0, 7), text: [
      Literal(text: "hi", span: Span(1, 1, 5, 2)),
    ]),
  ])
}

pub fn multiline_auto_arg_offset_test() {
  // Arg spans lines: interior leading whitespace stays in the text.
  parsed("echo a\n  b;\n")
  |> should.equal([
    Echo(span: Span(1, 1, 0, 10), text: [
      Literal(text: "a\n  b", span: Span(1, 1, 5, 5)),
    ]),
  ])
}

pub fn delimiter_on_own_line_test() {
  parsed("SELECT 1\n  ;\n")
  |> should.equal([Sql(span: Span(1, 1, 0, 8), sql: "SELECT 1")])
}

pub fn indented_auto_token_span_test() {
  parsed("  cat_file $f;\n")
  |> should.equal([
    CatFile(span: Span(1, 12, 11, 2), file: [
      Variable(name: "f", span: Span(1, 1, 11, 2)),
    ]),
  ])
}

// ── quote-aware SQL termination ─────────────────────────────────────────────

pub fn sql_delimiter_in_single_quotes_test() {
  parsed("SELECT ';';\n")
  |> should.equal([Sql(span: Span(1, 1, 0, 10), sql: "SELECT ';'")])
}

pub fn sql_delimiter_in_double_quotes_test() {
  parsed("SELECT \"a;b\";\n")
  |> should.equal([Sql(span: Span(1, 1, 0, 12), sql: "SELECT \"a;b\"")])
}

pub fn sql_delimiter_in_backticks_test() {
  parsed("SELECT `x;y` FROM t;\n")
  |> should.equal([Sql(span: Span(1, 1, 0, 19), sql: "SELECT `x;y` FROM t")])
}

pub fn sql_quote_spans_lines_test() {
  // An unclosed quote carries across lines: the delimiter inside the string
  // body does not terminate.
  parsed("SELECT 'a\nb';\n")
  |> should.equal([Sql(span: Span(1, 1, 0, 12), sql: "SELECT 'a\nb'")])
}

pub fn sql_escaped_quote_test() {
  parsed("SELECT 'a\\'; ok';\n")
  |> should.equal([Sql(span: Span(1, 1, 0, 16), sql: "SELECT 'a\\'; ok'")])
}

pub fn sql_quote_before_delimiter_midline_test() {
  // Delimiter mid-line with non-comment remainder does not terminate.
  parsed("SELECT ';' extra;\n")
  |> should.equal([Sql(span: Span(1, 1, 0, 16), sql: "SELECT ';' extra")])
}

pub fn multiline_echo_three_lines_test() {
  // Multi-line echo: the command text accumulates until the delimiter.
  let assert [Echo(_, t)] = parsed("echo a\nb\nc;\n")
  helper.to_raw_string(t) |> should.equal("a\nb\nc")
}

pub fn apostrophe_in_bare_echo_test() {
  // Quote-aware accumulation applies to bare commands too (mysqltest reads
  // everything query-first): the apostrophe opens a quote, the delimiter is
  // inert, and the `;` ends up inside the echo text.
  let assert [Echo(_, t)] = parsed("echo doesn't;\n")
  helper.to_raw_string(t) |> should.equal("doesn't;")
}

pub fn cat_file_extra_tokens_test() {
  // The filename is the FIRST token; trailing text is not part of it (the
  // token is a prefix of the arg, so its position anchors at the arg start).
  parsed("--cat_file $f extra\n")
  |> should.equal([
    DashCommand(
      span: Span(1, 1, 0, 19),
      dash: Span(1, 1, 0, 2),
      statement: CatFile(span: Span(1, 12, 11, 2), file: [
        Variable(name: "f", span: Span(1, 1, 11, 2)),
      ]),
    ),
  ])
}

pub fn sql_backtick_backslash_escape_test() {
  // mysqltest.cc: backslash escapes the next character inside ALL quote
  // kinds, backticks included — an escaped backtick does not close.
  parsed("SELECT `a\\`b`;\n")
  |> should.equal([Sql(span: Span(1, 1, 0, 13), sql: "SELECT `a\\`b`")])
}

pub fn sql_backslash_quote_guard_test() {
  // mysqltest.cc: outside quotes a backslash prevents the next character
  // from opening a quote (`\'` stays literal), so the delimiter terminates.
  // The delimiter itself is never backslash-escaped.
  parsed("SELECT a\\'b;\n")
  |> should.equal([Sql(span: Span(1, 1, 0, 11), sql: "SELECT a\\'b")])
}

// ── change_user ─────────────────────────────────────────────────────────────

pub fn change_user_bare_test() {
  let assert [ChangeUser(_, user, password, database)] =
    parsed("change_user root;\n")
  helper.to_raw_string(option.unwrap(user, [])) |> should.equal("root")
  password |> should.equal(option.None)
  database |> should.equal(option.None)
}

pub fn change_user_dash_empty_segments_test() {
  let assert [DashCommand(_, _, ChangeUser(_, user, password, database))] =
    parsed("--change_user root,,test\n")
  helper.to_raw_string(option.unwrap(user, [])) |> should.equal("root")
  password |> should.equal(option.None)
  helper.to_raw_string(option.unwrap(database, [])) |> should.equal("test")
}

// ── character_set ───────────────────────────────────────────────────────────

pub fn character_set_test() {
  let assert [CharacterSet(_, charset)] = parsed("character_set utf8;\n")
  charset |> should.equal("utf8")
  let assert [DashCommand(_, _, CharacterSet(_, cs))] =
    parsed("--character_set sjis\n")
  cs |> should.equal("sjis")
}

// ── chmod ───────────────────────────────────────────────────────────────────

pub fn chmod_test() {
  let assert [Chmod(_, mode, file)] =
    parsed("chmod 0644 $MYSQL_TMP_DIR/data_xxx01;\n")
  mode |> should.equal("0644")
  helper.to_raw_string(file) |> should.equal("$MYSQL_TMP_DIR/data_xxx01")
}

// ── connect ─────────────────────────────────────────────────────────────────

pub fn connect_bare_reconnect_test() {
  let assert [Connect(_, name, params)] = parsed("connect;\n")
  name |> should.equal(option.None)
  params.host |> should.equal(option.None)
}

pub fn connect_name_only_test() {
  let assert [Connect(_, name, _)] = parsed("connect(conn1);\n")
  helper.to_raw_string(option.unwrap(name, [])) |> should.equal("conn1")
}

pub fn connect_all_empty_test() {
  let assert [Connect(_, name, params)] = parsed("connect(,,,);\n")
  name |> should.equal(option.None)
  params.host |> should.equal(option.None)
  params.database |> should.equal(option.None)
}

pub fn connect_positional_test() {
  let assert [Connect(_, name, params)] =
    parsed("connect (conn1,localhost,root,,);\n")
  helper.to_raw_string(option.unwrap(name, [])) |> should.equal("conn1")
  helper.to_raw_string(option.unwrap(params.host, []))
  |> should.equal("localhost")
  helper.to_raw_string(option.unwrap(params.user, [])) |> should.equal("root")
  params.password |> should.equal(option.None)
  params.database |> should.equal(option.None)
}

pub fn connect_port_variable_test() {
  let assert [Connect(_, _, params)] =
    parsed("connect (conn1,127.0.0.1,root,,test,$MASTER_MYPORT);\n")
  helper.to_raw_string(option.unwrap(params.database, []))
  |> should.equal("test")
  helper.to_raw_string(option.unwrap(params.port, []))
  |> should.equal("$MASTER_MYPORT")
  helper.variable_names(option.unwrap(params.port, []))
  |> should.equal(["MASTER_MYPORT"])
}

pub fn connect_bare_name_form_test() {
  let assert [DashCommand(_, _, Connect(_, name, params))] =
    parsed("--connect conn1\n")
  helper.to_raw_string(option.unwrap(name, [])) |> should.equal("conn1")
  params.host |> should.equal(option.None)
}

// ── connection ──────────────────────────────────────────────────────────────

pub fn connection_test() {
  let assert [Connection(_, name)] = parsed("connection master;\n")
  helper.to_raw_string(name) |> should.equal("master")
  let assert [Connection(_, name2)] = parsed("connection default;\n")
  helper.to_raw_string(name2) |> should.equal("default")
}

// ── copy_file ───────────────────────────────────────────────────────────────

pub fn copy_file_test() {
  let assert [CopyFile(_, source, dest, retry)] =
    parsed("copy_file $DIR/a.txt $DIR/b.txt;\n")
  helper.to_raw_string(source) |> should.equal("$DIR/a.txt")
  helper.to_raw_string(dest) |> should.equal("$DIR/b.txt")
  retry |> should.equal(option.None)
}

pub fn copy_file_retry_test() {
  let assert [CopyFile(_, _, _, retry)] =
    parsed("copy_file $DIR/foo.txt $DIR/foo2.txt 5;\n")
  retry |> should.equal(option.Some("5"))
}

// ── copy_files_wildcard ─────────────────────────────────────────────────────

pub fn copy_files_wildcard_test() {
  let assert [CopyFilesWildcard(_, src, dst, pattern, retry)] =
    parsed("copy_files_wildcard $V/std_data/ $V/copy1/ *.txt;\n")
  helper.to_raw_string(src) |> should.equal("$V/std_data/")
  helper.to_raw_string(dst) |> should.equal("$V/copy1/")
  helper.to_raw_string(pattern) |> should.equal("*.txt")
  retry |> should.equal(option.None)
}

pub fn copy_files_wildcard_retry_test() {
  let assert [CopyFilesWildcard(_, _, _, _, retry)] =
    parsed("copy_files_wildcard $V/std_data/ $V/copy1/ *.txt 5;\n")
  retry |> should.equal(option.Some("5"))
}

// ── dec ─────────────────────────────────────────────────────────────────────

pub fn dec_test() {
  let assert [Dec(_, variable, span)] = parsed("dec $count;\n")
  variable |> should.equal("count")
  // The span covers the `$count` token, on the statement's first line.
  span |> should.equal(Span(1, 5, 4, 6))
  let assert [Dec(_, v2, span2)] = parsed("dec $2;\n")
  v2 |> should.equal("2")
  span2 |> should.equal(Span(1, 5, 4, 2))
}

pub fn dec_empty_variable_test() {
  let assert Error(error.Syntax(_, _)) = mtparser.parse("dec;\n")
}

// ── delimiter ───────────────────────────────────────────────────────────────

pub fn delimiter_test() {
  let assert [Delimiter(_, d)] = parsed("delimiter /;\n")
  d |> should.equal("/")
  let assert [DashCommand(_, _, Delimiter(_, d2))] =
    parsed("--delimiter stop\n")
  d2 |> should.equal("stop")
}

pub fn delimiter_changes_state_test() {
  // The unit is read under the OLD delimiter; the new one applies from the
  // next statement on.
  let assert [Delimiter(_, _), Sql(_, sql)] =
    parsed("delimiter //;\nSELECT 1//\n")
  sql |> should.equal("SELECT 1")
}

pub fn delimiter_missing_argument_test() {
  let assert Error(error.Syntax(_, _)) = mtparser.parse("delimiter;\n")
}

// ── die ─────────────────────────────────────────────────────────────────────

pub fn die_test() {
  let assert [Die(_, message)] = parsed("die Cannot continue;\n")
  option.unwrap(message, "") |> should.equal("Cannot continue")
}

pub fn die_no_message_test() {
  let assert [Die(_, message)] = parsed("die;\n")
  message |> should.equal(option.None)
}

pub fn die_message_with_variable_test() {
  // The message is verbatim (die("%s", first_argument)) — `$x` is NOT a
  // variable reference.
  let assert [Die(_, message)] = parsed("die failed at $x;\n")
  option.unwrap(message, "") |> should.equal("failed at $x")
}

// ── interpolated text (parser.parse_interpolated_text) ─────────────────────

pub fn pure_literal_test() {
  let t = parsed_text("hello world")
  helper.is_literal(t) |> should.be_true
  helper.to_raw_string(t) |> should.equal("hello world")
  list.length(t) |> should.equal(1)
}

pub fn single_variable_test() {
  let t = parsed_text("$mysql_errno")
  helper.is_literal(t) |> should.be_false
  helper.to_raw_string(t) |> should.equal("$mysql_errno")
  helper.variable_names(t) |> should.equal(["mysql_errno"])
}

pub fn mixed_test() {
  let t = parsed_text("counter is $counter")
  helper.to_raw_string(t) |> should.equal("counter is $counter")
  should.equal(t, [
    Literal(text: "counter is ", span: Span(1, 1, 0, 11)),
    Variable(name: "counter", span: Span(1, 1, 11, 8)),
  ])
}

pub fn escaped_dollar_test() {
  let t = parsed_text("price is \\$10")
  helper.is_literal(t) |> should.be_true
  helper.to_raw_string(t) |> should.equal("price is $10")
}

pub fn dollar_register_test() {
  // `$0`-`$9` are register variables (mysqltest.cc var_get: var_reg+digit) —
  // a single-digit name; the rest of `$10` is a literal.
  let t = parsed_text("price is $10")
  helper.variable_names(t) |> should.equal(["1"])
  helper.to_raw_string(t) |> should.equal("price is $10")
  let t2 = parsed_text("$0")
  helper.variable_names(t2) |> should.equal(["0"])
}

pub fn dollar_at_end_test() {
  let t = parsed_text("total$")
  helper.is_literal(t) |> should.be_true
  helper.to_raw_string(t) |> should.equal("total$")
}

pub fn multiple_variables_test() {
  let t = parsed_text("$host:$port/$db")
  helper.variable_names(t) |> should.equal(["host", "port", "db"])
  helper.to_raw_string(t) |> should.equal("$host:$port/$db")
}

pub fn variable_with_dash_test() {
  let t = parsed_text("$MYSQL_TMP_DIR/path")
  helper.variable_names(t) |> should.equal(["MYSQL_TMP_DIR"])
}

pub fn empty_string_test() {
  let t = parsed_text("")
  helper.is_literal(t) |> should.be_true
  helper.to_raw_string(t) |> should.equal("")
}

pub fn span_offsets_test() {
  parsed_text("ab$cd")
  |> should.equal([
    Literal(text: "ab", span: Span(1, 1, 0, 2)),
    Variable(name: "cd", span: Span(1, 1, 2, 3)),
  ])
}

pub fn span_with_base_offset_test() {
  parse_interpolated_text_with_offset("ab$cd", 100)
  |> should.equal([
    Literal(text: "ab", span: Span(1, 1, 100, 2)),
    Variable(name: "cd", span: Span(1, 1, 102, 3)),
  ])
}

pub fn variable_with_underscore_test() {
  let t = parsed_text("$my_var_name")
  helper.variable_names(t) |> should.equal(["my_var_name"])
}

pub fn trailing_backslash_test() {
  let t = parsed_text("path\\")
  helper.is_literal(t) |> should.be_true
  helper.to_raw_string(t) |> should.equal("path\\")
}

// ── expressions (parser.parse_expr) ─────────────────────────────────────────

pub fn integer_test() {
  parsed_expr("1") |> should.equal(Integer(span: Span(1, 1, 0, 1), value: 1))
}

pub fn parens_stripped_test() {
  parsed_expr("(1)") |> should.equal(Integer(span: Span(1, 1, 1, 1), value: 1))
}

pub fn variable_test() {
  parsed_expr("$x")
  |> should.equal(Var(variable: VariableRef(span: Span(1, 1, 0, 2), name: "x")))
}

pub fn negated_test() {
  parsed_expr("!$x")
  |> should.equal(Negated(
    span: Span(1, 1, 0, 1),
    expr: Var(variable: VariableRef(span: Span(1, 1, 1, 2), name: "x")),
  ))
}

pub fn negated_in_parens_test() {
  parsed_expr("(!$x)")
  |> should.equal(Negated(
    span: Span(1, 1, 1, 1),
    expr: Var(variable: VariableRef(span: Span(1, 1, 2, 2), name: "x")),
  ))
}

pub fn comparison_integer_rhs_test() {
  // $x == 1  →  var(0,2) op(3,2) rhs(6,1)
  parsed_expr("$x == 1")
  |> should.equal(Comparison(
    left: VariableRef(span: Span(1, 1, 0, 2), name: "x"),
    operator: Eq,
    operator_span: Span(1, 1, 3, 2),
    right: RhsInteger(value: 1, span: Span(1, 1, 6, 1)),
  ))
}

pub fn comparison_no_space_test() {
  parsed_expr("$x<3")
  |> should.equal(Comparison(
    left: VariableRef(span: Span(1, 1, 0, 2), name: "x"),
    operator: Lt,
    operator_span: Span(1, 1, 2, 1),
    right: RhsInteger(value: 3, span: Span(1, 1, 3, 1)),
  ))
}

pub fn comparison_quoted_string_rhs_test() {
  // $x == "abc" → content span excludes quotes
  let assert Comparison(_, _, _, RhsString(value, span)) =
    parsed_expr("$x == \"abc\"")
  value |> should.equal("abc")
  span |> should.equal(Span(1, 1, 7, 3))
}

pub fn comparison_variable_rhs_test() {
  let assert Comparison(_, _, _, RhsVariable(v)) = parsed_expr("$a != $b")
  v.name |> should.equal("b")
}

pub fn comparison_query_rhs_test() {
  let assert Comparison(left, _, _, RhsQuery(q)) =
    parsed_expr("$a == `select 1`")
  left.name |> should.equal("a")
  helper.to_raw_string(q.query) |> should.equal("select 1")
  q.span |> should.equal(Span(1, 1, 6, 10))
}

pub fn comparison_ge_test() {
  let assert Comparison(_, op, _, _) = parsed_expr("$x >= 5")
  op |> should.equal(Ge)
}

pub fn bare_string_rhs_test() {
  let assert Comparison(_, _, _, RhsString(value, _)) = parsed_expr("$x == abc")
  value |> should.equal("abc")
}

pub fn query_test() {
  // `select count(*) from t1` — span includes both backticks
  let assert Query(q) = parsed_expr("`select count(*) from t1`")
  helper.to_raw_string(q.query) |> should.equal("select count(*) from t1")
  q.span |> should.equal(Span(1, 1, 0, 25))
}

pub fn closure_test() {
  parsed_expr("$(1 + 2)")
  |> should.equal(Closure(span: Span(1, 1, 0, 8), expression: "1 + 2"))
}

pub fn logical_test() {
  parsed_expr("0 && $have_debug")
  |> should.equal(Logical(
    span: Span(1, 1, 0, 16),
    left: Integer(span: Span(1, 1, 0, 1), value: 0),
    op: And,
    right: Var(VariableRef(span: Span(1, 1, 5, 11), name: "have_debug")),
  ))
}

pub fn logical_precedence_test() {
  // `||` splits before `&&`: the `&&` binds its two neighbors.
  let assert Logical(_, left, Or, right) = parsed_expr("$a || $b && $c")
  left |> should.equal(Var(VariableRef(span: Span(1, 1, 0, 2), name: "a")))
  let assert Logical(_, l2, And, r2) = right
  l2 |> should.equal(Var(VariableRef(span: Span(1, 1, 6, 2), name: "b")))
  r2 |> should.equal(Var(VariableRef(span: Span(1, 1, 12, 2), name: "c")))
}

pub fn logical_paren_operand_test() {
  // A leading paren that does not wrap the whole expression is not stripped
  // (the old strip dropped everything after `($a)`).
  let assert Logical(_, left, And, right) = parsed_expr("($a) && $b")
  left |> should.equal(Var(VariableRef(span: Span(1, 1, 1, 2), name: "a")))
  right |> should.equal(Var(VariableRef(span: Span(1, 1, 8, 2), name: "b")))
}

pub fn logical_negated_operand_test() {
  let assert Logical(_, _, And, Negated(_, Var(VariableRef(_, "b")))) =
    parsed_expr("$a && !$b")
}

pub fn logical_comparison_sides_test() {
  let assert Logical(
    _,
    Comparison(VariableRef(_, "left"), Eq, _, RhsInteger(1, _)),
    And,
    Comparison(VariableRef(_, "right"), Neq, _, RhsInteger(2, _)),
  ) = parsed_expr("$left == 1 && $right != 2")
}

pub fn nested_parens_strip_test() {
  // Wrapping paren pairs strip repeatedly.
  parsed_expr("(($a))")
  |> should.equal(Var(VariableRef(span: Span(1, 1, 2, 2), name: "a")))
}

pub fn query_variable_interpolation_test() {
  // mysqltest.cc var_query_set do_evals the content before sending — SQL
  // quotes do not protect a `$` (rpl_sync_relay_log_info.inc's real form).
  let assert Query(q) = parsed_expr("`SELECT '$engine' <> 'INNODB'`")
  q.query
  |> should.equal(parsed_text_with_offset("SELECT '$engine' <> 'INNODB'", 1))
  let assert [Variable(name, span)] =
    list.filter(q.query, fn(part) {
      case part {
        Variable(_, _) -> True
        _ -> False
      }
    })
  name |> should.equal("engine")
  span |> should.equal(Span(1, 1, 9, 7))
}

pub fn let_query_value_variables_test() {
  // The let backtick form is eval'd too — references carry spans.
  let assert [Let(_, _, _, QueryValue(query))] =
    parsed("let $v= `SELECT $a, $b`;\n")
  let assert [Variable("a", s1), Variable("b", s2)] =
    list.filter(query.query, fn(part) {
      case part {
        Variable(_, _) -> True
        _ -> False
      }
    })
  // `let $v= \`SELECT $a, $b` — $a at 16, $b at 20.
  s1 |> should.equal(Span(1, 1, 16, 2))
  s2 |> should.equal(Span(1, 1, 20, 2))
}

pub fn nested_parens_query_test() {
  // Backtick content containing parens must not confuse paren matching.
  let assert Query(q) = parsed_expr("(`select count(*) from t1`)")
  helper.to_raw_string(q.query) |> should.equal("select count(*) from t1")
}

pub fn empty_test() {
  parser.parse_expr("", 0) |> should.equal(Error(Nil))
}

pub fn garbage_test() {
  parser.parse_expr("???", 0) |> should.equal(Error(Nil))
}

// ── diff_files ──────────────────────────────────────────────────────────────

pub fn diff_files_test() {
  let assert [DiffFiles(_, file1, file2)] =
    parsed("diff_files $DIR/a.txt $DIR/b.txt;\n")
  helper.to_raw_string(file1) |> should.equal("$DIR/a.txt")
  helper.to_raw_string(file2) |> should.equal("$DIR/b.txt")
}

// ── dirty_close ─────────────────────────────────────────────────────────────

pub fn dirty_close_test() {
  let assert [DashCommand(_, _, DirtyClose(_, name))] =
    parsed("--dirty_close conn1\n")
  helper.to_raw_string(name) |> should.equal("conn1")
  let assert [DirtyClose(_, name2)] = parsed("dirty_close $conn;\n")
  helper.variable_names(name2) |> should.equal(["conn"])
}

// ── testcase ────────────────────────────────────────────────────────────────

pub fn disable_testcase_test() {
  let assert [DisableTestcase(_, bug)] = parsed("disable_testcase BUG#12345;\n")
  bug |> should.equal("BUG#12345")
}

pub fn disable_testcase_missing_bug_test() {
  let assert Error(error.Syntax(_, _)) = mtparser.parse("disable_testcase;\n")
}

pub fn enable_testcase_test() {
  let assert [EnableTestcase(_)] = parsed("enable_testcase;\n")
}

// ── toggles ─────────────────────────────────────────────────────────────────

pub fn toggle_disable_query_log_test() {
  let assert [Toggle(_, kind, enable)] = parsed("disable_query_log;\n")
  kind |> should.equal(ast.QueryLog)
  enable |> should.be_false
  let assert [DashCommand(_, _, Toggle(_, k2, e2))] =
    parsed("--enable_query_log\n")
  k2 |> should.equal(ast.QueryLog)
  e2 |> should.be_true
}

pub fn toggle_warnings_both_forms_test() {
  let assert [Toggle(_, k1, e1)] = parsed("disable_warnings;\n")
  k1 |> should.equal(ast.Warnings)
  e1 |> should.be_false
  let assert [Toggle(_, k2, e2)] = parsed("enable_warnings;\n")
  k2 |> should.equal(ast.Warnings)
  e2 |> should.be_true
}

pub fn toggle_various_kinds_test() {
  let assert [Toggle(_, k1, _)] = parsed("disable_abort_on_error;\n")
  k1 |> should.equal(ast.AbortOnError)
  let assert [Toggle(_, k2, _)] = parsed("enable_async_client;\n")
  k2 |> should.equal(ast.AsyncClient)
  let assert [Toggle(_, k3, _)] = parsed("disable_connect_log;\n")
  k3 |> should.equal(ast.ConnectLog)
  let assert [Toggle(_, k4, _)] = parsed("enable_info;\n")
  k4 |> should.equal(ast.Info)
  let assert [Toggle(_, k5, _)] = parsed("disable_metadata;\n")
  k5 |> should.equal(ast.Metadata)
  let assert [Toggle(_, k6, _)] = parsed("enable_ps_protocol;\n")
  k6 |> should.equal(ast.PsProtocol)
  let assert [Toggle(_, k7, _)] = parsed("disable_result_log;\n")
  k7 |> should.equal(ast.ResultLog)
  let assert [Toggle(_, k8, _)] = parsed("enable_rpl_parse;\n")
  k8 |> should.equal(ast.RplParse)
  let assert [Toggle(_, k9, _)] = parsed("disable_session_track_info;\n")
  k9 |> should.equal(ast.SessionTrackInfo)
}

pub fn toggle_unknown_setting_test() {
  // Unknown settings are not commands: bare degrades to SQL, dash errors.
  let assert [Sql(_, sql)] = parsed("disable_nonsense;\n")
  sql |> should.equal("disable_nonsense")
  let assert Error(error.UnknownCommand("disable_nonsense", _, _)) =
    mtparser.parse("--disable_nonsense\n")
}

pub fn toggle_parsing_reconnect_test() {
  let assert [Toggle(_, k1, e1)] = parsed("disable_parsing;\n")
  k1 |> should.equal(ast.Parsing)
  e1 |> should.be_false
  let assert [Toggle(_, k2, e2)] = parsed("enable_parsing;\n")
  k2 |> should.equal(ast.Parsing)
  e2 |> should.be_true
  let assert [Toggle(_, k3, _)] = parsed("disable_reconnect;\n")
  k3 |> should.equal(ast.Reconnect)
  let assert [DashCommand(_, _, Toggle(_, k4, e4))] =
    parsed("--enable_reconnect\n")
  k4 |> should.equal(ast.Reconnect)
  e4 |> should.be_true
}

pub fn toggle_mariadb_kinds_test() {
  let assert [Toggle(_, k1, e1)] = parsed("disable_prepare_warnings;\n")
  k1 |> should.equal(ast.PrepareWarnings)
  e1 |> should.be_false
  let assert [Toggle(_, k2, _)] = parsed("enable_cursor_protocol;\n")
  k2 |> should.equal(ast.CursorProtocol)
  let assert [Toggle(_, k3, _)] = parsed("disable_non_blocking_api;\n")
  k3 |> should.equal(ast.NonBlockingApi)
  let assert [Toggle(_, k4, _)] = parsed("enable_ps2_protocol;\n")
  k4 |> should.equal(ast.Ps2Protocol)
  let assert [Toggle(_, k5, _)] = parsed("disable_service_connection;\n")
  k5 |> should.equal(ast.ServiceConnection)
  let assert [Toggle(_, k6, _)] = parsed("enable_view_protocol;\n")
  k6 |> should.equal(ast.ViewProtocol)
  let assert [DashCommand(_, _, Toggle(_, k7, e7))] =
    parsed("--disable_column_names\n")
  k7 |> should.equal(ast.ColumnNames)
  e7 |> should.be_false
}

// ── disconnect ──────────────────────────────────────────────────────────────

pub fn disconnect_test() {
  let assert [Disconnect(_, name)] = parsed("disconnect conn2;\n")
  helper.to_raw_string(name) |> should.equal("conn2")
}

// ── end_timer / exit / horizontal_results ───────────────────────────────────

pub fn no_args_commands_test() {
  let assert [EndTimer(_)] = parsed("end_timer;\n")
  let assert [DashCommand(_, _, Exit(_))] = parsed("--exit\n")
  let assert [DashCommand(_, _, HorizontalResults(_))] =
    parsed("--horizontal_results\n")
}

// ── error ───────────────────────────────────────────────────────────────────

pub fn error_single_code_test() {
  let assert [DashCommand(_, _, ast.Error(_, codes))] = parsed("--error 1050\n")
  codes |> list.length |> should.equal(1)
  codes |> list.map(helper.to_raw_string) |> should.equal(["1050"])
}

pub fn error_sqlstate_and_symbolic_test() {
  let assert [DashCommand(_, _, ast.Error(_, codes))] =
    parsed("--error S42S01\n")
  codes |> list.map(helper.to_raw_string) |> should.equal(["S42S01"])
  let assert [DashCommand(_, _, ast.Error(_, codes2))] =
    parsed("--error ER_TABLE_EXISTS_ERROR\n")
  codes2
  |> list.map(helper.to_raw_string)
  |> should.equal(["ER_TABLE_EXISTS_ERROR"])
}

pub fn error_comma_list_test() {
  let assert [DashCommand(_, _, ast.Error(_, codes))] =
    parsed("--error 1050, 1146,ER_BAD\n")
  codes
  |> list.map(helper.to_raw_string)
  |> should.equal(["1050", "1146", "ER_BAD"])
}

// ── eval / exec family ──────────────────────────────────────────────────────

pub fn eval_test() {
  let assert [Eval(_, stmt)] = parsed("eval USE $DB;\n")
  helper.to_raw_string(stmt) |> should.equal("USE $DB")
  helper.variable_names(stmt) |> should.equal(["DB"])
}

pub fn exec_family_test() {
  let assert [DashCommand(_, _, Exec(_, cmd))] =
    parsed("--exec rm $MYSQLTEST_VARDIR/tmp/t1\n")
  helper.to_raw_string(cmd) |> should.equal("rm $MYSQLTEST_VARDIR/tmp/t1")
  let assert [Execw(_, cmd2)] = parsed("execw $CMD --flag;\n")
  helper.to_raw_string(cmd2) |> should.equal("$CMD --flag")
  let assert [DashCommand(_, _, ExecInBackground(_, cmd3))] =
    parsed("--exec_in_background $DUMP --xml test\n")
  helper.to_raw_string(cmd3) |> should.equal("$DUMP --xml test")
}

// ── expr ────────────────────────────────────────────────────────────────────

pub fn expr_test() {
  let assert [DashCommand(_, _, Expr(_, variable, _, expression))] =
    parsed("--expr $res= $var1 + $var2\n")
  variable |> should.equal("res")
  expression |> should.equal("$var1 + $var2")
}

pub fn expr_missing_equals_test() {
  let assert Error(error.Syntax(_, _)) = mtparser.parse("--expr $res\n")
}

// ── file_exists / force-cpdir / force-rmdir ────────────────────────────────

pub fn file_exists_test() {
  let assert [FileExists(_, file, retry)] = parsed("file_exists /etc/passwd;\n")
  helper.to_raw_string(file) |> should.equal("/etc/passwd")
  retry |> should.equal(option.None)
  let assert [FileExists(_, file2, retry2)] = parsed("file_exists $DIR/f 5;\n")
  helper.to_raw_string(file2) |> should.equal("$DIR/f")
  retry2 |> should.equal(option.Some("5"))
}

pub fn force_dir_commands_test() {
  let assert [DashCommand(_, _, ForceCpdir(_, src, dst))] =
    parsed("--force-cpdir testdir testdir2\n")
  helper.to_raw_string(src) |> should.equal("testdir")
  helper.to_raw_string(dst) |> should.equal("testdir2")
  let assert [DashCommand(_, _, ForceRmdir(_, dir))] =
    parsed("--force-rmdir testdir\n")
  helper.to_raw_string(dir) |> should.equal("testdir")
}

// ── inc ─────────────────────────────────────────────────────────────────────

pub fn inc_test() {
  let assert [DashCommand(_, _, Inc(_, variable, span))] = parsed("--inc $i;\n")
  variable |> should.equal("i")
  // Column 7: the unit span starts at `inc` (column 3), the token 4 bytes in.
  span |> should.equal(Span(1, 7, 6, 2))
  let assert [Inc(_, v2, span2)] = parsed("inc $3;\n")
  v2 |> should.equal("3")
  span2 |> should.equal(Span(1, 5, 4, 2))
}

// ── variable token spans ────────────────────────────────────────────────────

pub fn let_variable_span_test() {
  let assert [DashCommand(_, _, Let(_, name, span, Text(_)))] =
    parsed("--let $foo= 1\n")
  name |> should.equal("foo")
  span |> should.equal(Span(1, 7, 6, 4))
  // Spaces around `=`: the token ends at the trimmed lhs boundary.
  let assert [Let(_, _, span2, Text(_))] = parsed("let $x = 1;\n")
  span2 |> should.equal(Span(1, 5, 4, 2))
  // Environment-variable form: no `$`, the bare name is the token.
  let assert [Let(_, _, span3, Text(_))] = parsed("let MY_ENV= bar;\n")
  span3 |> should.equal(Span(1, 5, 4, 6))
  // A statement on a later line keeps the real line number.
  let assert [_, _, Let(_, _, span4, Text(_))] =
    parsed("# c\n\n--echo x\nlet $late= 1;\n")
  span4 |> should.equal(Span(4, 5, 18, 5))
}

pub fn expr_variable_span_test() {
  let assert [DashCommand(_, _, Expr(_, name, span, expression))] =
    parsed("--expr $total= $a + $b\n")
  name |> should.equal("total")
  span |> should.equal(Span(1, 8, 7, 6))
  expression |> should.equal("$a + $b")
}

// ── if blocks ───────────────────────────────────────────────────────────────

pub fn if_same_line_brace_test() {
  let assert [If(_, condition, body, open_brace, close_brace)] =
    parsed("if ($x) {\n--echo hi\n}\n")
  condition
  |> should.equal(Var(variable: VariableRef(span: Span(1, 1, 4, 2), name: "x")))
  body |> list.length |> should.equal(1)
  open_brace.offset |> should.equal(8)
  close_brace.offset |> should.equal(20)
}

pub fn if_inner_space_condition_test() {
  // Real-suite form (rpl_create_msr_channels.inc): spaces inside the parens.
  let assert [If(_, condition, body, _, _)] =
    parsed("if ( $i ==1) {\n--echo x\n}\n")
  let assert Comparison(VariableRef(_, "i"), Eq, _, RhsInteger(1, _)) =
    condition
  body |> list.length |> should.equal(1)
}

pub fn if_trailing_delimiter_condition_test() {
  // Real-suite form (partition_alter_1.inc): the bare if's `;` delimiter
  // rides along in the head-to-`{` read — it is not part of the condition.
  let assert [If(_, Query(q), _, _, _)] =
    parsed("if (`SELECT @x NOT IN (0,42)`);\n{\n--echo x\n}\n")
  helper.to_raw_string(q.query) |> should.equal("SELECT @x NOT IN (0,42)")
}

pub fn if_own_line_brace_test() {
  let assert [If(_, _, body, open_brace, _)] =
    parsed("if ($x)\n{\n--echo hi\n}\n")
  body |> list.length |> should.equal(1)
  open_brace.line |> should.equal(2)
  open_brace.column |> should.equal(1)
}

pub fn if_end_terminator_test() {
  let assert [If(_, _, body, _, close_brace)] =
    parsed("if ($x) {\n--echo hi\nend\n")
  body |> list.length |> should.equal(1)
  close_brace.len |> should.equal(3)
}

pub fn if_negated_condition_test() {
  let assert [If(_, Negated(_, _), _, _, _)] =
    parsed("if (!$x) {\n--echo zero\n}\n")
}

pub fn if_nested_test() {
  let assert [If(_, _, outer_body, _, _)] =
    parsed("if ($a) {\nif ($b) {\n--echo inner\n}\n--echo outer\n}\n")
  outer_body |> list.length |> should.equal(2)
  let assert [If(_, _, inner_body, _, _), _] = outer_body
  inner_body |> list.length |> should.equal(1)
}

pub fn if_statement_after_block_test() {
  let assert [If(_, _, _, _, _), Sql(_, sql)] =
    parsed("if ($x) {\n--echo hi\n}\nSELECT 1;\n")
  sql |> should.equal("SELECT 1")
}

pub fn if_unterminated_brace_test() {
  let assert Error(error.UnterminatedFlowControl("if", _)) =
    mtparser.parse("if ($x)\n--echo no brace\n")
}

pub fn if_unterminated_block_test() {
  let assert Error(error.UnterminatedFlowControl("if", _)) =
    mtparser.parse("if ($x) {\n--echo hi\n")
}

pub fn stray_terminators_test() {
  let assert Error(error.Syntax(_, _)) = mtparser.parse("}\n")
  let assert Error(error.Syntax(_, _)) = mtparser.parse("end\n")
}

// ── let ─────────────────────────────────────────────────────────────────────

pub fn let_plain_test() {
  let assert [Let(_, variable, _, Text(value))] = parsed("let $count= 10;\n")
  variable |> should.equal("count")
  helper.to_raw_string(value) |> should.equal("10")
}

pub fn let_dash_and_env_test() {
  let assert [DashCommand(_, _, Let(_, v1, _, Text(val1)))] =
    parsed("--let $1= 0\n")
  v1 |> should.equal("1")
  helper.to_raw_string(val1) |> should.equal("0")
  let assert [Let(_, v2, _, Text(_))] = parsed("let ENV_VARIABLE= bar;\n")
  v2 |> should.equal("ENV_VARIABLE")
}

pub fn let_value_variable_substitution_test() {
  let assert [Let(_, _, _, Text(value))] = parsed("let $x= $a + $b;\n")
  helper.variable_names(value) |> should.equal(["a", "b"])
}

pub fn let_backtick_query_test() {
  let assert [Let(_, _, _, QueryValue(q))] =
    parsed("let $q= `SELECT VERSION()`;\n")
  helper.to_raw_string(q.query) |> should.equal("SELECT VERSION()")
}

pub fn let_query_get_value_test() {
  let assert [Let(_, _, _, QueryGetValue(_, args))] =
    parsed("let $value= query_get_value(SHOW COLUMNS FROM t1, Type, 1);\n")
  args
  |> list.map(helper.to_raw_string)
  |> should.equal(["SHOW COLUMNS FROM t1", "Type", "1"])
}

pub fn let_query_get_value_four_args_test() {
  let assert [Let(_, _, _, QueryGetValue(_, args))] =
    parsed("let $v= query_get_value(SHOW COLUMNS FROM t1, Type, 1, 2);\n")
  args |> list.length |> should.equal(4)
}

pub fn let_escape_test() {
  let assert [Let(_, _, _, Escape(_, characters, string))] =
    parsed("let $var= escape(\\',The 'path separator' in Windows is \\.);\n")
  helper.to_raw_string(characters) |> should.equal("\\'")
  helper.to_raw_string(string)
  |> should.equal("The 'path separator' in Windows is \\.")
}

pub fn let_convert_error_test() {
  let assert [Let(_, _, _, ConvertError(_, arg))] =
    parsed("let $errvar1=convert_error(ER_UNKNOWN_ERROR);\n")
  helper.to_raw_string(arg) |> should.equal("ER_UNKNOWN_ERROR")
  let assert [Let(_, _, _, ConvertError(_, arg2))] =
    parsed("let $errvar2=convert_error(1450);\n")
  helper.to_raw_string(arg2) |> should.equal("1450")
}

pub fn let_missing_equals_test() {
  let assert Error(error.Syntax(_, _)) = mtparser.parse("let $x;\n")
}

// ── list_files family ───────────────────────────────────────────────────────

pub fn list_files_test() {
  let assert [DashCommand(_, _, ListFiles(_, dir, pattern))] =
    parsed("--list_files $MYSQLD_DATADIR/test t1*\n")
  helper.to_raw_string(option.unwrap(dir, []))
  |> should.equal("$MYSQLD_DATADIR/test")
  helper.to_raw_string(option.unwrap(pattern, [])) |> should.equal("t1*")
}

pub fn list_files_no_pattern_test() {
  let assert [ListFiles(_, dir, pattern)] = parsed("list_files $DIR;\n")
  helper.to_raw_string(option.unwrap(dir, [])) |> should.equal("$DIR")
  pattern |> should.equal(option.None)
}

pub fn list_files_write_append_test() {
  let assert [DashCommand(_, _, ListFilesAppendFile(_, file, dir, pattern))] =
    parsed("--list_files_append_file $TMP/filelist $TMP/testdir *.txt\n")
  helper.to_raw_string(file) |> should.equal("$TMP/filelist")
  helper.to_raw_string(dir) |> should.equal("$TMP/testdir")
  helper.to_raw_string(option.unwrap(pattern, [])) |> should.equal("*.txt")
  let assert [ListFilesWriteFile(_, file2, dir2, pattern2)] =
    parsed("list_files_write_file $TMP/fl $TMP/td;\n")
  helper.to_raw_string(file2) |> should.equal("$TMP/fl")
  helper.to_raw_string(dir2) |> should.equal("$TMP/td")
  pattern2 |> should.equal(option.None)
}

// ── mkdir / move_file / output / lowercase_result / ping / query ──────────

pub fn misc_commands_test() {
  let assert [DashCommand(_, _, Mkdir(_, dir))] = parsed("--mkdir testdir\n")
  helper.to_raw_string(dir) |> should.equal("testdir")
  let assert [MoveFile(_, src, dst, retry)] =
    parsed("move_file $TMP/data01 $TMP/test.out;\n")
  helper.to_raw_string(src) |> should.equal("$TMP/data01")
  helper.to_raw_string(dst) |> should.equal("$TMP/test.out")
  retry |> should.equal(option.None)
  let assert [Output(_, file)] = parsed("output $TMP/out-file;\n")
  helper.to_raw_string(file) |> should.equal("$TMP/out-file")
  let assert [DashCommand(_, _, LowercaseResults(_))] =
    parsed("--lowercase_result\n")
  let assert [Ping(_)] = parsed("ping;\n")
  let assert [QueryCommand(_, sql)] = parsed("query SELECT 1;\n")
  helper.to_raw_string(sql) |> should.equal("SELECT 1")
}

// ── perl ────────────────────────────────────────────────────────────────────

pub fn perl_default_terminator_test() {
  let assert [Perl(_, open, close, content)] =
    parsed("perl;\nprint \"This is a test\\n\";\nEOF\n")
  open.name |> should.equal("EOF")
  open.span.len |> should.equal(0)
  close.name |> should.equal("EOF")
  content |> should.equal("print \"This is a test\\n\";")
}

pub fn perl_custom_terminator_test() {
  let assert [DashCommand(_, _, Perl(_, open, close, content))] =
    parsed("--perl END_OF_FILE\nprint \"Another\\n\";\nEND_OF_FILE\n")
  open.name |> should.equal("END_OF_FILE")
  close.name |> should.equal("END_OF_FILE")
  content |> should.equal("print \"Another\\n\";")
}

pub fn perl_unterminated_test() {
  let assert Error(error.UnterminatedBlock("perl", "EOF", _)) =
    mtparser.parse("perl;\ncode\n")
}

// ── query_horizontal / query_vertical / reap / reset_connection ────────────

pub fn query_display_test() {
  let assert [QueryHorizontal(_, sql)] =
    parsed("query_horizontal SELECT PI();\n")
  helper.to_raw_string(sql) |> should.equal("SELECT PI()")
  let assert [QueryVertical(_, sql2)] = parsed("query_vertical SELECT PI();\n")
  helper.to_raw_string(sql2) |> should.equal("SELECT PI()")
  let assert [Reap(_)] = parsed("reap;\n")
  let assert [DashCommand(_, _, ResetConnection(_))] =
    parsed("--reset_connection\n")
}

// ── remove_file / remove_files_wildcard ────────────────────────────────────

pub fn remove_file_test() {
  let assert [RemoveFile(_, file, retry)] =
    parsed("remove_file $MYSQL_TMP_DIR/data01;\n")
  helper.to_raw_string(file) |> should.equal("$MYSQL_TMP_DIR/data01")
  retry |> should.equal(option.None)
  let assert [DashCommand(_, _, RemoveFile(_, file2, retry2))] =
    parsed("--remove_file $D/f 5\n")
  helper.to_raw_string(file2) |> should.equal("$D/f")
  retry2 |> should.equal(option.Some("5"))
}

pub fn remove_files_wildcard_test() {
  let assert [RemoveFilesWildcard(_, dir, pattern, retry)] =
    parsed("remove_files_wildcard $MYSQL_TMP_DIR file*.txt;\n")
  helper.to_raw_string(dir) |> should.equal("$MYSQL_TMP_DIR")
  helper.to_raw_string(pattern) |> should.equal("file*.txt")
  retry |> should.equal(option.None)
}

// ── replace_column ──────────────────────────────────────────────────────────

pub fn replace_column_pairs_test() {
  let assert [ReplaceColumn(_, pairs)] = parsed("replace_column 1 b 2 d;\n")
  pairs
  |> list.map(fn(p) { #(p.0, helper.to_raw_string(p.1)) })
  |> should.equal([#(1, "b"), #(2, "d")])
}

pub fn replace_column_quoted_value_test() {
  // Quoted values may contain spaces; "" is a literal quote.
  let assert [DashCommand(_, _, ReplaceColumn(_, pairs))] =
    parsed("--replace_column 9 #\n")
  pairs |> list.length |> should.equal(1)
  let assert [ReplaceColumn(_, pairs2)] = parsed("replace_column 1 \"a b\";\n")
  let assert Ok(#(_, value)) = list.first(pairs2)
  helper.to_raw_string(value) |> should.equal("a b")
}

pub fn replace_column_empty_value_test() {
  // Regression: an empty quoted value used to be dropped by the tokenizer.
  let assert [DashCommand(_, _, ReplaceColumn(_, pairs))] =
    parsed("--replace_column 1 ''\n")
  pairs
  |> list.map(fn(p) { #(p.0, helper.to_raw_string(p.1)) })
  |> should.equal([#(1, "")])
}

// ── replace_numeric_round / result_format ──────────────────────────────────

pub fn numeric_round_and_format_test() {
  let assert [DashCommand(_, _, ReplaceNumericRound(_, precision))] =
    parsed("--replace_numeric_round 6\n")
  precision |> should.equal(6)
  let assert [DashCommand(_, _, ResultFormat(_, version))] =
    parsed("--result_format 2\n")
  version |> should.equal(2)
  let assert Error(error.Syntax(_, _)) = mtparser.parse("--result_format x\n")
}

// ── replace_regex ───────────────────────────────────────────────────────────

pub fn replace_regex_test() {
  let assert [DashCommand(_, _, ReplaceRegex(_, rules))] =
    parsed("--replace_regex /(strawberry)/raspberry and \\1/\n")
  rules |> list.length |> should.equal(1)
  let assert Ok(rule) = list.first(rules)
  rule.pattern |> should.equal("(strawberry)")
  rule.replacement |> should.equal("raspberry and \\1")
  rule.insensitive |> should.be_false
}

pub fn replace_regex_multiple_insensitive_test() {
  let assert [ReplaceRegex(_, rules)] = parsed("replace_regex /A/B/i /B/C/;\n")
  rules |> list.length |> should.equal(2)
  let assert Ok(r1) = nth_of(rules, 0)
  r1.insensitive |> should.be_true
  let assert Ok(r2) = nth_of(rules, 1)
  r2.pattern |> should.equal("B")
  r2.insensitive |> should.be_false
}

fn nth_of(items: List(a), n: Int) -> Result(a, Nil) {
  case n, items {
    _, [] -> Error(Nil)
    0, [x, ..] -> Ok(x)
    _, [_, ..rest] -> nth_of(rest, n - 1)
  }
}

// ── replace_result ─────────────────────────────────────────────────────────

pub fn replace_result_pairs_test() {
  let assert [DashCommand(_, _, ReplaceResult(_, pairs))] =
    parsed("--replace_result 1024 MAX 3072 MAX\n")
  pairs
  |> list.map(fn(p) { #(helper.to_raw_string(p.0), helper.to_raw_string(p.1)) })
  |> should.equal([#("1024", "MAX"), #("3072", "MAX")])
}

pub fn replace_result_quoted_and_variable_test() {
  let assert [ReplaceResult(_, pairs)] =
    parsed("replace_result $MASTER_MYPORT SOURCE_PORT 'a b' \"c d\";\n")
  let assert Ok(#(from, to)) = list.first(pairs)
  helper.variable_names(from) |> should.equal(["MASTER_MYPORT"])
  helper.to_raw_string(to) |> should.equal("SOURCE_PORT")
  let assert Ok(#(from2, to2)) = nth_of(pairs, 1)
  helper.to_raw_string(from2) |> should.equal("a b")
  helper.to_raw_string(to2) |> should.equal("c d")
}

pub fn replace_result_odd_args_test() {
  let assert Error(error.Syntax(_, _)) =
    mtparser.parse("replace_result only_one;\n")
}

pub fn replace_result_empty_quoted_token_test() {
  // `''` is an empty to-value (real form: mix2.inc). Regression: the
  // tokenizer used to drop empty quoted tokens entirely.
  let assert [DashCommand(_, _, ReplaceResult(_, pairs))] =
    parsed("--replace_result $MYSQLTEST_VARDIR . mysqld.1/data/ ''\n")
  pairs
  |> list.map(fn(p) { #(helper.to_raw_string(p.0), helper.to_raw_string(p.1)) })
  |> should.equal([#("$MYSQLTEST_VARDIR", "."), #("mysqld.1/data/", "")])
}

pub fn replace_result_mid_token_quote_literal_test() {
  // A quote mid-token is a literal character (mysqltest.cc ARG_STRING only
  // opens a quote at the start of an argument), so this is three tokens —
  // an odd count — and an error, not `b''` collapsing into one token.
  let assert Error(error.Syntax(_, _)) =
    mtparser.parse("--replace_result a b'' c\n")
}

// ── final batch ─────────────────────────────────────────────────────────────

pub fn simple_final_commands_test() {
  let assert [DashCommand(_, _, Rmdir(_, dir))] = parsed("--rmdir testdir\n")
  helper.to_raw_string(dir) |> should.equal("testdir")
  let assert [SaveMasterPos(_)] = parsed("save_master_pos;\n")
  let assert [SendQuit(_, conn)] = parsed("send_quit con;\n")
  helper.to_raw_string(conn) |> should.equal("con")
  let assert [SendShutdown(_)] = parsed("send_shutdown;\n")
  let assert [DashCommand(_, _, SortedResult(_))] = parsed("--sorted_result\n")
  let assert [StartTimer(_)] = parsed("start_timer;\n")
  let assert [DashCommand(_, _, VerticalResults(_))] =
    parsed("--vertical_results\n")
  let assert [WaitForSlaveToStop(_)] = parsed("wait_for_slave_to_stop;\n")
}

pub fn send_optional_statement_test() {
  let assert [Send(_, sql)] = parsed("send SELECT 1;\n")
  option.unwrap(sql, "") |> should.equal("SELECT 1")
  let assert [Send(_, sql2)] = parsed("send;\n")
  sql2 |> should.equal(option.None)
  let assert [DashCommand(_, _, SendEval(_, sql3))] =
    parsed("--send_eval $my_stmt\n")
  helper.variable_names(option.unwrap(sql3, [])) |> should.equal(["my_stmt"])
}

pub fn shutdown_server_test() {
  let assert [ShutdownServer(_, t1)] = parsed("shutdown_server;\n")
  t1 |> should.equal(option.None)
  let assert [ShutdownServer(_, t2)] = parsed("shutdown_server 30;\n")
  helper.to_raw_string(option.unwrap(t2, [])) |> should.equal("30")
  // The timeout may be a $variable (no type validation) and may carry a
  // trailing `#` comment (mysqltest.cc check_command_args tolerates one).
  let assert [DashCommand(_, _, ShutdownServer(_, t3))] =
    parsed("--shutdown_server $rpl_shutdown_timeout\n")
  helper.variable_names(option.unwrap(t3, []))
  |> should.equal(["rpl_shutdown_timeout"])
  let assert [ShutdownServer(_, t4)] = parsed("shutdown_server 60 # timeout;\n")
  helper.to_raw_string(option.unwrap(t4, [])) |> should.equal("60")
}

pub fn skip_test() {
  let assert [Skip(_, message)] = parsed("skip value of $var;\n")
  option.unwrap(message, "") |> should.equal("value of $var")
  let assert [Skip(_, message2)] = parsed("skip;\n")
  message2 |> should.equal(option.None)
}

pub fn sleep_test() {
  let assert [DashCommand(_, _, Sleep(_, seconds))] = parsed("--sleep 10\n")
  helper.to_raw_string(seconds) |> should.equal("10")
  let assert [Sleep(_, s2)] = parsed("sleep 0.5;\n")
  helper.to_raw_string(s2) |> should.equal("0.5")
  // The argument may be a $variable — no type validation is performed.
  let assert [DashCommand(_, _, Sleep(_, s3))] = parsed("--sleep $v\n")
  helper.variable_names(s3) |> should.equal(["v"])
  // A non-numeric literal is accepted (lenient: we do no type inference).
  let assert [Sleep(_, s4)] = parsed("sleep abc;\n")
  helper.to_raw_string(s4) |> should.equal("abc")
  let assert Error(error.Syntax(_, _)) = mtparser.parse("sleep;\n")
}

pub fn sorted_int_args_test() {
  let assert [PartiallySortedResult(_, n)] =
    parsed("partially_sorted_result 1;\n")
  n |> should.equal(1)
  let assert [SyncWithMaster(_, off)] = parsed("sync_with_master 0;\n")
  helper.to_raw_string(option.unwrap(off, [])) |> should.equal("0")
  // No-argument form is legal (runtime offset 0) and the offset may be a
  // $variable.
  let assert [SyncWithMaster(_, off2)] = parsed("sync_with_master;\n")
  off2 |> should.equal(option.None)
  let assert [SyncWithMaster(_, off3)] = parsed("sync_with_master $off;\n")
  helper.variable_names(option.unwrap(off3, [])) |> should.equal(["off"])
  let assert Error(error.Syntax(_, _)) =
    mtparser.parse("partially_sorted_result x;\n")
}

pub fn source_test() {
  let assert [DashCommand(_, _, Source(_, file))] =
    parsed("--source include/have_csv.inc\n")
  helper.to_raw_string(file) |> should.equal("include/have_csv.inc")
  let assert [Source(_, file2)] = parsed("source include/varchar.inc;\n")
  helper.to_raw_string(file2) |> should.equal("include/varchar.inc")
}

pub fn sync_slave_test() {
  let assert [SyncSlaveWithMaster(_, conn)] =
    parsed("sync_slave_with_master;\n")
  conn |> should.equal(option.None)
  let assert [SyncSlaveWithMaster(_, conn2)] =
    parsed("sync_slave_with_master conn1;\n")
  helper.to_raw_string(option.unwrap(conn2, [])) |> should.equal("conn1")
}

pub fn while_block_test() {
  let assert [While(_, condition, body, open_brace, close_brace)] =
    parsed("while ($i)\n{\n--echo $i\ndec $i;\n}\n")
  condition
  |> should.equal(Var(variable: VariableRef(span: Span(1, 1, 7, 2), name: "i")))
  body |> list.length |> should.equal(2)
  open_brace.line |> should.equal(2)
  close_brace.len |> should.equal(1)
}

pub fn while_end_terminator_test() {
  let assert [While(_, _, body, _, _)] =
    parsed("while ($i) {\n--echo hi\nend\n")
  body |> list.length |> should.equal(1)
}

pub fn while_unterminated_test() {
  let assert Error(error.UnterminatedFlowControl("while", _)) =
    mtparser.parse("while ($i) {\n--echo hi\n")
}

pub fn write_file_test() {
  let assert [WriteFile(_, filename, open, close, content)] =
    parsed("write_file $MYSQL_TMP_DIR/data01;\nline one\nline two\nEOF\n")
  helper.to_raw_string(filename) |> should.equal("$MYSQL_TMP_DIR/data01")
  open.name |> should.equal("EOF")
  open.span.len |> should.equal(0)
  close.name |> should.equal("EOF")
  content |> should.equal("line one\nline two")
}

pub fn write_file_custom_marker_test() {
  let assert [WriteFile(_, filename, open, _, content)] =
    parsed(
      "write_file $MYSQL_TMP_DIR/data02 END_OF_FILE;\nline one\nEND_OF_FILE\n",
    )
  helper.to_raw_string(filename) |> should.equal("$MYSQL_TMP_DIR/data02")
  open.name |> should.equal("END_OF_FILE")
  content |> should.equal("line one")
}

pub fn write_file_unterminated_test() {
  let assert Error(error.UnterminatedBlock("write_file", "EOF", _)) =
    mtparser.parse("write_file $f;\ncontent\n")
}

// ── require / real_sleep / query_attributes / system ───────────────────────

pub fn require_test() {
  let assert [Require(_, file)] = parsed("require mysql-test/t/foo.require;\n")
  helper.to_raw_string(file) |> should.equal("mysql-test/t/foo.require")
  let assert [DashCommand(_, _, Require(_, file2))] =
    parsed("--require $VARDIR/tmp/x.require\n")
  helper.variable_names(file2) |> should.equal(["VARDIR"])
}

pub fn require_missing_file_test() {
  let assert Error(error.Syntax(_, _)) = mtparser.parse("require;\n")
}

pub fn real_sleep_test() {
  let assert [RealSleep(_, seconds)] = parsed("real_sleep 10;\n")
  helper.to_raw_string(seconds) |> should.equal("10")
  let assert [DashCommand(_, _, RealSleep(_, s2))] =
    parsed("--real_sleep 0.5\n")
  helper.to_raw_string(s2) |> should.equal("0.5")
  // A $variable argument is accepted without type validation.
  let assert [RealSleep(_, s3)] = parsed("real_sleep $delay;\n")
  helper.variable_names(s3) |> should.equal(["delay"])
}

pub fn query_attributes_test() {
  let assert [DashCommand(_, _, QueryAttributes(_, attrs))] =
    parsed("--query_attributes attr1 value1 attr2 value2\n")
  attrs
  |> list.map(fn(p) { #(helper.to_raw_string(p.0), helper.to_raw_string(p.1)) })
  |> should.equal([#("attr1", "value1"), #("attr2", "value2")])
}

pub fn system_test() {
  let assert [System(_, command)] =
    parsed("system rm $MYSQLTEST_VARDIR/tmp/t1;\n")
  helper.to_raw_string(command) |> should.equal("rm $MYSQLTEST_VARDIR/tmp/t1")
}

// ── ps_* / optimizer_trace / skip_if_hypergraph / write_line / evalp ───────

pub fn ps_prepare_test() {
  let assert [DashCommand(_, _, PsPrepare(_, sql))] =
    parsed("--ps_prepare SELECT * FROM t1 WHERE a=?\n")
  helper.to_raw_string(sql) |> should.equal("SELECT * FROM t1 WHERE a=?")
}

pub fn ps_bind_test() {
  let assert [DashCommand(_, _, PsBind(_, params))] =
    parsed("--ps_bind 1 2.5 $three\n")
  params
  |> list.map(helper.to_raw_string)
  |> should.equal(["1", "2.5", "$three"])
}

pub fn ps_execute_close_test() {
  let assert [DashCommand(_, _, PsExecute(_))] = parsed("--ps_execute\n")
  let assert [PsClose(_)] = parsed("ps_close;\n")
}

pub fn optimizer_trace_test() {
  let assert [OptimizerTrace(_)] = parsed("optimizer_trace;\n")
}

pub fn skip_if_hypergraph_test() {
  let assert [DashCommand(_, _, SkipIfHypergraph(_))] =
    parsed("--skip_if_hypergraph\n")
}

pub fn write_line_test() {
  let assert [WriteLine(_, line, file)] =
    parsed("write_line some-text $MYSQL_TMP_DIR/out.txt;\n")
  helper.to_raw_string(line) |> should.equal("some-text")
  helper.to_raw_string(file) |> should.equal("$MYSQL_TMP_DIR/out.txt")
}

pub fn evalp_test() {
  let assert [Evalp(_, stmt)] = parsed("evalp $stmt;\n")
  helper.variable_names(stmt) |> should.equal(["stmt"])
}

// ── inline comments / lenient numeric args (2026-10 batch) ─────────────────

pub fn inline_comment_token_commands_test() {
  // mysqltest.cc tolerates a `#` comment after a complete table-driven
  // argument list (check_command_args / check_eol_junk_line).
  let assert [DashCommand(_, _, Sleep(_, seconds))] =
    parsed("--sleep 10 # secs\n")
  helper.to_raw_string(seconds) |> should.equal("10")
  let assert [DashCommand(_, _, ast.Error(_, codes))] =
    parsed("--error 1064,ER_DUP_ENTRY # expected\n")
  codes
  |> list.map(helper.to_raw_string)
  |> should.equal(["1064", "ER_DUP_ENTRY"])
  let assert [CopyFile(_, _, _, retry)] =
    parsed("copy_file $D/a $D/b # make a copy;\n")
  retry |> should.equal(option.None)
  let assert [Mkdir(_, dir)] = parsed("mkdir $D/tmp # create;\n")
  helper.variable_names(dir) |> should.equal(["D"])
  let assert [PartiallySortedResult(_, n)] =
    parsed("partially_sorted_result 2 # from col 2;\n")
  n |> should.equal(2)
}

pub fn inline_comment_not_cut_for_rest_commands_test() {
  // ARG_REST commands (echo etc.) keep `#` as literal text — mysqltest.cc
  // do_eval never strips comments.
  let assert [DashCommand(_, _, Echo(_, text))] = parsed("--echo a # b\n")
  helper.to_raw_string(text) |> should.equal("a # b")
}

pub fn inline_comment_quoted_hash_test() {
  // A `#` inside a quoted token is literal, not a comment start.
  let assert [CopyFile(_, src, _, _)] = parsed("copy_file \"a#b\" $D/c;\n")
  // ws_tokens keeps the quotes; the point is the `#` survived the cut.
  helper.to_raw_string(src) |> should.equal("\"a#b\"")
}

pub fn replace_regex_variable_test() {
  // The whole rule list may come from a $variable (5.7 do_get_replace_regex:
  // "Allow variable for the *entire* list of replacements").
  let assert [DashCommand(_, _, ReplaceRegexVariable(_, v))] =
    parsed("--replace_regex $_slave_field_result_replace\n")
  helper.variable_names(v) |> should.equal(["_slave_field_result_replace"])
}

pub fn let_convert_error_trailing_comment_test() {
  // Real form (rpl_extra_col_slave.test): text after the closing paren is
  // ignored — mysqltest.cc var_set_convert_error reads only up to the
  // first ')'.
  let assert [DashCommand(_, _, Let(_, name, _, ConvertError(_, arg)))] =
    parsed("--let $slave_sql_errno= convert_error(ER_DUP_FIELDNAME) # 1060\n")
  name |> should.equal("slave_sql_errno")
  helper.to_raw_string(arg) |> should.equal("ER_DUP_FIELDNAME")
}

pub fn let_query_get_value_single_arg_test() {
  // Real form (rpl_assert.inc): fewer than three args occur inside
  // branches that never execute — accept any non-empty count.
  let assert [DashCommand(_, _, Let(_, _, _, QueryGetValue(_, args)))] =
    parsed("--let $r= query_get_value($_substmt)\n")
  args
  |> list.map(helper.to_raw_string)
  |> should.equal(["$_substmt"])
}

pub fn let_unclosed_backtick_test() {
  // Real form (diff_tables.inc): an unclosed backtick query — mysqltest
  // validates only at run time; parsing takes the rest as the query.
  let assert [DashCommand(_, _, Let(_, _, _, QueryValue(query)))] =
    parsed("--let $d= `SET SESSION x = $v\n")
  helper.to_raw_string(query.query) |> should.equal("SET SESSION x = $v")
}

pub fn dash_block_terminator_test() {
  // `-- }` and `-- end` close blocks (mysqltest.cc get_command_type routes
  // any command line starting with `}` to Q_END_BLOCK; `end` is a command).
  let assert [If(_, _, body, _, close)] = parsed("if ($x) {\n--echo hi\n-- }\n")
  body |> list.length |> should.equal(1)
  close.len |> should.equal(1)
  let assert [While(_, _, body2, _, _)] =
    parsed("while ($x) {\n--echo hi\n--end\n")
  body2 |> list.length |> should.equal(1)
}

pub fn dash_block_terminator_stray_test() {
  let assert Error(error.Syntax(_, _)) = mtparser.parse("-- }\n")
  let assert Error(error.Syntax(_, _)) = mtparser.parse("-- end\n")
}

pub fn dash_terminator_not_confused_test() {
  // `-- end_timer` / `-- echo end` are ordinary commands, not terminators.
  let assert [DashCommand(_, _, EndTimer(_))] = parsed("--end_timer\n")
  let assert [DashCommand(_, _, Echo(_, text))] = parsed("-- echo end\n")
  helper.to_raw_string(text) |> should.equal("end")
}

// ── reading-unit residues (2026-10: mid-line resumption) ────────────────────

pub fn single_line_if_test() {
  // `if (…) { body }` on one line: the text after `{` continues as the next
  // reading unit (kill_query_and_diff_master_slave.inc).
  let assert [If(_, _, body, _, close), Toggle(_, _, _)] =
    parsed(
      "if ($debug_lock) { eval SELECT GET_LOCK($debug_lock, 10); }\nenable_result_log;\n",
    )
  body |> list.length |> should.equal(1)
  let assert [Eval(_, sql)] = body
  helper.to_raw_string(sql) |> should.equal("SELECT GET_LOCK($debug_lock, 10)")
  close.line |> should.equal(1)
}

pub fn brace_trailing_comment_close_test() {
  // `} #eof comment` closes the block; the trailing text continues as the
  // next unit — a comment statement (rpl_parallel_recovery_core.test).
  let assert [
    While(_, _, body, _, _),
    Comment(_, text),
    DashCommand(_, _, Echo(_, _)),
  ] =
    parsed(
      "while ($iter)\n{\n--echo x\ndec $iter;\n} #eof of the slave error load\n--echo after\n",
    )
  body |> list.length |> should.equal(2)
  text |> should.equal("eof of the slave error load")
}

pub fn semicolon_brace_close_test() {
  // A `}` residue right after a delimiter closes the block — mysqltest's
  // reading engine resumes mid-line after `;`.
  let assert [While(_, _, body, _, _), DashCommand(_, _, Echo(_, _))] =
    parsed("while ($i)\n{\n--echo x\ndec $i; }\n--echo after\n")
  body |> list.length |> should.equal(2)
}

pub fn two_statements_one_line_test() {
  let assert [Sql(_, a), Sql(_, b)] = parsed("SELECT 1; SELECT 2;\n")
  a |> should.equal("SELECT 1")
  b |> should.equal("SELECT 2")
}

// ── replace_regex lenient forms (mysqltest.cc init_replace_regex) ───────────

pub fn replace_regex_s_prefix_test() {
  // sed-style `s/` prefix: everything before the first '/' is skipped.
  let assert [ReplaceRegex(_, rules)] =
    parsed("replace_regex s/(YES|DISABLED)/yesordisabled/;\n")
  let assert Ok(rule) = list.first(rules)
  rule.pattern |> should.equal("(YES|DISABLED)")
  rule.replacement |> should.equal("yesordisabled")
  rule.insensitive |> should.equal(False)
}

pub fn replace_regex_multichar_flags_test() {
  // Only `i` right after the closing '/' matters; `gso` etc. are junk.
  let assert [DashCommand(_, _, ReplaceRegex(_, rules))] =
    parsed(
      "--replace_regex /.*Last checkpoint at[[:space:]]*([0-9]+).*/\\1/gso\n",
    )
  let assert Ok(rule) = list.first(rules)
  rule.replacement |> should.equal("\\1")
  rule.insensitive |> should.equal(False)
}

pub fn replace_regex_junk_separators_test() {
  // Junk between rules (`, `) and after the last one (trailing `;` on a
  // dash line) is skipped, exactly like init_replace_regex scanning for
  // the next '/'.
  let assert [DashCommand(_, _, ReplaceRegex(_, rules))] =
    parsed("--replace_regex /a/b/i, /c/d/;\n")
  rules |> list.length |> should.equal(2)
  let assert Ok(r1) = list.first(rules)
  r1.insensitive |> should.equal(True)
}

pub fn replace_regex_multiline_test() {
  // A bare command under a custom delimiter reads rules across lines;
  // newlines between rules are junk (show_last_deadlock.inc).
  let assert [Delimiter(_, _), ReplaceRegex(_, rules), Delimiter(_, _)] =
    parsed(
      "delimiter !;\nreplace_regex\n  /TRANSACTIONS.*//\n  /x/y/i\n!\ndelimiter ;!\n",
    )
  rules |> list.length |> should.equal(2)
  let assert Ok(r2) = nth_of(rules, 1)
  r2.pattern |> should.equal("x")
  r2.replacement |> should.equal("y")
  r2.insensitive |> should.equal(True)
}

pub fn replace_regex_clear_form_test() {
  // No argument legally clears the rule set.
  let assert [DashCommand(_, _, ReplaceRegex(_, rules))] =
    parsed("--replace_regex\n")
  rules |> should.equal([])
  // Junk with no rule at all is still an error.
  let assert Error(error.Syntax(_, _)) = mtparser.parse("--replace_regex s\n")
}

// ── let function forms: empty convert_error & escape grammar ───────────────

pub fn convert_error_empty_test() {
  // `convert_error()` — an empty argument evaluates to 0 (9.7's own
  // mysqltest.test uses this form).
  let assert [Let(_, _, _, ConvertError(_, argument))] =
    parsed("let $var=convert_error();\n")
  helper.to_raw_string(argument) |> should.equal("")
}

pub fn escape_close_paren_is_last_test() {
  // mysqltest.cc var_set_escape scans TEXT to the end and backs up one
  // char for the closing paren — so a `)` in first position is an
  // ARGUMENT (mysqltest_escape.test R4.1).
  let assert [DashCommand(_, _, Let(_, _, _, Escape(_, characters, string)))] =
    parsed("--let $x = escape(), hihi (haha))\n")
  helper.to_raw_string(characters) |> should.equal(")")
  helper.to_raw_string(string) |> should.equal(" hihi (haha)")
}

pub fn escape_comma_forms_test() {
  // CHARACTERS: first char may be a comma; TEXT may contain commas
  // (R4.2) and be empty (R3).
  let assert [DashCommand(_, _, Let(_, _, _, Escape(_, characters, string)))] =
    parsed("--let $x = escape(u,,x,)\n")
  helper.to_raw_string(characters) |> should.equal("u")
  helper.to_raw_string(string) |> should.equal(",x,")
  let assert [DashCommand(_, _, Let(_, _, _, Escape(_, characters2, _)))] =
    parsed("--let $x = escape(,,,x,)\n")
  helper.to_raw_string(characters2) |> should.equal(",")
  let assert [DashCommand(_, _, Let(_, _, _, Escape(_, _, string3)))] =
    parsed("--let $x = escape(a,)\n")
  helper.to_raw_string(string3) |> should.equal("")
}

pub fn escape_multichar_and_paren_chars_test() {
  let assert [DashCommand(_, _, Let(_, _, _, Escape(_, characters, string)))] =
    parsed("--let $x = escape(lo,Hello, world)\n")
  helper.to_raw_string(characters) |> should.equal("lo")
  helper.to_raw_string(string) |> should.equal("Hello, world")
  let assert [DashCommand(_, _, Let(_, _, _, Escape(_, characters2, _)))] =
    parsed("--let $x = escape((, hihi (haha))\n")
  helper.to_raw_string(characters2) |> should.equal("(")
}

pub fn escape_unterminated_test() {
  // No closing paren at the end is a real error (R7).
  let assert Error(error.Syntax(_, _)) =
    mtparser.parse("--let $x = escape(a,aa\n")
}

pub fn write_file_quoted_filename_test() {
  // Filenames may be quoted — they can contain spaces and special
  // characters such as `;` (mariabackup/options_check.test).
  let assert [DashCommand(_, _, WriteFile(_, filename, open, _, content))] =
    parsed("--write_file \"$MYSQL_TMP_DIR/a;touch b.qp\"\nfoo\nEOF\n")
  helper.to_raw_string(filename) |> should.equal("$MYSQL_TMP_DIR/a;touch b.qp")
  open.name |> should.equal("EOF")
  content |> should.equal("foo")
}

pub fn write_file_quoted_with_marker_test() {
  // Quoted filename followed by an explicit marker.
  let assert [DashCommand(_, _, AppendFile(_, filename, open, _, _))] =
    parsed("--append_file \"$D/a b\" DONE\nx\nDONE\n")
  helper.to_raw_string(filename) |> should.equal("$D/a b")
  open.name |> should.equal("DONE")
}

// ── variable charset per mysqltest.cc my_isvar (2026-10) ────────────────────

pub fn dash_ends_variable_name_test() {
  // my_isvar = alnum | '_': `-` ENDS the name — `$i-1` is `$i` + literal
  // "-1" (a wrong "i-1" name would break go-to-definition).
  let t = parsed_text("$i-1")
  helper.variable_names(t) |> should.equal(["i"])
  helper.to_raw_string(t) |> should.equal("$i-1")
}

pub fn underscore_and_digits_in_name_test() {
  let t = parsed_text("$_rpl_assert_lbracket + 1")
  helper.variable_names(t) |> should.equal(["_rpl_assert_lbracket"])
}

pub fn replace_result_whole_word_test() {
  // get_string substitutes only whole-word variables; a `$` mid-word is
  // literal text.
  let assert [ReplaceResult(_, pairs)] =
    parsed("replace_result $MASTER_MYPORT abc$var 'a b' 'c d';\n")
  let assert Ok(#(from, to)) = list.first(pairs)
  helper.variable_names(from) |> should.equal(["MASTER_MYPORT"])
  // `abc$var` is NOT a variable — one literal part.
  helper.is_literal(to) |> should.be_true
  helper.to_raw_string(to) |> should.equal("abc$var")
  let assert Ok(#(from2, _)) = nth_of(pairs, 1)
  helper.to_raw_string(from2) |> should.equal("a b")
}

pub fn replace_result_register_word_test() {
  let assert [ReplaceResult(_, pairs)] = parsed("replace_result $1 x;\n")
  let assert Ok(#(from, _)) = list.first(pairs)
  helper.variable_names(from) |> should.equal(["1"])
}
