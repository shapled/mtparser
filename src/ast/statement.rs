use crate::ast::Span;
use crate::ast::commands::*;
use crate::ast::expr::Expr;

/// Flow control block: if (expr) { body }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfBlock {
    pub span: Span,
    pub condition: Expr,
    pub body: Vec<Statement>,
}

/// Flow control block: while (expr) { body }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhileBlock {
    pub span: Span,
    pub condition: Expr,
    pub body: Vec<Statement>,
}

/// `end` (end of while loop, 5.7 only syntax)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndCmd {
    pub span: Span,
}

/// Perl block: --perl ... END_PERL
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerlBlock {
    pub span: Span,
    pub end_marker: String,
    pub content: String,
}

/// A comment line (# comment)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentNode {
    pub span: Span,
    pub text: String,
}

/// A SQL statement (unrecognized command text)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlStatement {
    pub span: Span,
    pub sql: String,
}

/// The root AST node: a single mysqltest statement or command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    // ── Output ──────────────────────────────────────────────
    Echo(EchoCmd),
    Output(OutputCmd),

    // ── Variables ───────────────────────────────────────────
    Let(LetCmd),
    Inc(IncCmd),
    Dec(DecCmd),
    Expr(ExprCmd),

    // ── Error handling ──────────────────────────────────────
    Error(ErrorCmd),
    Die(DieCmd),
    Skip(SkipCmd),
    Exit(ExitCmd),
    Assert(AssertCmd),
    Require(RequireCmd),

    // ── Connections ──────────────────────────────────────────
    Connect(ConnectCmd),
    Connection(ConnectionCmd),
    Disconnect(DisconnectCmd),
    ChangeUser(ChangeUserCmd),
    ResetConnection(ResetConnectionCmd),
    DirtyClose(DirtyCloseCmd),
    Ping(PingCmd),

    // ── SQL execution ────────────────────────────────────────
    Query(QueryCmd),
    Eval(EvalCmd),
    Send(SendCmd),
    SendEval(SendEvalCmd),
    Reap(ReapCmd),
    QueryVertical(QueryVerticalCmd),
    QueryAttributes(QueryAttributesCmd),
    EvalP(EvalPCmd),

    // ── Result modifiers (one-shot) ──────────────────────────
    HorizontalResults(HorizontalResultsCmd),
    VerticalResults(VerticalResultsCmd),
    SortedResult(SortedResultCmd),
    PartiallySortedResult(PartiallySortedResultCmd),
    ReplaceResult(ReplaceResultCmd),
    ReplaceColumn(ReplaceColumnCmd),
    ReplaceRegex(ReplaceRegexCmd),
    ReplaceNumericRound(ReplaceNumericRoundCmd),
    LowercaseResult(LowercaseResultCmd),
    ResultFormat(ResultFormatCmd),
    OptimizerTrace(OptimizerTraceCmd),

    // ── Enable/disable toggles ─────────────────────────────
    Toggle(ToggleCmd),

    // ── Delimiter ──────────────────────────────────────────
    Delimiter(DelimiterCmd),

    // ── File I/O ────────────────────────────────────────────
    WriteFile(WriteFileCmd),
    AppendFile(AppendFileCmd),
    RemoveFile(RemoveFileCmd),
    RemoveFilesWildcard(RemoveFilesWildcardCmd),
    CopyFile(CopyFileCmd),
    CopyFilesWildcard(CopyFilesWildcardCmd),
    MoveFile(MoveFileCmd),
    Mkdir(MkdirCmd),
    Rmdir(RmdirCmd),
    ForceRmdir(ForceRmdirCmd),
    ForceCpdir(ForceCpdirCmd),
    Chmod(ChmodCmd),
    DiffFiles(DiffFilesCmd),
    FileExists(FileExistsCmd),
    CatFile(CatFileCmd),
    ListFiles(ListFilesCmd),
    ListFilesWriteFile(ListFilesWriteFileCmd),
    ListFilesAppendFile(ListFilesAppendFileCmd),
    WriteLine(WriteLineCmd),

    // ── Server control ──────────────────────────────────────
    ShutdownServer(ShutdownServerCmd),
    SendQuit(SendQuitCmd),
    SendShutdown(SendShutdownCmd),

    // ── Flow control ───────────────────────────────────────
    If(IfBlock),
    While(WhileBlock),
    End(EndCmd),

    // ── Prepared statements (MariaDB) ────────────────────────
    PsPrepare(PsPrepareCmd),
    PsBind(PsBindCmd),
    PsExecute(PsExecuteCmd),
    PsClose(PsCloseCmd),

    // ── Replication ─────────────────────────────────────────
    SaveMasterPos(SaveMasterPosCmd),
    SyncSlaveWithMaster(SyncSlaveWithMasterCmd),
    SyncWithMaster(SyncWithMasterCmd),
    WaitForSlaveToStop(WaitForSlaveToStopCmd),

    // ── System commands ─────────────────────────────────────
    Exec(ExecCmd),
    Execw(ExecwCmd),
    ExecInBackground(ExecInBackgroundCmd),
    Sleep(SleepCmd),
    RealSleep(RealSleepCmd),
    System(SystemCmd),
    CharacterSet(CharacterSetCmd),

    // ── Source ───────────────────────────────────────────────
    Source(SourceCmd),

    // ── Blocks ──────────────────────────────────────────────
    Perl(PerlBlock),

    // ── Misc ───────────────────────────────────────────────
    SkipIfHypergraph(SkipIfHypergraphCmd),

    // ── Structural ──────────────────────────────────────────
    Comment(CommentNode),
    Sql(SqlStatement),
    Empty,
}

impl Statement {
    /// Returns the span of this statement.
    pub fn span(&self) -> Span {
        match self {
            // Output
            Statement::Echo(c) => c.span,
            Statement::Output(c) => c.span,
            // Variables
            Statement::Let(c) => c.span,
            Statement::Inc(c) => c.span,
            Statement::Dec(c) => c.span,
            Statement::Expr(c) => c.span,
            // Error handling
            Statement::Error(c) => c.span,
            Statement::Die(c) => c.span,
            Statement::Skip(c) => c.span,
            Statement::Exit(c) => c.span,
            Statement::Assert(c) => c.span,
            Statement::Require(c) => c.span,
            // Connections
            Statement::Connect(c) => c.span,
            Statement::Connection(c) => c.span,
            Statement::Disconnect(c) => c.span,
            Statement::ChangeUser(c) => c.span,
            Statement::ResetConnection(c) => c.span,
            Statement::DirtyClose(c) => c.span,
            Statement::Ping(c) => c.span,
            // SQL execution
            Statement::Query(c) => c.span,
            Statement::Eval(c) => c.span,
            Statement::Send(c) => c.span,
            Statement::SendEval(c) => c.span,
            Statement::Reap(c) => c.span,
            Statement::QueryVertical(c) => c.span,
            Statement::QueryAttributes(c) => c.span,
            Statement::EvalP(c) => c.span,
            // Result modifiers
            Statement::HorizontalResults(c) => c.span,
            Statement::VerticalResults(c) => c.span,
            Statement::SortedResult(c) => c.span,
            Statement::PartiallySortedResult(c) => c.span,
            Statement::ReplaceResult(c) => c.span,
            Statement::ReplaceColumn(c) => c.span,
            Statement::ReplaceRegex(c) => c.span,
            Statement::ReplaceNumericRound(c) => c.span,
            Statement::LowercaseResult(c) => c.span,
            Statement::ResultFormat(c) => c.span,
            Statement::OptimizerTrace(c) => c.span,
            // Toggles
            Statement::Toggle(c) => c.span,
            // Delimiter
            Statement::Delimiter(c) => c.span,
            // File I/O
            Statement::WriteFile(c) => c.span,
            Statement::AppendFile(c) => c.span,
            Statement::RemoveFile(c) => c.span,
            Statement::RemoveFilesWildcard(c) => c.span,
            Statement::CopyFile(c) => c.span,
            Statement::CopyFilesWildcard(c) => c.span,
            Statement::MoveFile(c) => c.span,
            Statement::Mkdir(c) => c.span,
            Statement::Rmdir(c) => c.span,
            Statement::ForceRmdir(c) => c.span,
            Statement::ForceCpdir(c) => c.span,
            Statement::Chmod(c) => c.span,
            Statement::DiffFiles(c) => c.span,
            Statement::FileExists(c) => c.span,
            Statement::CatFile(c) => c.span,
            Statement::ListFiles(c) => c.span,
            Statement::ListFilesWriteFile(c) => c.span,
            Statement::ListFilesAppendFile(c) => c.span,
            Statement::WriteLine(c) => c.span,
            // Server control
            Statement::ShutdownServer(c) => c.span,
            Statement::SendQuit(c) => c.span,
            Statement::SendShutdown(c) => c.span,
            // Flow control
            Statement::If(b) => b.span,
            Statement::While(b) => b.span,
            Statement::End(c) => c.span,
            // Prepared statements
            Statement::PsPrepare(c) => c.span,
            Statement::PsBind(c) => c.span,
            Statement::PsExecute(c) => c.span,
            Statement::PsClose(c) => c.span,
            // Replication
            Statement::SaveMasterPos(c) => c.span,
            Statement::SyncSlaveWithMaster(c) => c.span,
            Statement::SyncWithMaster(c) => c.span,
            Statement::WaitForSlaveToStop(c) => c.span,
            // System commands
            Statement::Exec(c) => c.span,
            Statement::Execw(c) => c.span,
            Statement::ExecInBackground(c) => c.span,
            Statement::Sleep(c) => c.span,
            Statement::RealSleep(c) => c.span,
            Statement::System(c) => c.span,
            Statement::CharacterSet(c) => c.span,
            // Source
            Statement::Source(c) => c.span,
            // Blocks
            Statement::Perl(b) => b.span,
            // Misc
            Statement::SkipIfHypergraph(c) => c.span,
            // Structural
            Statement::Comment(c) => c.span,
            Statement::Sql(s) => s.span,
            Statement::Empty => Span::dummy(),
        }
    }
}
