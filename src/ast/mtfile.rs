use super::Statement;

/// Parsed result of a .test or .inc file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MTFile {
    pub statements: Vec<Statement>,
}

impl MTFile {
    pub fn new(statements: Vec<Statement>) -> Self {
        Self { statements }
    }
}
