//! Command parsers using winnow `dispatch!` + `Caseless`-style matching.
//!
//! Each command has a standalone `parse_cmd_xxx_args(stream: &mut Stream) -> ModalResult<Statement>`
//! parser. Dispatch uses `dispatch!` with `eq_ignore_ascii_case` match guards
//! for case-insensitive command name routing.

use winnow::Parser;
use winnow::combinator::{alt, delimited, opt, preceded, terminated};
use winnow::token::{one_of, take_till, take_while};

use crate::ast::Span;
use crate::ast::commands::*;
use crate::ast::expr::QueryExpr;
use crate::ast::statement::*;
use crate::ast::text::InterpolatedText;
use crate::parser::arg;
use crate::parser::{Stream, range_to_span};

type ModalResult<T> = winnow::error::ModalResult<T>;

/// Create a Cut error with a descriptive message.
fn cut_err_msg(msg: &'static str) -> winnow::error::ErrMode<winnow::error::ContextError> {
    let mut err: winnow::error::ContextError = winnow::error::ContextError::new();
    err.push(winnow::error::StrContext::Expected(
        winnow::error::StrContextValue::Description(msg),
    ));
    winnow::error::ErrMode::Cut(err)
}

// ---------------------------------------------------------------------------
// Command name token + dispatch
// ---------------------------------------------------------------------------

/// Read a command name identifier from the stream.
fn command_name<'s>(s: &mut Stream<'s>) -> ModalResult<&'s str> {
    take_while(1.., ('a'..='z', 'A'..='Z', '0'..='9', '_', '-')).parse_next(s)
}

/// Parse a command from the stream. Does NOT consume `--` prefix.
/// The stream should be positioned at the command name.
/// Works for both `--` prefixed (caller consumes `--` first) and bare commands.
pub(crate) fn parse_command(stream: &mut Stream) -> ModalResult<Statement> {
    let (mut stmt, range) = (|s: &mut Stream| -> ModalResult<Statement> {
        // Skip leading whitespace (bare commands may be indented)
        let _ = arg::ws(s)?;
        // Read command name, lowercase for case-insensitive matching
        let name = command_name.parse_next(s)?.to_ascii_lowercase();

        let stmt = match name.as_str() {
            "echo" => parse_cmd_echo_args(s)?,
            "source" => parse_cmd_source_args(s)?,
            "skip" => parse_cmd_skip_args(s)?,
            "die" => parse_cmd_die_args(s)?,
            "exit" => parse_cmd_exit_args(s)?,
            "exec" => parse_cmd_exec_args(s)?,
            "execw" => parse_cmd_execw_args(s)?,
            "exec_in_background" => parse_cmd_exec_bg_args(s)?,
            "sleep" => parse_cmd_sleep_args(s)?,
            "reap" => parse_cmd_reap_args(s)?,
            "output" => parse_cmd_output_args(s)?,
            "let" => parse_cmd_let_args(s)?,
            "inc" => parse_cmd_inc_args(s)?,
            "dec" => parse_cmd_dec_args(s)?,
            "expr" => parse_cmd_expr_args(s)?,
            "error" => parse_cmd_error_args(s)?,
            "connect" => parse_cmd_connect_args(s)?,
            "connection" => parse_cmd_connection_args(s)?,
            "disconnect" => parse_cmd_disconnect_args(s)?,
            "change_user" => parse_cmd_change_user_args(s)?,
            "reset_connection" => parse_cmd_reset_conn_args(s)?,
            "query" => parse_cmd_query_args(s)?,
            "eval" => parse_cmd_eval_args(s)?,
            "send" => parse_cmd_send_args(s)?,
            "send_eval" => parse_cmd_send_eval_args(s)?,
            "replace_result" => parse_cmd_replace_result_args(s)?,
            "replace_column" => parse_cmd_replace_column_args(s)?,
            "replace_regex" => parse_cmd_replace_regex_args(s)?,
            "sorted_result" => parse_cmd_sorted_result_args(s)?,
            "partially_sorted_result" => parse_cmd_partially_sorted_args(s)?,
            "replace_numeric_round" => parse_cmd_replace_num_round_args(s)?,
            "horizontal_results" | "query_horizontal" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::HorizontalResults(HorizontalResultsCmd {
                    span: range_to_span(s, range),
                })
            }
            "vertical_results" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::VerticalResults(VerticalResultsCmd {
                    span: range_to_span(s, range),
                })
            }
            "delimiter" => parse_cmd_delimiter_args(s)?,
            "write_file" => super::parse_cmd_write_file_args(s)?,
            "append_file" => super::parse_cmd_append_file_args(s)?,
            "perl" => super::parse_cmd_perl_args(s)?,
            "if" => super::parse_cmd_if_args(s, false)?,
            "while" => super::parse_cmd_if_args(s, true)?,
            "remove_file" => parse_cmd_remove_file_args(s)?,
            "remove_files_wildcard" => parse_cmd_remove_files_wildcard_args(s)?,
            "copy_file" => parse_cmd_copy_file_args(s)?,
            "move_file" => parse_cmd_move_file_args(s)?,
            "mkdir" => parse_cmd_mkdir_args(s)?,
            "rmdir" => parse_cmd_rmdir_args(s)?,
            "chmod" => parse_cmd_chmod_args(s)?,
            "diff_files" => parse_cmd_diff_files_args(s)?,
            "file_exists" => parse_cmd_file_exists_args(s)?,
            "cat_file" => parse_cmd_cat_file_args(s)?,
            "list_files" => parse_cmd_list_files_args(s)?,
            "shutdown_server" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::ShutdownServer(ShutdownServerCmd {
                    span: range_to_span(s, range),
                })
            }
            "send_quit" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::SendQuit(SendQuitCmd {
                    span: range_to_span(s, range),
                })
            }
            "send_shutdown" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::SendShutdown(SendShutdownCmd {
                    span: range_to_span(s, range),
                })
            }
            "query_vertical" => parse_cmd_query_vertical_args(s)?,
            "save_master_pos" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::SaveMasterPos(SaveMasterPosCmd {
                    span: range_to_span(s, range),
                })
            }
            "wait_for_slave_to_stop" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::WaitForSlaveToStop(WaitForSlaveToStopCmd {
                    span: range_to_span(s, range),
                })
            }
            "dirty_close" | "force-restart" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::DirtyClose(DirtyCloseCmd {
                    span: range_to_span(s, range),
                })
            }
            "ping" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::Ping(PingCmd {
                    span: range_to_span(s, range),
                })
            }
            "ps_prepare" => parse_cmd_ps_prepare_args(s)?,
            "ps_bind" => parse_cmd_ps_bind_args(s)?,
            "ps_execute" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::PsExecute(PsExecuteCmd {
                    span: range_to_span(s, range),
                })
            }
            "ps_close" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::PsClose(PsCloseCmd {
                    span: range_to_span(s, range),
                })
            }
            "optimizer_trace" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::OptimizerTrace(OptimizerTraceCmd {
                    span: range_to_span(s, range),
                })
            }
            "result_format" => parse_cmd_result_format_args(s)?,
            "query_attributes" => parse_cmd_query_attributes_args(s)?,
            "skip_if_hypergraph" => parse_cmd_skip_if_hypergraph_args(s)?,
            "sync_with_master" => parse_cmd_sync_with_master_args(s)?,
            "evalp" => parse_cmd_evalp_args(s)?,
            "list_files_write_file" => parse_cmd_list_files_write_file_args(s)?,
            "list_files_append_file" => parse_cmd_list_files_append_file_args(s)?,
            "force-rmdir" => parse_cmd_force_rmdir_args(s)?,
            "force-cpdir" => parse_cmd_force_cpdir_args(s)?,
            "write_line" => parse_cmd_write_line_args(s)?,
            "end" => parse_cmd_end_args(s)?,
            "assert" => parse_cmd_assert_args(s)?,
            "character_set" => parse_cmd_character_set_args(s)?,
            "system" => parse_cmd_system_args(s)?,
            "real_sleep" => parse_cmd_real_sleep_args(s)?,
            "require" => parse_cmd_require_args(s)?,
            "lowercase_result" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::LowercaseResult(LowercaseResultCmd {
                    span: range_to_span(s, range),
                })
            }
            "sync_slave_with_master" => {
                let (_, range) = take_till(0.., ['\r', '\n']).with_span().parse_next(s)?;
                Statement::SyncSlaveWithMaster(SyncSlaveWithMasterCmd {
                    span: range_to_span(s, range),
                })
            }
            "copy_files_wildcard" => parse_cmd_copy_files_wildcard_args(s)?,
            // Toggle commands (enable_/disable_) handled in default
            n if n.starts_with("enable_") || n.starts_with("disable_") => {
                parse_cmd_toggle_args(s, &name)?
            }
            _ => {
                let mut err = winnow::error::ContextError::new();
                err.push(winnow::error::StrContext::Label(Box::leak(
                    format!("unknown command '{}'", name).into_boxed_str(),
                )));
                return Err(winnow::error::ErrMode::Backtrack(err));
            }
        };
        let _: &str = take_while(0.., [' ', '\t']).parse_next(s)?;
        let delim = s.state.delimiter.clone();
        let remaining: &str = *s.input;
        if remaining.starts_with(&delim) {
            let _: &str = winnow::token::take(delim.len()).parse_next(s)?;
        }

        Ok(stmt)
    })
    .with_span()
    .parse_next(stream)?;

    stmt.set_span(range_to_span(stream, range));
    Ok(stmt)
}

// ---------------------------------------------------------------------------
// Simple value commands
// ---------------------------------------------------------------------------

fn parse_cmd_echo_args(s: &mut Stream) -> ModalResult<Statement> {
    let (text, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::Echo(EchoCmd {
        span: range_to_span(s, range),
        text,
    }))
}

fn parse_cmd_source_args(s: &mut Stream) -> ModalResult<Statement> {
    let (file, range) = (|s: &mut Stream| -> ModalResult<String> {
        let _ = arg::ws(s)?;
        Ok(arg::arg_token(s).unwrap_or("").to_string())
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::Source(SourceCmd {
        span: range_to_span(s, range),
        file: file.into(),
    }))
}

fn parse_cmd_skip_args(s: &mut Stream) -> ModalResult<Statement> {
    let (msg, range) = preceded(arg::ws, arg::arg_rest_opt)
        .with_span()
        .parse_next(s)?;
    Ok(Statement::Skip(SkipCmd {
        span: range_to_span(s, range),
        message: msg.map(String::from),
    }))
}

fn parse_cmd_die_args(s: &mut Stream) -> ModalResult<Statement> {
    let (msg, range) = preceded(arg::ws, arg::arg_rest_opt)
        .with_span()
        .parse_next(s)?;
    Ok(Statement::Die(DieCmd {
        span: range_to_span(s, range),
        message: msg.map(String::from),
    }))
}

fn parse_cmd_exit_args(s: &mut Stream) -> ModalResult<Statement> {
    let (_, range) = (|_s: &mut Stream| -> ModalResult<()> { Ok(()) })
        .with_span()
        .parse_next(s)?;
    Ok(Statement::Exit(ExitCmd {
        span: range_to_span(s, range),
    }))
}

fn parse_cmd_exec_args(s: &mut Stream) -> ModalResult<Statement> {
    let (cmd, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::Exec(ExecCmd {
        span: range_to_span(s, range),
        command: cmd,
    }))
}

fn parse_cmd_execw_args(s: &mut Stream) -> ModalResult<Statement> {
    let (cmd, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::Execw(ExecwCmd {
        span: range_to_span(s, range),
        command: cmd,
    }))
}

fn parse_cmd_exec_bg_args(s: &mut Stream) -> ModalResult<Statement> {
    let (cmd, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::ExecInBackground(ExecInBackgroundCmd {
        span: range_to_span(s, range),
        command: cmd,
    }))
}

fn parse_cmd_sleep_args(s: &mut Stream) -> ModalResult<Statement> {
    let (sec, range) = preceded(arg::ws, arg::arg_rest_literal)
        .with_span()
        .parse_next(s)?;
    Ok(Statement::Sleep(SleepCmd {
        span: range_to_span(s, range),
        seconds: sec,
    }))
}

fn parse_cmd_reap_args(s: &mut Stream) -> ModalResult<Statement> {
    let (_, range) = (|_s: &mut Stream| -> ModalResult<()> { Ok(()) })
        .with_span()
        .parse_next(s)?;
    Ok(Statement::Reap(ReapCmd {
        span: range_to_span(s, range),
    }))
}

fn parse_cmd_output_args(s: &mut Stream) -> ModalResult<Statement> {
    let (file, range) = (|s: &mut Stream| -> ModalResult<String> {
        let _ = arg::ws(s)?;
        let f = strip_quotes(take_till(0.., ['\r', '\n']).parse_next(s)?.trim_end());
        Ok(f.to_string())
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::Output(OutputCmd {
        span: range_to_span(s, range),
        file: file.into(),
    }))
}

// ---------------------------------------------------------------------------
// Variable commands
// ---------------------------------------------------------------------------

fn parse_cmd_inc_args(s: &mut Stream) -> ModalResult<Statement> {
    let (var, range) = (|s: &mut Stream| -> ModalResult<String> {
        let _ = arg::ws(s)?;
        let var = arg::arg_variable(s).map_err(|_| cut_err_msg("inc requires a variable"))?;
        if var.is_empty() {
            return Err(cut_err_msg("inc requires a variable"));
        }
        Ok(var)
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::Inc(IncCmd {
        span: range_to_span(s, range),
        variable: var,
    }))
}

fn parse_cmd_dec_args(s: &mut Stream) -> ModalResult<Statement> {
    let (var, range) = (|s: &mut Stream| -> ModalResult<String> {
        let _ = arg::ws(s)?;
        let var = arg::arg_variable(s).map_err(|_| cut_err_msg("dec requires a variable"))?;
        if var.is_empty() {
            return Err(cut_err_msg("dec requires a variable"));
        }
        Ok(var)
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::Dec(DecCmd {
        span: range_to_span(s, range),
        variable: var,
    }))
}

fn parse_cmd_expr_args(s: &mut Stream) -> ModalResult<Statement> {
    let ((var, expression), range) = (|s: &mut Stream| -> ModalResult<(String, String)> {
        let _ = arg::ws(s)?;
        let var = arg::arg_variable(s)?;
        let _: ModalResult<Option<char>> = opt(one_of('=')).parse_next(s);
        let _ = arg::ws(s)?;
        let expression = take_till(0.., ['\r', '\n'])
            .parse_next(s)?
            .trim_end()
            .to_string();
        if expression.is_empty() {
            return Err(winnow::error::ErrMode::Cut(
                winnow::error::ContextError::new(),
            ));
        }
        Ok((var, expression))
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::Expr(ExprCmd {
        span: range_to_span(s, range),
        variable: var,
        expression,
    }))
}

// ---------------------------------------------------------------------------
// Assignment
// ---------------------------------------------------------------------------

fn parse_cmd_let_args(s: &mut Stream) -> ModalResult<Statement> {
    let ((variable, value), range) = (|s: &mut Stream| -> ModalResult<(String, LetValue)> {
        let _ = arg::ws(s)?;
        // Variable name: optionally $ or @ prefixed, extends until = or whitespace
        let var = take_till(1.., ['=', ' ', '\t', '\r', '\n']).parse_next(s)?;
        let var = var
            .strip_prefix('$')
            .or_else(|| var.strip_prefix('@'))
            .unwrap_or(var);
        let _ = arg::ws(s)?;
        let _: char = one_of('=').parse_next(s).map_err(
            |_: winnow::error::ErrMode<winnow::error::ContextError>| {
                cut_err_msg("invalid let syntax: expected '='")
            },
        )?;
        let _ = arg::ws(s)?;
        // Value: backtick query or literal
        let value = alt((
            preceded('`', terminated(take_till(0.., ['`']), '`'))
                .map(|q: &str| LetValue::Query(QueryExpr::new(Span::new(0, 0, 0, q.len()), q.to_string()))),
            take_till(0.., ['\r', '\n']).map(|v: &str| LetValue::Literal(v.trim_end().to_string())),
        ))
        .parse_next(s)?;
        Ok((var.to_string(), value))
    })
    .with_span()
    .parse_next(s)?;

    Ok(Statement::Let(LetCmd {
        span: range_to_span(s, range),
        variable,
        value,
    }))
}

// ---------------------------------------------------------------------------
// Comma-separated list commands
// ---------------------------------------------------------------------------

fn parse_cmd_error_args(s: &mut Stream) -> ModalResult<Statement> {
    let (codes, range) = (|s: &mut Stream| -> ModalResult<Vec<InterpolatedText>> {
        let _ = arg::ws(s)?;
        let elems = arg::arg_comma_list(take_till(0.., [',', '\r', '\n'])).parse_next(s)?;
        let codes: Vec<InterpolatedText> = elems
            .into_iter()
            .map(|e| e.trim())
            .filter(|e| !e.is_empty())
            .map(InterpolatedText::from)
            .collect();
        Ok(codes)
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::Error(ErrorCmd {
        span: range_to_span(s, range),
        error_codes: codes,
    }))
}

fn parse_cmd_connect_args(s: &mut Stream) -> ModalResult<Statement> {
    let ((name, params), range) = (|s: &mut Stream| -> ModalResult<(Option<InterpolatedText>, ConnectParams)> {
        let _ = arg::ws(s)?;
        // Try parenthesized form first
        let paren_result: ModalResult<Vec<&str>> = delimited(
            one_of('('),
            arg::arg_comma_list(take_till(0.., [',', ')'])),
            one_of(')'),
        )
        .parse_next(s);

        match paren_result {
            Ok(parts) => {
                let name = parts.first().map(|p| InterpolatedText::from(p.trim()));
                let mut params = ConnectParams::default();
                if let Some(h) = parts.get(1) {
                    params.host = Some(h.trim().into());
                }
                if let Some(u) = parts.get(2) {
                    params.user = Some(u.trim().into());
                }
                if let Some(p) = parts.get(3) {
                    params.password = Some(p.trim().into());
                }
                if let Some(d) = parts.get(4) {
                    params.database = Some(d.trim().into());
                }
                if let Some(p) = parts.get(5) {
                    params.port = Some(p.trim().into());
                }
                if let Some(so) = parts.get(6) {
                    params.socket = Some(so.trim().into());
                }
                Ok((name, params))
            }
            Err(_) => {
                let name = arg::arg_rest(s)?;
                Ok((Some(name), ConnectParams::default()))
            }
        }
    })
    .with_span()
    .parse_next(s)?;

    Ok(Statement::Connect(ConnectCmd {
        span: range_to_span(s, range),
        name,
        params,
    }))
}

fn parse_cmd_connection_args(s: &mut Stream) -> ModalResult<Statement> {
    let (name, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::Connection(ConnectionCmd {
        span: range_to_span(s, range),
        name,
    }))
}

fn parse_cmd_disconnect_args(s: &mut Stream) -> ModalResult<Statement> {
    let (name, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::Disconnect(DisconnectCmd {
        span: range_to_span(s, range),
        name,
    }))
}

fn parse_cmd_change_user_args(s: &mut Stream) -> ModalResult<Statement> {
    let ((user, password, database), range) = (|s: &mut Stream| -> ModalResult<(
        Option<InterpolatedText>,
        Option<InterpolatedText>,
        Option<InterpolatedText>,
    )> {
        let _ = arg::ws(s)?;
        let elems = arg::arg_comma_list(take_till(0.., [',', '\r', '\n'])).parse_next(s)?;
        let get = |i: usize| {
            elems
                .get(i)
                .map(|e| e.trim())
                .filter(|e| !e.is_empty())
                .map(|e| e.into())
        };
        Ok((get(0), get(1), get(2)))
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::ChangeUser(ChangeUserCmd {
        span: range_to_span(s, range),
        user,
        password,
        database,
    }))
}

fn parse_cmd_reset_conn_args(s: &mut Stream) -> ModalResult<Statement> {
    let (_, range) = (|_s: &mut Stream| -> ModalResult<()> { Ok(()) })
        .with_span()
        .parse_next(s)?;
    Ok(Statement::ResetConnection(ResetConnectionCmd {
        span: range_to_span(s, range),
    }))
}

// ---------------------------------------------------------------------------
// SQL commands
// ---------------------------------------------------------------------------

fn parse_cmd_query_args(s: &mut Stream) -> ModalResult<Statement> {
    let (sql, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::Query(QueryCmd {
        span: range_to_span(s, range),
        sql,
    }))
}

fn parse_cmd_eval_args(s: &mut Stream) -> ModalResult<Statement> {
    let (sql, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::Eval(EvalCmd {
        span: range_to_span(s, range),
        sql,
    }))
}

fn parse_cmd_send_args(s: &mut Stream) -> ModalResult<Statement> {
    let (sql, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::Send(SendCmd {
        span: range_to_span(s, range),
        sql,
    }))
}

fn parse_cmd_send_eval_args(s: &mut Stream) -> ModalResult<Statement> {
    let (sql, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::SendEval(SendEvalCmd {
        span: range_to_span(s, range),
        sql,
    }))
}

fn parse_cmd_query_vertical_args(s: &mut Stream) -> ModalResult<Statement> {
    let (sql, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::QueryVertical(QueryVerticalCmd {
        span: range_to_span(s, range),
        sql,
    }))
}

fn parse_cmd_ps_prepare_args(s: &mut Stream) -> ModalResult<Statement> {
    let (sql, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::PsPrepare(PsPrepareCmd {
        span: range_to_span(s, range),
        sql,
    }))
}

fn parse_cmd_ps_bind_args(s: &mut Stream) -> ModalResult<Statement> {
    let (name, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::PsBind(PsBindCmd {
        span: range_to_span(s, range),
        name,
    }))
}

// ---------------------------------------------------------------------------
// Replace commands
// ---------------------------------------------------------------------------

fn parse_cmd_replace_result_args(s: &mut Stream) -> ModalResult<Statement> {
    let (pairs, range) = (|s: &mut Stream| -> ModalResult<Vec<(InterpolatedText, InterpolatedText)>> {
        Ok(arg::arg_kv_pairs(s)?)
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::ReplaceResult(ReplaceResultCmd {
        span: range_to_span(s, range),
        replacements: pairs,
    }))
}

fn parse_cmd_replace_column_args(s: &mut Stream) -> ModalResult<Statement> {
    let (replacements, range) = (|s: &mut Stream| -> ModalResult<Vec<ReplaceColumnItem>> {
        let triples = arg::arg_kv_triples(s)?;
        let replacements = triples
            .into_iter()
            .map(|(c, o, n)| ReplaceColumnItem {
                column: c,
                old_value: o,
                new_value: n,
            })
            .collect();
        Ok(replacements)
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::ReplaceColumn(ReplaceColumnCmd {
        span: range_to_span(s, range),
        replacements,
    }))
}

fn parse_cmd_sorted_result_args(s: &mut Stream) -> ModalResult<Statement> {
    let (_, range) = (|_s: &mut Stream| -> ModalResult<()> { Ok(()) })
        .with_span()
        .parse_next(s)?;
    Ok(Statement::SortedResult(SortedResultCmd {
        span: range_to_span(s, range),
    }))
}

fn parse_cmd_partially_sorted_args(s: &mut Stream) -> ModalResult<Statement> {
    let (cols, range) = preceded(arg::ws, arg::arg_rest_literal)
        .with_span()
        .parse_next(s)?;
    Ok(Statement::PartiallySortedResult(PartiallySortedResultCmd {
        span: range_to_span(s, range),
        columns: cols,
    }))
}

fn parse_cmd_replace_num_round_args(s: &mut Stream) -> ModalResult<Statement> {
    let (dec, range) = preceded(arg::ws, arg::arg_rest_literal)
        .with_span()
        .parse_next(s)?;
    Ok(Statement::ReplaceNumericRound(ReplaceNumericRoundCmd {
        span: range_to_span(s, range),
        decimals: dec,
    }))
}

// ---------------------------------------------------------------------------
// Result/config commands
// ---------------------------------------------------------------------------

fn parse_cmd_result_format_args(s: &mut Stream) -> ModalResult<Statement> {
    let (ver, range) = preceded(arg::ws, arg::arg_rest_literal)
        .with_span()
        .parse_next(s)?;
    Ok(Statement::ResultFormat(ResultFormatCmd {
        span: range_to_span(s, range),
        version: ver,
    }))
}

fn parse_cmd_query_attributes_args(s: &mut Stream) -> ModalResult<Statement> {
    let (attr, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::QueryAttributes(QueryAttributesCmd {
        span: range_to_span(s, range),
        attributes: attr,
    }))
}

fn parse_cmd_skip_if_hypergraph_args(s: &mut Stream) -> ModalResult<Statement> {
    let (msg, range) = preceded(arg::ws, arg::arg_rest_opt)
        .with_span()
        .parse_next(s)?;
    Ok(Statement::SkipIfHypergraph(SkipIfHypergraphCmd {
        span: range_to_span(s, range),
        message: msg,
    }))
}

fn parse_cmd_sync_with_master_args(s: &mut Stream) -> ModalResult<Statement> {
    let (offset, range) = (|s: &mut Stream| -> ModalResult<Option<String>> {
        let _ = arg::ws(s)?;
        let rest = take_till(0.., ['\r', '\n']).parse_next(s)?.trim_end();
        let offset = if rest.is_empty() {
            None
        } else {
            Some(rest.to_string())
        };
        Ok(offset)
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::SyncWithMaster(SyncWithMasterCmd {
        span: range_to_span(s, range),
        offset,
    }))
}

fn parse_cmd_evalp_args(s: &mut Stream) -> ModalResult<Statement> {
    let (sql, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::EvalP(EvalPCmd {
        span: range_to_span(s, range),
        sql,
    }))
}

// ---------------------------------------------------------------------------
// Delimiter
// ---------------------------------------------------------------------------

fn parse_cmd_delimiter_args(s: &mut Stream) -> ModalResult<Statement> {
    let (new_delim, range) = (|s: &mut Stream| -> ModalResult<String> {
        let _ = arg::ws(s)?;
        let new_delim = take_till(0.., ['\r', '\n'])
            .parse_next(s)?
            .trim_end()
            .to_string();
        if new_delim.is_empty() {
            return Err(cut_err_msg("delimiter requires an argument"));
        }
        s.state.delimiter = new_delim.clone();
        Ok(new_delim)
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::Delimiter(DelimiterCmd {
        span: range_to_span(s, range),
        new_delimiter: new_delim,
    }))
}

// ---------------------------------------------------------------------------
// File I/O commands
// ---------------------------------------------------------------------------

fn parse_cmd_remove_file_args(s: &mut Stream) -> ModalResult<Statement> {
    let (tokens, range) = preceded(arg::ws, arg::arg_ws_tokens).with_span().parse_next(s)?;
    Ok(Statement::RemoveFile(RemoveFileCmd {
        span: range_to_span(s, range),
        file: strip_quotes(tokens.first().unwrap_or(&"")).into(),
        timeout: tokens.get(1).map(|t| t.to_string()),
    }))
}

fn parse_cmd_remove_files_wildcard_args(s: &mut Stream) -> ModalResult<Statement> {
    let (tokens, range) = preceded(arg::ws, arg::arg_ws_tokens).with_span().parse_next(s)?;
    Ok(Statement::RemoveFilesWildcard(RemoveFilesWildcardCmd {
        span: range_to_span(s, range),
        dir: strip_quotes(tokens.first().unwrap_or(&"")).into(),
        pattern: tokens.get(1).copied().unwrap_or("").into(),
        timeout: tokens.get(2).map(|t| t.to_string()),
    }))
}

fn parse_cmd_copy_file_args(s: &mut Stream) -> ModalResult<Statement> {
    let (tokens, range) = preceded(arg::ws, arg::arg_ws_tokens).with_span().parse_next(s)?;
    Ok(Statement::CopyFile(CopyFileCmd {
        span: range_to_span(s, range),
        source: strip_quotes(tokens.first().unwrap_or(&"")).into(),
        dest: strip_quotes(tokens.get(1).unwrap_or(&"")).into(),
        retry: tokens.get(2).map(|t| t.to_string()),
    }))
}

fn parse_cmd_move_file_args(s: &mut Stream) -> ModalResult<Statement> {
    let (tokens, range) = preceded(arg::ws, arg::arg_ws_tokens).with_span().parse_next(s)?;
    Ok(Statement::MoveFile(MoveFileCmd {
        span: range_to_span(s, range),
        source: strip_quotes(tokens.first().unwrap_or(&"")).into(),
        dest: strip_quotes(tokens.get(1).unwrap_or(&"")).into(),
        timeout: tokens.get(2).map(|t| t.to_string()),
    }))
}

fn parse_cmd_mkdir_args(s: &mut Stream) -> ModalResult<Statement> {
    let (dir, range) = (|s: &mut Stream| -> ModalResult<String> {
        let _ = arg::ws(s)?;
        Ok(arg::arg_token(s).unwrap_or("").to_string())
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::Mkdir(MkdirCmd {
        span: range_to_span(s, range),
        dir: dir.into(),
    }))
}

fn parse_cmd_rmdir_args(s: &mut Stream) -> ModalResult<Statement> {
    let (dir, range) = (|s: &mut Stream| -> ModalResult<String> {
        let _ = arg::ws(s)?;
        Ok(arg::arg_token(s).unwrap_or("").to_string())
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::Rmdir(RmdirCmd {
        span: range_to_span(s, range),
        dir: dir.into(),
    }))
}

fn parse_cmd_chmod_args(s: &mut Stream) -> ModalResult<Statement> {
    let (tokens, range) = preceded(arg::ws, arg::arg_ws_tokens).with_span().parse_next(s)?;
    Ok(Statement::Chmod(ChmodCmd {
        span: range_to_span(s, range),
        mode: tokens.first().copied().unwrap_or("").to_string(),
        file: strip_quotes(tokens.get(1).unwrap_or(&"")).into(),
    }))
}

fn parse_cmd_diff_files_args(s: &mut Stream) -> ModalResult<Statement> {
    let (tokens, range) = preceded(arg::ws, arg::arg_ws_tokens).with_span().parse_next(s)?;
    Ok(Statement::DiffFiles(DiffFilesCmd {
        span: range_to_span(s, range),
        file1: strip_quotes(tokens.first().unwrap_or(&"")).into(),
        file2: strip_quotes(tokens.get(1).unwrap_or(&"")).into(),
    }))
}

fn parse_cmd_file_exists_args(s: &mut Stream) -> ModalResult<Statement> {
    let (file, range) = (|s: &mut Stream| -> ModalResult<String> {
        let _ = arg::ws(s)?;
        Ok(arg::arg_token(s).unwrap_or("").to_string())
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::FileExists(FileExistsCmd {
        span: range_to_span(s, range),
        file: file.into(),
    }))
}

fn parse_cmd_cat_file_args(s: &mut Stream) -> ModalResult<Statement> {
    let (file, range) = (|s: &mut Stream| -> ModalResult<String> {
        let _ = arg::ws(s)?;
        Ok(arg::arg_token(s).unwrap_or("").to_string())
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::CatFile(CatFileCmd {
        span: range_to_span(s, range),
        file: file.into(),
    }))
}

fn parse_cmd_list_files_args(s: &mut Stream) -> ModalResult<Statement> {
    let (tokens, range) = preceded(arg::ws, arg::arg_ws_tokens).with_span().parse_next(s)?;
    let dir = tokens
        .first()
        .filter(|t| !t.is_empty())
        .map(|t| strip_quotes(t).into());
    let pattern = tokens
        .get(1)
        .filter(|t| !t.is_empty())
        .copied()
        .map(|t| t.into());
    Ok(Statement::ListFiles(ListFilesCmd {
        span: range_to_span(s, range),
        dir,
        pattern,
    }))
}

fn parse_cmd_copy_files_wildcard_args(s: &mut Stream) -> ModalResult<Statement> {
    let (tokens, range) = preceded(arg::ws, arg::arg_ws_tokens).with_span().parse_next(s)?;
    Ok(Statement::CopyFilesWildcard(CopyFilesWildcardCmd {
        span: range_to_span(s, range),
        source: strip_quotes(tokens.first().unwrap_or(&"")).into(),
        dest: strip_quotes(tokens.get(1).unwrap_or(&"")).into(),
        retry: tokens.get(2).map(|t| t.to_string()),
    }))
}

fn parse_cmd_list_files_write_file_args(s: &mut Stream) -> ModalResult<Statement> {
    let (tokens, range) = preceded(arg::ws, arg::arg_ws_tokens).with_span().parse_next(s)?;
    Ok(Statement::ListFilesWriteFile(ListFilesWriteFileCmd {
        span: range_to_span(s, range),
        file: strip_quotes(tokens.first().unwrap_or(&"")).into(),
        dir_pattern: tokens
            .get(1)
            .map(|t| strip_quotes(t).into())
            .unwrap_or_else(|| "*".into()),
    }))
}

fn parse_cmd_list_files_append_file_args(s: &mut Stream) -> ModalResult<Statement> {
    let (tokens, range) = preceded(arg::ws, arg::arg_ws_tokens).with_span().parse_next(s)?;
    Ok(Statement::ListFilesAppendFile(ListFilesAppendFileCmd {
        span: range_to_span(s, range),
        file: strip_quotes(tokens.first().unwrap_or(&"")).into(),
        dir_pattern: tokens
            .get(1)
            .map(|t| strip_quotes(t).into())
            .unwrap_or_else(|| "*".into()),
    }))
}

fn parse_cmd_force_rmdir_args(s: &mut Stream) -> ModalResult<Statement> {
    let (dir, range) = (|s: &mut Stream| -> ModalResult<String> {
        let _ = arg::ws(s)?;
        let dir = strip_quotes(take_till(0.., ['\r', '\n']).parse_next(s)?.trim_end());
        Ok(dir.to_string())
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::ForceRmdir(ForceRmdirCmd {
        span: range_to_span(s, range),
        dir: dir.into(),
    }))
}

fn parse_cmd_force_cpdir_args(s: &mut Stream) -> ModalResult<Statement> {
    let (tokens, range) = preceded(arg::ws, arg::arg_ws_tokens).with_span().parse_next(s)?;
    Ok(Statement::ForceCpdir(ForceCpdirCmd {
        span: range_to_span(s, range),
        source: strip_quotes(tokens.first().unwrap_or(&"")).into(),
        dest: strip_quotes(tokens.get(1).unwrap_or(&"")).into(),
    }))
}

fn parse_cmd_write_line_args(s: &mut Stream) -> ModalResult<Statement> {
    let (tokens, range) = preceded(arg::ws, arg::arg_ws_tokens).with_span().parse_next(s)?;
    Ok(Statement::WriteLine(WriteLineCmd {
        span: range_to_span(s, range),
        text: tokens.first().copied().unwrap_or("").into(),
        file: tokens
            .get(1)
            .map(|t| strip_quotes(t).into())
            .unwrap_or_else(|| "".into()),
    }))
}

// ---------------------------------------------------------------------------
// Special commands
// ---------------------------------------------------------------------------

fn parse_cmd_end_args(s: &mut Stream) -> ModalResult<Statement> {
    let (_, range) = (|_s: &mut Stream| -> ModalResult<()> { Ok(()) })
        .with_span()
        .parse_next(s)?;
    Ok(Statement::End(EndCmd {
        span: range_to_span(s, range),
    }))
}

fn parse_cmd_assert_args(s: &mut Stream) -> ModalResult<Statement> {
    let (sql, range) = (|s: &mut Stream| -> ModalResult<String> {
        if !s.state.version.has_assert() {
            return Err(winnow::error::ErrMode::Cut(
                winnow::error::ContextError::new(),
            ));
        }
        let sql = take_till(0.., ['\r', '\n']).parse_next(s)?.trim_end();
        Ok(sql.to_string())
    })
    .with_span()
    .parse_next(s)?;
    Ok(Statement::Sql(SqlStatement {
        span: range_to_span(s, range),
        sql: sql.into(),
    }))
}

fn parse_cmd_character_set_args(s: &mut Stream) -> ModalResult<Statement> {
    let (cs, range) = preceded(arg::ws, arg::arg_rest_literal)
        .with_span()
        .parse_next(s)?;
    Ok(Statement::CharacterSet(CharacterSetCmd {
        span: range_to_span(s, range),
        charset: cs,
    }))
}

fn parse_cmd_system_args(s: &mut Stream) -> ModalResult<Statement> {
    if !s.state.version.has_command("system") {
        return Err(cut_err_msg(
            "system command is not available in this version",
        ));
    }
    let (cmd, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::System(SystemCmd {
        span: range_to_span(s, range),
        command: cmd,
    }))
}

fn parse_cmd_real_sleep_args(s: &mut Stream) -> ModalResult<Statement> {
    if !s.state.version.has_command("real_sleep") {
        return Err(cut_err_msg(
            "real_sleep command is not available in this version",
        ));
    }
    let (sec, range) = preceded(arg::ws, arg::arg_rest_literal)
        .with_span()
        .parse_next(s)?;
    Ok(Statement::RealSleep(RealSleepCmd {
        span: range_to_span(s, range),
        seconds: sec,
    }))
}

fn parse_cmd_require_args(s: &mut Stream) -> ModalResult<Statement> {
    if !s.state.version.has_command("require") {
        return Err(cut_err_msg(
            "require command is not available in this version",
        ));
    }
    let (file, range) = preceded(arg::ws, arg::arg_rest).with_span().parse_next(s)?;
    Ok(Statement::Require(RequireCmd {
        span: range_to_span(s, range),
        file,
    }))
}

// ---------------------------------------------------------------------------
// Toggle commands
// ---------------------------------------------------------------------------

fn parse_cmd_toggle_args(s: &mut Stream, name: &str) -> ModalResult<Statement> {
    let (enabled, kind_str) = if let Some(k) = name.strip_prefix("enable_") {
        (true, k)
    } else {
        (false, &name[8..]) // "disable_"
    };
    let kind = toggle_kind(kind_str);
    let (once, range) = (|s: &mut Stream| -> ModalResult<bool> {
        let _ = arg::ws(s)?;
        let once = opt(|i: &mut Stream| {
            let rest = take_till(0.., ['\r', '\n']).parse_next(i)?;
            if rest.trim().eq_ignore_ascii_case("ONCE") {
                Ok(())
            } else {
                Err(winnow::error::ErrMode::Backtrack(
                    winnow::error::ContextError::new(),
                ))
            }
        })
        .parse_next(s)?
        .is_some();
        Ok(once)
    })
    .with_span()
    .parse_next(s)?;

    Ok(Statement::Toggle(ToggleCmd {
        span: range_to_span(s, range),
        kind,
        enabled,
        once,
    }))
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
// Regex parsers (hand-written — byte-level escape/delimiter state machines)
// ---------------------------------------------------------------------------

fn parse_cmd_replace_regex_args(s: &mut Stream) -> ModalResult<Statement> {
    let _ = arg::ws(s)?;
    // Read arguments: single-line for `--` commands, multi-line for bare commands
    let first_line = take_till(0.., ['\r', '\n']).parse_next(s)?;
    let trimmed = if first_line.trim().is_empty() && !s.input.is_empty() {
        // Multi-line: args continue on subsequent lines (bare command via sub_stream)
        let rest_str: &str = winnow::token::rest.parse_next(s)?;
        rest_str.trim().to_string()
    } else {
        first_line.trim().to_string()
    };

    // $var form
    if trimmed.starts_with('$') {
        return Ok(Statement::ReplaceRegex(ReplaceRegexCmd {
            span: Span::new(0, 0, 0, trimmed.len()),
            pattern: trimmed,
            replacement: String::new(),
            flags: None,
        }));
    }
    if trimmed.is_empty() {
        return Err(winnow::error::ErrMode::Cut(
            winnow::error::ContextError::new(),
        ));
    }
    if s.state.version.is_mariadb() {
        parse_replace_regex_mariadb(&trimmed)
    } else {
        parse_replace_regex_mysql(&trimmed)
    }
}

fn parse_replace_regex_mysql(trimmed: &str) -> ModalResult<Statement> {
    let mut stream: &str = trimmed;
    let _prefix: &str = alt(("s/", "/")).parse_next(&mut stream).map_err(
        |_: winnow::error::ErrMode<winnow::error::ContextError>| {
            winnow::error::ErrMode::Cut(winnow::error::ContextError::new())
        },
    )?;
    let sep = '/';
    let pattern = scan_until_char(&mut stream, sep).to_string();
    let replacement = scan_until_char(&mut stream, sep).to_string();
    let flags = stream.trim();
    let flags = if flags.is_empty() {
        None
    } else {
        Some(flags.to_string())
    };
    Ok(Statement::ReplaceRegex(ReplaceRegexCmd {
        span: Span::new(0, 0, 0, trimmed.len()),
        pattern,
        replacement,
        flags,
    }))
}

fn parse_replace_regex_mariadb(trimmed: &str) -> ModalResult<Statement> {
    let first = trimmed.as_bytes()[0];
    let (pattern_close, rest_after_pattern) = match first {
        b'(' => (b')', &trimmed[1..]),
        b'[' => (b']', &trimmed[1..]),
        b'{' => (b'}', &trimmed[1..]),
        b'<' => (b'>', &trimmed[1..]),
        _ => (first, &trimmed[1..]),
    };
    let (pattern, rest) = scan_regex_part(rest_after_pattern, pattern_close);
    let rest = rest.trim_start();
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
    let (replacement, after_replacement) =
        scan_regex_part(rest_after_replacement, replacement_close);
    let flags_str = after_replacement.trim_start();
    let flags = if flags_str.starts_with('i') {
        Some("i".to_string())
    } else if !flags_str.is_empty() {
        Some(flags_str.to_string())
    } else {
        None
    };
    Ok(Statement::ReplaceRegex(ReplaceRegexCmd {
        span: Span::new(0, 0, 0, trimmed.len()),
        pattern,
        replacement,
        flags,
    }))
}

/// Scan input until an unescaped occurrence of `delimiter`.
fn scan_regex_part(input: &str, delimiter: u8) -> (String, &str) {
    let mut result = String::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == delimiter {
            i += 1;
            result.push(bytes[i] as char);
        } else if bytes[i] == delimiter {
            return (result, &input[i + 1..]);
        } else {
            result.push(bytes[i] as char);
        }
        i += 1;
    }
    (result, "")
}

/// Scan stream until an unescaped occurrence of `ch`, advancing past it.
fn scan_until_char<'a>(stream: &mut &'a str, ch: char) -> &'a str {
    let bytes = stream.as_bytes();
    let target = ch as u8;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == target {
            i += 2;
            continue;
        }
        if bytes[i] == target {
            let result = &stream[..i];
            *stream = &stream[i + 1..];
            return result;
        }
        i += 1;
    }
    let result = *stream;
    *stream = "";
    result
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

/// Parse a possibly quote-wrapped argument (used by mod.rs for file args).
pub(crate) fn parse_quoted_arg<'s>(input: &mut &'s str) -> winnow::ModalResult<&'s str> {
    let open = one_of(['\'', '"', '`']).parse_next(input)?;
    let content = take_till(1.., [open]).parse_next(input)?;
    one_of([open]).parse_next(input)?;
    Ok(content)
}
