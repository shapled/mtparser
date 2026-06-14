/// Source location for error reporting and tooling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub column: u32,
    /// Byte offset into the source.
    pub offset: usize,
    /// Byte length of this node's text.
    pub len: usize,
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {} col {}", self.line, self.column)
    }
}

impl Span {
    pub fn new(line: u32, column: u32, offset: usize, len: usize) -> Self {
        Self {
            line,
            column,
            offset,
            len,
        }
    }

    /// Merge two spans into one covering both.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            line: self.line.min(other.line),
            column: if self.line == other.line {
                self.column.min(other.column)
            } else if self.line < other.line {
                self.column
            } else {
                other.column
            },
            offset: self.offset.min(other.offset),
            len: (self.offset + self.len).max(other.offset + other.len) - self.offset.min(other.offset),
        }
    }

    /// Create a dummy span for testing.
    pub const fn dummy() -> Self {
        Self {
            line: 1,
            column: 1,
            offset: 0,
            len: 0,
        }
    }
}
