use crate::ast::MTFile;
use crate::ast::statement::*;

use super::VisitResult;

/// Mutable visitor trait for traversing and modifying the AST.
pub trait MutVisitor {
    fn visit_mt_file_mut(&mut self, file: &mut MTFile) -> VisitResult {
        for stmt in &mut file.statements {
            if self.visit_statement_mut(stmt) == VisitResult::Stop {
                return VisitResult::Stop;
            }
            if self.visit_statement_inner_mut(stmt) == VisitResult::Stop {
                return VisitResult::Stop;
            }
        }
        VisitResult::Continue
    }

    fn visit_statement_mut(&mut self, _stmt: &mut Statement) -> VisitResult {
        VisitResult::Continue
    }

    fn visit_statement_inner_mut(&mut self, stmt: &mut Statement) -> VisitResult {
        match stmt {
            Statement::If(block) => {
                if self.visit_if_mut(block) == VisitResult::Continue {
                    for child in &mut block.body {
                        if self.visit_statement_mut(child) == VisitResult::Stop {
                            return VisitResult::Stop;
                        }
                        if self.visit_statement_inner_mut(child) == VisitResult::Stop {
                            return VisitResult::Stop;
                        }
                    }
                }
            }
            Statement::While(block) => {
                if self.visit_while_mut(block) == VisitResult::Continue {
                    for child in &mut block.body {
                        if self.visit_statement_mut(child) == VisitResult::Stop {
                            return VisitResult::Stop;
                        }
                        if self.visit_statement_inner_mut(child) == VisitResult::Stop {
                            return VisitResult::Stop;
                        }
                    }
                }
            }
            _ => {}
        }
        VisitResult::Continue
    }

    fn visit_if_mut(&mut self, _block: &mut IfBlock) -> VisitResult {
        VisitResult::Continue
    }
    fn visit_while_mut(&mut self, _block: &mut WhileBlock) -> VisitResult {
        VisitResult::Continue
    }
}
