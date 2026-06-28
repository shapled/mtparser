//! AST traversal traits.
//!
//! - [`Visitor`] — immutable traversal with pre/post hooks
//! - [`mut_visitor::MutVisitor`] — mutable traversal for AST transformations
//!
//! Both support flow control via [`VisitResult`]: `Continue`, `Skip`, `Stop`.

pub mod mut_visitor;

use crate::ast::commands::*;
use crate::ast::statement::*;

/// Result of a visit operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitResult {
    /// Continue visiting children.
    Continue,
    /// Skip children of this node.
    Skip,
    /// Stop traversal entirely.
    Stop,
}

/// Visitor trait for traversing the AST.
pub trait Visitor {
    fn visit_statements(&mut self, statements: &[Statement]) -> VisitResult {
        for stmt in statements {
            if self.visit_statement(stmt) == VisitResult::Stop {
                return VisitResult::Stop;
            }
            if self.visit_statement_inner(stmt) == VisitResult::Stop {
                return VisitResult::Stop;
            }
        }
        VisitResult::Continue
    }

    fn visit_statement(&mut self, _stmt: &Statement) -> VisitResult {
        VisitResult::Continue
    }

    fn leave_statement(&mut self, _stmt: &Statement) {}

    fn visit_statement_inner(&mut self, stmt: &Statement) -> VisitResult {
        let result = match stmt {
            Statement::If(block) => self.visit_if_block(block),
            Statement::While(block) => self.visit_while_block(block),
            Statement::Echo(c) => self.visit_echo(c),
            Statement::Let(c) => self.visit_let(c),
            Statement::Error(c) => self.visit_error(c),
            Statement::Source(c) => self.visit_source(c),
            Statement::Skip(c) => self.visit_skip(c),
            Statement::Die(c) => self.visit_die(c),
            Statement::Exit(c) => self.visit_exit(c),
            Statement::Exec(c) => self.visit_exec(c),
            Statement::Execw(c) => self.visit_execw(c),
            Statement::ExecInBackground(c) => self.visit_exec_in_background(c),
            Statement::Sleep(c) => self.visit_sleep(c),
            Statement::Inc(c) => self.visit_inc(c),
            Statement::Dec(c) => self.visit_dec(c),
            Statement::Assert(c) => self.visit_assert(c),
            Statement::Expr(c) => self.visit_expr(c),
            Statement::Connect(c) => self.visit_connect(c),
            Statement::Connection(c) => self.visit_connection(c),
            Statement::Disconnect(c) => self.visit_disconnect(c),
            Statement::ChangeUser(c) => self.visit_change_user(c),
            Statement::ResetConnection(c) => self.visit_reset_connection(c),
            Statement::Query(c) => self.visit_query(c),
            Statement::Eval(c) => self.visit_eval(c),
            Statement::Send(c) => self.visit_send(c),
            Statement::SendEval(c) => self.visit_send_eval(c),
            Statement::Reap(c) => self.visit_reap(c),
            Statement::HorizontalResults(c) => self.visit_horizontal_results(c),
            Statement::VerticalResults(c) => self.visit_vertical_results(c),
            Statement::ReplaceResult(c) => self.visit_replace_result(c),
            Statement::ReplaceColumn(c) => self.visit_replace_column(c),
            Statement::ReplaceRegex(c) => self.visit_replace_regex(c),
            Statement::SortedResult(c) => self.visit_sorted_result(c),
            Statement::PartiallySortedResult(c) => self.visit_partially_sorted_result(c),
            Statement::ReplaceNumericRound(c) => self.visit_replace_numeric_round(c),
            Statement::Toggle(c) => self.visit_toggle(c),
            Statement::Delimiter(c) => self.visit_delimiter(c),
            Statement::WriteFile(c) => self.visit_write_file(c),
            Statement::AppendFile(c) => self.visit_append_file(c),
            Statement::RemoveFile(c) => self.visit_remove_file(c),
            Statement::RemoveFilesWildcard(c) => self.visit_remove_files_wildcard(c),
            Statement::CopyFile(c) => self.visit_copy_file(c),
            Statement::MoveFile(c) => self.visit_move_file(c),
            Statement::Mkdir(c) => self.visit_mkdir(c),
            Statement::Rmdir(c) => self.visit_rmdir(c),
            Statement::Chmod(c) => self.visit_chmod(c),
            Statement::DiffFiles(c) => self.visit_diff_files(c),
            Statement::FileExists(c) => self.visit_file_exists(c),
            Statement::CatFile(c) => self.visit_cat_file(c),
            Statement::ListFiles(c) => self.visit_list_files(c),
            Statement::Output(c) => self.visit_output(c),
            Statement::ShutdownServer(c) => self.visit_shutdown_server(c),
            Statement::SendQuit(c) => self.visit_send_quit(c),
            Statement::SendShutdown(c) => self.visit_send_shutdown(c),
            Statement::End(c) => self.visit_end(c),
            Statement::Perl(b) => self.visit_perl(b),
            Statement::Sql(s) => self.visit_sql(s),
            Statement::Comment(c) => self.visit_comment(c),
            Statement::Empty => VisitResult::Continue,
            // 5.7 only
            Statement::CharacterSet(c) => self.visit_character_set(c),
            Statement::System(c) => self.visit_system(c),
            Statement::RealSleep(c) => self.visit_real_sleep(c),
            Statement::Require(c) => self.visit_require(c),
            Statement::LowercaseResult(c) => self.visit_lowercase_result(c),
            Statement::SyncSlaveWithMaster(c) => self.visit_sync_slave_with_master(c),
            Statement::CopyFilesWildcard(c) => self.visit_copy_files_wildcard(c),
            Statement::QueryVertical(c) => self.visit_query_vertical(c),
            Statement::ResultFormat(c) => self.visit_result_format(c),
            Statement::QueryAttributes(c) => self.visit_query_attributes(c),
            Statement::ListFilesWriteFile(c) => self.visit_list_files_write_file(c),
            Statement::ListFilesAppendFile(c) => self.visit_list_files_append_file(c),
            Statement::ForceRmdir(c) => self.visit_force_rmdir(c),
            Statement::ForceCpdir(c) => self.visit_force_cpdir(c),
            Statement::SaveMasterPos(c) => self.visit_save_master_pos(c),
            Statement::SyncWithMaster(c) => self.visit_sync_with_master(c),
            Statement::WaitForSlaveToStop(c) => self.visit_wait_for_slave_to_stop(c),
            Statement::SkipIfHypergraph(c) => self.visit_skip_if_hypergraph(c),
            Statement::EvalP(c) => self.visit_eval_p(c),
            Statement::WriteLine(c) => self.visit_write_line(c),
            Statement::DirtyClose(c) => self.visit_dirty_close(c),
            Statement::Ping(c) => self.visit_ping(c),
            Statement::PsPrepare(c) => self.visit_ps_prepare(c),
            Statement::PsBind(c) => self.visit_ps_bind(c),
            Statement::PsExecute(c) => self.visit_ps_execute(c),
            Statement::PsClose(c) => self.visit_ps_close(c),
            Statement::OptimizerTrace(c) => self.visit_optimizer_trace(c),
        };
        self.leave_statement(stmt);
        result
    }

    // Default implementations - override as needed
    fn visit_echo(&mut self, _cmd: &EchoCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_let(&mut self, _cmd: &LetCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_error(&mut self, _cmd: &ErrorCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_source(&mut self, _cmd: &SourceCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_skip(&mut self, _cmd: &SkipCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_die(&mut self, _cmd: &DieCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_exit(&mut self, _cmd: &ExitCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_exec(&mut self, _cmd: &ExecCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_execw(&mut self, _cmd: &ExecwCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_exec_in_background(&mut self, _cmd: &ExecInBackgroundCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_sleep(&mut self, _cmd: &SleepCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_inc(&mut self, _cmd: &IncCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_dec(&mut self, _cmd: &DecCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_assert(&mut self, _cmd: &AssertCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_expr(&mut self, _cmd: &ExprCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_connect(&mut self, _cmd: &ConnectCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_connection(&mut self, _cmd: &ConnectionCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_disconnect(&mut self, _cmd: &DisconnectCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_change_user(&mut self, _cmd: &ChangeUserCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_reset_connection(&mut self, _cmd: &ResetConnectionCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_query(&mut self, _cmd: &QueryCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_eval(&mut self, _cmd: &EvalCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_send(&mut self, _cmd: &SendCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_send_eval(&mut self, _cmd: &SendEvalCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_reap(&mut self, _cmd: &ReapCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_horizontal_results(&mut self, _cmd: &HorizontalResultsCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_vertical_results(&mut self, _cmd: &VerticalResultsCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_replace_result(&mut self, _cmd: &ReplaceResultCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_replace_column(&mut self, _cmd: &ReplaceColumnCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_replace_regex(&mut self, _cmd: &ReplaceRegexCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_sorted_result(&mut self, _cmd: &SortedResultCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_partially_sorted_result(&mut self, _cmd: &PartiallySortedResultCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_replace_numeric_round(&mut self, _cmd: &ReplaceNumericRoundCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_toggle(&mut self, _cmd: &ToggleCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_delimiter(&mut self, _cmd: &DelimiterCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_write_file(&mut self, _cmd: &WriteFileCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_append_file(&mut self, _cmd: &AppendFileCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_remove_file(&mut self, _cmd: &RemoveFileCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_remove_files_wildcard(&mut self, _cmd: &RemoveFilesWildcardCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_copy_file(&mut self, _cmd: &CopyFileCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_move_file(&mut self, _cmd: &MoveFileCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_mkdir(&mut self, _cmd: &MkdirCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_rmdir(&mut self, _cmd: &RmdirCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_chmod(&mut self, _cmd: &ChmodCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_diff_files(&mut self, _cmd: &DiffFilesCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_file_exists(&mut self, _cmd: &FileExistsCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_cat_file(&mut self, _cmd: &CatFileCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_list_files(&mut self, _cmd: &ListFilesCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_output(&mut self, _cmd: &OutputCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_shutdown_server(&mut self, _cmd: &ShutdownServerCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_send_quit(&mut self, _cmd: &SendQuitCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_send_shutdown(&mut self, _cmd: &SendShutdownCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_end(&mut self, _cmd: &EndCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_perl(&mut self, _block: &PerlBlock) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_sql(&mut self, _stmt: &SqlStatement) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_comment(&mut self, _comment: &CommentNode) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_character_set(&mut self, _cmd: &CharacterSetCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_system(&mut self, _cmd: &SystemCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_real_sleep(&mut self, _cmd: &RealSleepCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_require(&mut self, _cmd: &RequireCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_lowercase_result(&mut self, _cmd: &LowercaseResultCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_sync_slave_with_master(&mut self, _cmd: &SyncSlaveWithMasterCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_copy_files_wildcard(&mut self, _cmd: &CopyFilesWildcardCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_query_vertical(&mut self, _cmd: &QueryVerticalCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_result_format(&mut self, _cmd: &ResultFormatCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_query_attributes(&mut self, _cmd: &QueryAttributesCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_list_files_write_file(&mut self, _cmd: &ListFilesWriteFileCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_list_files_append_file(&mut self, _cmd: &ListFilesAppendFileCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_force_rmdir(&mut self, _cmd: &ForceRmdirCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_force_cpdir(&mut self, _cmd: &ForceCpdirCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_save_master_pos(&mut self, _cmd: &SaveMasterPosCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_sync_with_master(&mut self, _cmd: &SyncWithMasterCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_wait_for_slave_to_stop(&mut self, _cmd: &WaitForSlaveToStopCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_skip_if_hypergraph(&mut self, _cmd: &SkipIfHypergraphCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_eval_p(&mut self, _cmd: &EvalPCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_write_line(&mut self, _cmd: &WriteLineCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_dirty_close(&mut self, _cmd: &DirtyCloseCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_ping(&mut self, _cmd: &PingCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_ps_prepare(&mut self, _cmd: &PsPrepareCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_ps_bind(&mut self, _cmd: &PsBindCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_ps_execute(&mut self, _cmd: &PsExecuteCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_ps_close(&mut self, _cmd: &PsCloseCmd) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_optimizer_trace(&mut self, _cmd: &OptimizerTraceCmd) -> VisitResult {
        VisitResult::Continue
    }

    fn visit_if_block(&mut self, block: &IfBlock) -> VisitResult {
        if self.visit_if(block) == VisitResult::Continue {
            for child in &block.body {
                if self.visit_statement(child) == VisitResult::Stop {
                    return VisitResult::Stop;
                }
                if self.visit_statement_inner(child) == VisitResult::Stop {
                    return VisitResult::Stop;
                }
            }
        }
        VisitResult::Continue
    }

    fn visit_while_block(&mut self, block: &WhileBlock) -> VisitResult {
        if self.visit_while(block) == VisitResult::Continue {
            for child in &block.body {
                if self.visit_statement(child) == VisitResult::Stop {
                    return VisitResult::Stop;
                }
                if self.visit_statement_inner(child) == VisitResult::Stop {
                    return VisitResult::Stop;
                }
            }
        }
        VisitResult::Continue
    }

    fn visit_if(&mut self, _block: &IfBlock) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_while(&mut self, _block: &WhileBlock) -> VisitResult {
        VisitResult::Continue
    }
}
