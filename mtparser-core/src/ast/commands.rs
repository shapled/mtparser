//! Command structures for the mysqltest AST.
//!
//! Each `--command_name` maps to a `FooCmd` struct. Fields that may contain
//! `$variable` references use [`InterpolatedText`]
//! instead of `String`. Pure identifier fields (variable names, filenames)
//! use plain `String`.
//!
//! ## Command Categories
//!
//! - **Output**: EchoCmd, SourceCmd
//! - **Flow**: (see Statement enum variants)
//! - **Variables**: LetCmd, IncCmd, DecCmd, ExprCmd
//! - **Connections**: ConnectCmd, ConnectionCmd, DisconnectCmd, ChangeUserCmd, ResetConnectionCmd
//! - **SQL execution**: QueryCmd, EvalCmd, SendCmd, SendEvalCmd, ReapCmd
//! - **Result formatting**: HorizontalResultsCmd, VerticalResultsCmd, SortedResultCmd, ReplaceResultCmd, ReplaceColumnCmd, ReplaceRegexCmd, ReplaceNumericRoundCmd, PartiallySortedResultCmd, LowercaseResultCmd
//! - **Error handling**: ErrorCmd, DieCmd, ExitCmd, SkipCmd
//! - **File I/O**: WriteFileCmd, AppendFileCmd, RemoveFileCmd, RemoveFilesWildcardCmd, CopyFileCmd, MoveFileCmd, MkdirCmd, RmdirCmd, ChmodCmd, DiffFilesCmd, FileExistsCmd, CatFileCmd, ListFilesCmd, CopyFilesWildcardCmd
//! - **Server control**: ShutdownServerCmd, SendQuitCmd, SendShutdownCmd
//! - **Toggles**: ToggleCmd (disable_warnings, enable_query_log, etc.)
//! - **Misc**: SleepCmd, ExecCmd, ExecwCmd, ExecInBackgroundCmd, DelimiterCmd, AssertCmd, CharacterSetCmd, SystemCmd, RealSleepCmd, RequireCmd, SyncSlaveWithMasterCmd, EndCmd

use crate::ast::Span;
use crate::ast::expr::{Expr, QueryExpr};
use crate::ast::text::InterpolatedText;

/// `--echo text` or `echo text;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EchoCmd {
    pub span: Span,
    pub text: InterpolatedText,
}

/// `--let $var=value` or `let $var=value;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LetCmd {
    pub span: Span,
    pub variable: String,
    pub value: LetValue,
}

/// Value in a let assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum LetValue {
    Literal(String),
    Query(QueryExpr),
}

/// `--error ER_CODE` or `error ER_CODE;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ErrorCmd {
    pub span: Span,
    pub error_codes: Vec<InterpolatedText>,
}

/// `--source file` or `source file;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SourceCmd {
    pub span: Span,
    pub file: InterpolatedText,
}

/// `--skip [message]` or `skip [message];`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SkipCmd {
    pub span: Span,
    pub message: Option<String>,
}

/// `--die message` or `die message;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DieCmd {
    pub span: Span,
    pub message: Option<String>,
}

/// `--exit` or `exit;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]

pub struct ExitCmd {
    pub span: Span,
}

/// `--exec command` or `exec command;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ExecCmd {
    pub span: Span,
    pub command: InterpolatedText,
}

/// `--execw command` (wide character exec)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ExecwCmd {
    pub span: Span,
    pub command: InterpolatedText,
}

/// `--exec_in_background command`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ExecInBackgroundCmd {
    pub span: Span,
    pub command: InterpolatedText,
}

/// `--sleep N` or `sleep N;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SleepCmd {
    pub span: Span,
    pub seconds: String,
}

/// `--inc $var` or `inc $var;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct IncCmd {
    pub span: Span,
    pub variable: String,
}

/// `--dec $var` or `dec $var;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DecCmd {
    pub span: Span,
    pub variable: String,
}

/// `--assert expr` (8.0+)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct AssertCmd {
    pub span: Span,
    pub expression: Expr,
}

/// `--connect(name, host, user, pass, db, port, socket, ...)`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ConnectCmd {
    pub span: Span,
    pub name: Option<InterpolatedText>,
    pub params: ConnectParams,
}

/// Connection parameters for connect command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ConnectParams {
    pub host: Option<InterpolatedText>,
    pub user: Option<InterpolatedText>,
    pub password: Option<InterpolatedText>,
    pub database: Option<InterpolatedText>,
    pub port: Option<InterpolatedText>,
    pub socket: Option<InterpolatedText>,
    pub default_charset: Option<InterpolatedText>,
}

/// `--connection name` or `connection name;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ConnectionCmd {
    pub span: Span,
    pub name: InterpolatedText,
}

/// `--disconnect name` or `disconnect name;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DisconnectCmd {
    pub span: Span,
    pub name: InterpolatedText,
}

/// `--change_user [user, pass, db]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ChangeUserCmd {
    pub span: Span,
    pub user: Option<InterpolatedText>,
    pub password: Option<InterpolatedText>,
    pub database: Option<InterpolatedText>,
}

/// `--reset_connection` or `reset_connection;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ResetConnectionCmd {
    pub span: Span,
}

/// `query SQL;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct QueryCmd {
    pub span: Span,
    pub sql: InterpolatedText,
}

/// `eval SQL;` (variable substitution before execution)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EvalCmd {
    pub span: Span,
    pub sql: InterpolatedText,
}

/// `send SQL;` (send without reaping result)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SendCmd {
    pub span: Span,
    pub sql: InterpolatedText,
}

/// `send_eval SQL;` (variable substitution + send)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SendEvalCmd {
    pub span: Span,
    pub sql: InterpolatedText,
}

/// `reap;` (reap result from send)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ReapCmd {
    pub span: Span,
}

/// `--horizontal_results` or `horizontal_results;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct HorizontalResultsCmd {
    pub span: Span,
}

/// `--vertical_results` or `vertical_results;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct VerticalResultsCmd {
    pub span: Span,
}

/// `--replace_result old new [old2 new2 ...]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ReplaceResultCmd {
    pub span: Span,
    pub replacements: Vec<(InterpolatedText, InterpolatedText)>,
}

/// `--replace_column col_num old new [col_num2 old2 new2 ...]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ReplaceColumnCmd {
    pub span: Span,
    pub replacements: Vec<ReplaceColumnItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ReplaceColumnItem {
    pub column: String,
    pub old_value: InterpolatedText,
    pub new_value: InterpolatedText,
}

/// `--replace_regex /pattern/replacement/ [flags]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ReplaceRegexCmd {
    pub span: Span,
    pub pattern: String,
    pub replacement: String,
    pub flags: Option<String>,
}

/// `--sorted_result` or `sorted_result;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SortedResultCmd {
    pub span: Span,
}

/// `--partially_sorted_result columns` (8.0+)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PartiallySortedResultCmd {
    pub span: Span,
    pub columns: String,
}

/// `--replace_numeric_round decimals`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ReplaceNumericRoundCmd {
    pub span: Span,
    pub decimals: String,
}

// --- Enable/Disable commands ---

/// Generic enable/disable command with optional ONCE modifier and error code.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ToggleCmd {
    pub span: Span,
    pub kind: ToggleKind,
    pub enabled: bool,
    pub once: bool,
}

/// What is being toggled.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum ToggleKind {
    Warnings,
    QueryLog,
    ResultLog,
    Info,
    Metadata,
    PsProtocol,
    Reconnect,
    ConnectLog,
    SessionTrackInfo,
    Testcase,
    AbortOnError,
    // 5.7 only
    Parsing,
    AsyncClient,
    // MariaDB only
    PrepareWarnings,
    CursorProtocol,
    NonBlockingApi,
    Ps2Protocol,
    ServiceConnection,
    ViewProtocol,
    ColumnNames,
}

/// `--delimiter new_delimiter` or `delimiter new_delimiter;`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DelimiterCmd {
    pub span: Span,
    pub new_delimiter: String,
}

// --- File I/O ---

/// `--write_file filename END_MARKER ... END_MARKER`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct WriteFileCmd {
    pub span: Span,
    pub filename: InterpolatedText,
    pub end_marker: String,
    pub content: InterpolatedText,
}

/// `--append_file filename END_MARKER ... END_MARKER`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct AppendFileCmd {
    pub span: Span,
    pub filename: InterpolatedText,
    pub end_marker: String,
    pub content: InterpolatedText,
}

/// `--remove_file file [timeout]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RemoveFileCmd {
    pub span: Span,
    pub file: InterpolatedText,
    pub timeout: Option<String>,
}

/// `--remove_files_wildcard dir pattern [timeout]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RemoveFilesWildcardCmd {
    pub span: Span,
    pub dir: InterpolatedText,
    pub pattern: InterpolatedText,
    pub timeout: Option<String>,
}

/// `--copy_file src dest [retry]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CopyFileCmd {
    pub span: Span,
    pub source: InterpolatedText,
    pub dest: InterpolatedText,
    pub retry: Option<String>,
}

/// `--move_file src dest [timeout]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MoveFileCmd {
    pub span: Span,
    pub source: InterpolatedText,
    pub dest: InterpolatedText,
    pub timeout: Option<String>,
}

/// `--mkdir dir`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MkdirCmd {
    pub span: Span,
    pub dir: InterpolatedText,
}

/// `--rmdir dir`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RmdirCmd {
    pub span: Span,
    pub dir: InterpolatedText,
}

/// `--chmod mode file`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ChmodCmd {
    pub span: Span,
    pub mode: String,
    pub file: InterpolatedText,
}

/// `--diff_files file1 file2`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DiffFilesCmd {
    pub span: Span,
    pub file1: InterpolatedText,
    pub file2: InterpolatedText,
}

/// `--file_exists file`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FileExistsCmd {
    pub span: Span,
    pub file: InterpolatedText,
}

/// `--cat_file file`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CatFileCmd {
    pub span: Span,
    pub file: InterpolatedText,
}

/// `--list_files [dir] [pattern]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ListFilesCmd {
    pub span: Span,
    pub dir: Option<InterpolatedText>,
    pub pattern: Option<InterpolatedText>,
}

/// `--output file`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct OutputCmd {
    pub span: Span,
    pub file: InterpolatedText,
}

// --- Server control ---

/// `--shutdown_server`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ShutdownServerCmd {
    pub span: Span,
}

/// `--send_quit`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SendQuitCmd {
    pub span: Span,
}

/// `--send_shutdown`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SendShutdownCmd {
    pub span: Span,
}

/// `--expr $var = expression`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ExprCmd {
    pub span: Span,
    pub variable: String,
    pub expression: String,
}

// --- 5.7 only commands ---

/// `--character_set charset` (5.7 only)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CharacterSetCmd {
    pub span: Span,
    pub charset: String,
}

/// `--system command` (5.7 only)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SystemCmd {
    pub span: Span,
    pub command: InterpolatedText,
}

/// `--real_sleep seconds` (5.7 only)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RealSleepCmd {
    pub span: Span,
    pub seconds: String,
}

/// `--require filename` (5.7 only)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RequireCmd {
    pub span: Span,
    pub file: InterpolatedText,
}

/// `--lowercase_result` (5.7 only)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LowercaseResultCmd {
    pub span: Span,
}

/// `--sync_slave_with_master` (5.7 only)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SyncSlaveWithMasterCmd {
    pub span: Span,
}

/// `--copy_files_wildcard src_pattern dest [retry]` (5.7 only)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CopyFilesWildcardCmd {
    pub span: Span,
    pub source: InterpolatedText,
    pub dest: InterpolatedText,
    pub retry: Option<String>,
}

// --- Additional commands ---

/// `--query_vertical [SQL]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct QueryVerticalCmd {
    pub span: Span,
    pub sql: InterpolatedText,
}

/// `--result_format N`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ResultFormatCmd {
    pub span: Span,
    pub version: String,
}

/// `--query_attributes name value`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct QueryAttributesCmd {
    pub span: Span,
    pub attributes: InterpolatedText,
}

/// `--list_files_write_file file [dir_pattern]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ListFilesWriteFileCmd {
    pub span: Span,
    pub file: InterpolatedText,
    pub dir_pattern: InterpolatedText,
}

/// `--list_files_append_file file [dir_pattern]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ListFilesAppendFileCmd {
    pub span: Span,
    pub file: InterpolatedText,
    pub dir_pattern: InterpolatedText,
}

/// `--force-rmdir dir`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ForceRmdirCmd {
    pub span: Span,
    pub dir: InterpolatedText,
}

/// `--force-cpdir from_dir to_dir`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ForceCpdirCmd {
    pub span: Span,
    pub source: InterpolatedText,
    pub dest: InterpolatedText,
}

/// `--save_master_pos` — no args
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SaveMasterPosCmd {
    pub span: Span,
}

/// `--sync_with_master [offset]` — alias for sync_slave_with_master
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SyncWithMasterCmd {
    pub span: Span,
    pub offset: Option<String>,
}

/// `--wait_for_slave_to_stop`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct WaitForSlaveToStopCmd {
    pub span: Span,
}

/// `--skip_if_hypergraph [message]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SkipIfHypergraphCmd {
    pub span: Span,
    pub message: Option<InterpolatedText>,
}

/// `--evalp SQL` — MariaDB only, execute prepared statement
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EvalPCmd {
    pub span: Span,
    pub sql: InterpolatedText,
}

/// `--write_line text file`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct WriteLineCmd {
    pub span: Span,
    pub text: InterpolatedText,
    pub file: InterpolatedText,
}

/// `--dirty_close`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DirtyCloseCmd {
    pub span: Span,
}

/// `--ping`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PingCmd {
    pub span: Span,
}

// --- MariaDB Prepared Statement commands ---

/// `--PS_prepare stmt` (MariaDB Galera)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PsPrepareCmd {
    pub span: Span,
    pub sql: InterpolatedText,
}

/// `--PS_bind name` (MariaDB Galera)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PsBindCmd {
    pub span: Span,
    pub name: InterpolatedText,
}

/// `--PS_execute` (MariaDB Galera)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PsExecuteCmd {
    pub span: Span,
}

/// `--PS_close` (MariaDB Galera)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PsCloseCmd {
    pub span: Span,
}

/// `--optimizer_trace` (MariaDB)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct OptimizerTraceCmd {
    pub span: Span,
}
