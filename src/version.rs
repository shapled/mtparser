use bitflags::bitflags;

bitflags! {
    /// Target MySQL/MariaDB version(s) for the parser.
    ///
    /// Single versions: `MysqlVersion::V80`
    /// Union of versions: `MysqlVersion::V80 | MysqlVersion::V84`
    /// All MySQL: `MysqlVersion::MySQL`
    /// All MariaDB: `MysqlVersion::MariaDB`
    /// Everything: `MysqlVersion::Compatible` (= `MySQL | MariaDB`)
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct MysqlVersion: u16 {
        // MySQL versions
        const V57 = 1 << 0;
        const V80 = 1 << 1;
        const V84 = 1 << 2;
        const V97 = 1 << 3;

        /// Union of all MySQL versions.
        const MySQL = 0b1111;

        // MariaDB versions
        const MariaDB_1011 = 1 << 4;
        const MariaDB_114  = 1 << 5;
        const MariaDB_118  = 1 << 6;
        const MariaDB_123  = 1 << 7;

        /// Union of all MariaDB versions.
        const MariaDB = 0b11110000;

        /// All MySQL + MariaDB versions.
        const Compatible = 0b11111111;
    }
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
    /// Returns true if **any** version in this set supports the given command.
    /// MariaDB is a superset of MySQL (supports all MySQL commands).
    pub fn has_command(&self, command: &str) -> bool {
        if V57_ONLY_COMMANDS.contains(&command) {
            return self.contains(MysqlVersion::V57) || self.intersects(MysqlVersion::MariaDB);
        }
        if command == "assert" {
            return self.intersects(MysqlVersion::V80 | MysqlVersion::V84 | MysqlVersion::V97)
                || self.contains(MysqlVersion::MariaDB);
        }
        // All other commands exist in every version.
        true
    }

    /// Returns true if **any** version in this set supports the `assert` command (8.0+/MariaDB).
    pub fn has_assert(&self) -> bool {
        self.intersects(MysqlVersion::V80 | MysqlVersion::V84 | MysqlVersion::V97)
            || self.contains(MysqlVersion::MariaDB)
    }

    /// Returns true if **any** version in this set is a MariaDB version.
    pub fn is_mariadb(&self) -> bool {
        self.intersects(MysqlVersion::MariaDB)
    }
}
