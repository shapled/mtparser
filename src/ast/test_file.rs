use super::Statement;

/// A complete parsed test file (.test or .inc).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFile {
    pub statements: Vec<Statement>,
}

impl TestFile {
    pub fn new(statements: Vec<Statement>) -> Self {
        Self { statements }
    }
}
