/// MySQL version that the parser targets.
///
/// Different versions have slightly different command sets.
/// 5.7 has the most commands (superset); 8.0+ removed some.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MysqlVersion {
    V57,
    V80,
    V84,
    V97,
}

/// Commands that only exist in MySQL 5.7 (verified against 8.0/8.4/9.7 mysqltest.cc source).
const V57_ONLY_COMMANDS: &[&str] = &[
    "require",
    "system",
    "real_sleep",
    "disable_parsing",
    "enable_parsing",
];

impl MysqlVersion {
    /// Returns true if this version supports the given command.
    pub fn has_command(&self, command: &str) -> bool {
        match self {
            Self::V57 => true,
            _ => !V57_ONLY_COMMANDS.contains(&command),
        }
    }

    /// Returns true if this version supports the `assert` command (8.0+).
    pub fn has_assert(&self) -> bool {
        !matches!(self, Self::V57)
    }
}
