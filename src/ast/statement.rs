use crate::ast::commands::*;
use crate::ast::expr::Expr;
use crate::ast::Span;

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
    // Simple commands
    Echo(EchoCmd),
    Let(LetCmd),
    Error(ErrorCmd),
    Source(SourceCmd),
    Skip(SkipCmd),
    Die(DieCmd),
    Exit(ExitCmd),
    Exec(ExecCmd),
    Execw(ExecwCmd),
    ExecInBackground(ExecInBackgroundCmd),
    Sleep(SleepCmd),
    Inc(IncCmd),
    Dec(DecCmd),
    Assert(AssertCmd),
    Expr(ExprCmd),

    // Connection commands
    Connect(ConnectCmd),
    Connection(ConnectionCmd),
    Disconnect(DisconnectCmd),
    ChangeUser(ChangeUserCmd),
    ResetConnection(ResetConnectionCmd),

    // Query commands
    Query(QueryCmd),
    Eval(EvalCmd),
    Send(SendCmd),
    SendEval(SendEvalCmd),
    Reap(ReapCmd),

    // Result modifiers
    HorizontalResults(HorizontalResultsCmd),
    VerticalResults(VerticalResultsCmd),
    ReplaceResult(ReplaceResultCmd),
    ReplaceColumn(ReplaceColumnCmd),
    ReplaceRegex(ReplaceRegexCmd),
    SortedResult(SortedResultCmd),
    PartiallySortedResult(PartiallySortedResultCmd),
    ReplaceNumericRound(ReplaceNumericRoundCmd),

    // Enable/disable toggles
    Toggle(ToggleCmd),

    // Delimiter
    Delimiter(DelimiterCmd),

    // File I/O
    WriteFile(WriteFileCmd),
    AppendFile(AppendFileCmd),
    RemoveFile(RemoveFileCmd),
    RemoveFilesWildcard(RemoveFilesWildcardCmd),
    CopyFile(CopyFileCmd),
    MoveFile(MoveFileCmd),
    Mkdir(MkdirCmd),
    Rmdir(RmdirCmd),
    Chmod(ChmodCmd),
    DiffFiles(DiffFilesCmd),
    FileExists(FileExistsCmd),
    CatFile(CatFileCmd),
    ListFiles(ListFilesCmd),
    Output(OutputCmd),

    // Error

    // Server control
    ShutdownServer(ShutdownServerCmd),
    SendQuit(SendQuitCmd),
    SendShutdown(SendShutdownCmd),

    // Flow control
    If(IfBlock),
    While(WhileBlock),
    End(EndCmd),

    // Block
    Perl(PerlBlock),

    // Comment
    Comment(CommentNode),

    // SQL fallback
    Sql(SqlStatement),

    // Empty line
    Empty,

    // 5.7 only commands
    CharacterSet(CharacterSetCmd),
    System(SystemCmd),
    RealSleep(RealSleepCmd),
    Require(RequireCmd),
    LowercaseResult(LowercaseResultCmd),
    SyncSlaveWithMaster(SyncSlaveWithMasterCmd),
    CopyFilesWildcard(CopyFilesWildcardCmd),
}

impl Statement {
    /// Returns the span of this statement.
    pub fn span(&self) -> Span {
        match self {
            Statement::Echo(c) => c.span,
            Statement::Let(c) => c.span,
            Statement::Error(c) => c.span,
            Statement::Source(c) => c.span,
            Statement::Skip(c) => c.span,
            Statement::Die(c) => c.span,
            Statement::Exit(c) => c.span,
            Statement::Exec(c) => c.span,
            Statement::Execw(c) => c.span,
            Statement::ExecInBackground(c) => c.span,
            Statement::Sleep(c) => c.span,
            Statement::Inc(c) => c.span,
            Statement::Dec(c) => c.span,
            Statement::Assert(c) => c.span,
            Statement::Expr(c) => c.span,
            Statement::Connect(c) => c.span,
            Statement::Connection(c) => c.span,
            Statement::Disconnect(c) => c.span,
            Statement::ChangeUser(c) => c.span,
            Statement::ResetConnection(c) => c.span,
            Statement::Query(c) => c.span,
            Statement::Eval(c) => c.span,
            Statement::Send(c) => c.span,
            Statement::SendEval(c) => c.span,
            Statement::Reap(c) => c.span,
            Statement::HorizontalResults(c) => c.span,
            Statement::VerticalResults(c) => c.span,
            Statement::ReplaceResult(c) => c.span,
            Statement::ReplaceColumn(c) => c.span,
            Statement::ReplaceRegex(c) => c.span,
            Statement::SortedResult(c) => c.span,
            Statement::PartiallySortedResult(c) => c.span,
            Statement::ReplaceNumericRound(c) => c.span,
            Statement::Toggle(c) => c.span,
            Statement::Delimiter(c) => c.span,
            Statement::WriteFile(c) => c.span,
            Statement::AppendFile(c) => c.span,
            Statement::RemoveFile(c) => c.span,
            Statement::RemoveFilesWildcard(c) => c.span,
            Statement::CopyFile(c) => c.span,
            Statement::MoveFile(c) => c.span,
            Statement::Mkdir(c) => c.span,
            Statement::Rmdir(c) => c.span,
            Statement::Chmod(c) => c.span,
            Statement::DiffFiles(c) => c.span,
            Statement::FileExists(c) => c.span,
            Statement::CatFile(c) => c.span,
            Statement::ListFiles(c) => c.span,
            Statement::Output(c) => c.span,
            Statement::ShutdownServer(c) => c.span,
            Statement::SendQuit(c) => c.span,
            Statement::SendShutdown(c) => c.span,
            Statement::If(b) => b.span,
            Statement::While(b) => b.span,
            Statement::End(c) => c.span,
            Statement::Perl(b) => b.span,
            Statement::Comment(c) => c.span,
            Statement::Sql(s) => s.span,
            Statement::Empty => Span::dummy(),
            Statement::CharacterSet(c) => c.span,
            Statement::System(c) => c.span,
            Statement::RealSleep(c) => c.span,
            Statement::Require(c) => c.span,
            Statement::LowercaseResult(c) => c.span,
            Statement::SyncSlaveWithMaster(c) => c.span,
            Statement::CopyFilesWildcard(c) => c.span,
        }
    }
}
