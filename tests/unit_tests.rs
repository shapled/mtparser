use mtparser::ast::*;
use mtparser::ast::text::TextPart;
use mtparser::error::ParseMode;
use mtparser::parser::{parse, ParserConfig};
use mtparser::version::MysqlVersion;
use mtparser::visitor::{VisitResult, Visitor};

fn strict_parse(input: &str) -> TestFile {
    let config = ParserConfig::new(MysqlVersion::V80, ParseMode::Strict);
    parse(input, config).expect("parse failed")
}

// --- Comment tests ---

#[test]
fn test_empty_input() {
    let result = strict_parse("");
    assert!(result.statements.is_empty());
}

#[test]
fn test_comment_only() {
    let result = strict_parse("# this is a comment\n");
    assert_eq!(result.statements.len(), 1);
    assert!(matches!(&result.statements[0], Statement::Comment(c) if c.text == " this is a comment"));
}

#[test]
fn test_indented_comment() {
    let result = strict_parse("  # indented comment\n");
    assert!(matches!(&result.statements[0], Statement::Comment(c) if c.text == " indented comment"));
}

// --- Echo ---

#[test]
fn test_echo_single_line() {
    let result = strict_parse("--echo hello world\n");
    assert_eq!(result.statements.len(), 1);
    match &result.statements[0] {
        Statement::Echo(c) => assert_eq!(c.text.to_raw_string(), "hello world"),
        other => panic!("expected Echo, got {:?}", other),
    }
}

// --- Let ---

#[test]
fn test_let_literal() {
    let result = strict_parse("--let $var = hello\n");
    assert_eq!(result.statements.len(), 1);
    match &result.statements[0] {
        Statement::Let(c) => {
            assert_eq!(c.variable, "var");
            match &c.value {
                LetValue::Literal(s) => assert_eq!(s, "hello"),
                other => panic!("expected Literal, got {:?}", other),
            }
        }
        other => panic!("expected Let, got {:?}", other),
    }
}

#[test]
fn test_let_query() {
    let result = strict_parse("--let $result = `SELECT 1`\n");
    assert_eq!(result.statements.len(), 1);
    match &result.statements[0] {
        Statement::Let(c) => {
            assert_eq!(c.variable, "result");
            match &c.value {
                LetValue::Query(q) => assert_eq!(q.query, "SELECT 1"),
                other => panic!("expected Query, got {:?}", other),
            }
        }
        other => panic!("expected Let, got {:?}", other),
    }
}

// --- Inc/Dec ---

#[test]
fn test_inc() {
    let result = strict_parse("--inc $counter\n");
    match &result.statements[0] {
        Statement::Inc(c) => assert_eq!(c.variable, "counter"),
        other => panic!("expected Inc, got {:?}", other),
    }
}

#[test]
fn test_dec() {
    let result = strict_parse("--dec $counter\n");
    match &result.statements[0] {
        Statement::Dec(c) => assert_eq!(c.variable, "counter"),
        other => panic!("expected Dec, got {:?}", other),
    }
}

// --- Error ---

#[test]
fn test_error_single() {
    let result = strict_parse("--error 1064\nSELECT bad;\n");
    assert_eq!(result.statements.len(), 2);
    match &result.statements[0] {
        Statement::Error(c) => {
            assert_eq!(c.error_codes.iter().map(|e| e.to_raw_string()).collect::<Vec<_>>(), vec!["1064"]);
        }
        other => panic!("expected Error, got {:?}", other),
    }
}

#[test]
fn test_error_multiple() {
    let result = strict_parse("--error 1064, 1146\n");
    match &result.statements[0] {
        Statement::Error(c) => {
            assert_eq!(c.error_codes.iter().map(|e| e.to_raw_string()).collect::<Vec<_>>(), vec!["1064", "1146"]);
        }
        other => panic!("expected Error, got {:?}", other),
    }
}

// --- Source ---

#[test]
fn test_source() {
    let result = strict_parse("--source include/test.inc\n");
    match &result.statements[0] {
        Statement::Source(c) => assert_eq!(c.file.to_raw_string(), "include/test.inc"),
        other => panic!("expected Source, got {:?}", other),
    }
}

// --- Skip/Die/Exit ---

#[test]
fn test_skip_with_message() {
    let result = strict_parse("--skip Test requires InnoDB\n");
    match &result.statements[0] {
        Statement::Skip(c) => {
            assert_eq!(c.message.as_deref(), Some("Test requires InnoDB"));
        }
        other => panic!("expected Skip, got {:?}", other),
    }
}

#[test]
fn test_die_with_message() {
    let result = strict_parse("--die fatal error\n");
    match &result.statements[0] {
        Statement::Die(c) => {
            assert_eq!(c.message.as_deref(), Some("fatal error"));
        }
        other => panic!("expected Die, got {:?}", other),
    }
}

#[test]
fn test_exit() {
    let result = strict_parse("--exit\n");
    assert!(matches!(&result.statements[0], Statement::Exit(_)));
}

// --- Toggle commands ---

#[test]
fn test_disable_warnings() {
    let result = strict_parse("--disable_warnings\n");
    match &result.statements[0] {
        Statement::Toggle(c) => {
            assert!(!c.enabled);
            assert_eq!(c.kind, ToggleKind::Warnings);
        }
        other => panic!("expected Toggle, got {:?}", other),
    }
}

#[test]
fn test_enable_warnings() {
    let result = strict_parse("--enable_warnings\n");
    match &result.statements[0] {
        Statement::Toggle(c) => {
            assert!(c.enabled);
            assert_eq!(c.kind, ToggleKind::Warnings);
        }
        other => panic!("expected Toggle, got {:?}", other),
    }
}

#[test]
fn test_disable_query_log() {
    let result = strict_parse("--disable_query_log\n");
    match &result.statements[0] {
        Statement::Toggle(c) => {
            assert!(!c.enabled);
            assert_eq!(c.kind, ToggleKind::QueryLog);
        }
        other => panic!("expected Toggle, got {:?}", other),
    }
}

// --- Connection ---

#[test]
fn test_connect() {
    let result = strict_parse("--connect(con1, localhost, root, , test)\n");
    match &result.statements[0] {
        Statement::Connect(c) => {
            assert_eq!(c.name.as_ref().map(|m| m.to_raw_string()), Some("con1".to_string()));
            assert_eq!(c.params.host.as_ref().map(|m| m.to_raw_string()), Some("localhost".to_string()));
            assert_eq!(c.params.user.as_ref().map(|m| m.to_raw_string()), Some("root".to_string()));
            assert_eq!(c.params.database.as_ref().map(|m| m.to_raw_string()), Some("test".to_string()));
        }
        other => panic!("expected Connect, got {:?}", other),
    }
}

#[test]
fn test_connection_switch() {
    let result = strict_parse("connection default;\n");
    match &result.statements[0] {
        Statement::Connection(c) => assert_eq!(c.name.to_raw_string(), "default"),
        other => panic!("expected Connection, got {:?}", other),
    }
}

#[test]
fn test_disconnect() {
    let result = strict_parse("disconnect con1;\n");
    match &result.statements[0] {
        Statement::Disconnect(c) => assert_eq!(c.name.to_raw_string(), "con1"),
        other => panic!("expected Disconnect, got {:?}", other),
    }
}

// --- SQL statements ---

#[test]
fn test_simple_sql() {
    let result = strict_parse("SELECT 1;\n");
    match &result.statements[0] {
        Statement::Sql(s) => assert_eq!(s.sql, "SELECT 1"),
        other => panic!("expected Sql, got {:?}", other),
    }
}

#[test]
fn test_multi_line_sql() {
    let result = strict_parse("SELECT a, b\n  FROM t1\n  WHERE a > 1;\n");
    match &result.statements[0] {
        Statement::Sql(s) => assert_eq!(s.sql, "SELECT a, b\n  FROM t1\n  WHERE a > 1"),
        other => panic!("expected Sql, got {:?}", other),
    }
}

// --- Delimiter ---

#[test]
fn test_delimiter_change() {
    let result = strict_parse("SELECT 1;\ndelimiter ||\nCREATE PROCEDURE p1()\nBEGIN\n  SELECT 1;\nEND||\ndelimiter ;\nSELECT 2;\n");
    // Should have 5 statements: SELECT 1, Delimiter, CREATE PROCEDURE, Delimiter, SELECT 2
    assert_eq!(result.statements.len(), 5);
    assert!(matches!(&result.statements[0], Statement::Sql(_)));
    assert!(matches!(&result.statements[1], Statement::Delimiter(_)));
    assert!(matches!(&result.statements[2], Statement::Sql(_)));
    assert!(matches!(&result.statements[3], Statement::Delimiter(_)));
    assert!(matches!(&result.statements[4], Statement::Sql(_)));

    // The CREATE PROCEDURE should include the content between ||
    if let Statement::Sql(s) = &result.statements[2] {
        assert!(s.sql.contains("CREATE PROCEDURE"));
        assert!(s.sql.contains("BEGIN"));
        assert!(s.sql.contains("END"));
    }
}

// --- If block ---

#[test]
fn test_if_with_query() {
    let result = strict_parse("--echo before\n--if (`SELECT 1 = 1`)\n{\n  --echo inside\n}\n--echo after\n");
    assert_eq!(result.statements.len(), 3);
    assert!(matches!(&result.statements[0], Statement::Echo(_))); // before
    match &result.statements[1] {
        Statement::If(b) => {
            assert!(matches!(&b.condition, Expr::Query(_)));
            assert_eq!(b.body.len(), 1);
            assert!(matches!(&b.body[0], Statement::Echo(_)));
        }
        other => panic!("expected If, got {:?}", other),
    }
    assert!(matches!(&result.statements[2], Statement::Echo(_))); // after
}

#[test]
fn test_if_negated_variable() {
    let result = strict_parse("--if (!$undefined)\n{\n  --echo var is not set\n}\n");
    match &result.statements[0] {
        Statement::If(b) => {
            assert!(matches!(&b.condition, Expr::NegatedVariable(_)));
        }
        other => panic!("expected If, got {:?}", other),
    }
}

// --- While block ---

#[test]
fn test_while_with_variable() {
    let result = strict_parse("--while ($counter)\n{\n  --dec $counter\n}\n");
    match &result.statements[0] {
        Statement::While(b) => {
            assert!(matches!(&b.condition, Expr::Variable(v) if v.name == "counter"));
            assert_eq!(b.body.len(), 1);
            assert!(matches!(&b.body[0], Statement::Dec(_)));
        }
        other => panic!("expected While, got {:?}", other),
    }
}

// --- Mixed file parsing ---

#[test]
fn test_basic_commands_fixture() {
    let input = std::fs::read_to_string("tests/fixtures/basic_commands.test")
        .expect("failed to read fixture");
    let result = strict_parse(&input);
    // Just verify it parses without error and has a reasonable number of statements
    assert!(result.statements.len() > 10);
}

#[test]
fn test_sql_statements_fixture() {
    let input = std::fs::read_to_string("tests/fixtures/sql_statements.test")
        .expect("failed to read fixture");
    let result = strict_parse(&input);
    assert!(result.statements.len() > 5);
    // All non-empty, non-comment statements should be SQL
    let sql_count = result.statements.iter().filter(|s| matches!(s, Statement::Sql(_))).count();
    assert!(sql_count >= 5, "expected at least 5 SQL statements, got {}", sql_count);
}

#[test]
fn test_flow_control_fixture() {
    let input = std::fs::read_to_string("tests/fixtures/flow_control.test")
        .expect("failed to read fixture");
    let result = strict_parse(&input);
    let if_count = result.statements.iter().filter(|s| matches!(s, Statement::If(_))).count();
    let while_count = result.statements.iter().filter(|s| matches!(s, Statement::While(_))).count();
    assert!(if_count >= 3, "expected at least 3 if blocks, got {}", if_count);
    assert!(while_count >= 1, "expected at least 1 while block, got {}", while_count);
}

#[test]
fn test_connection_fixture() {
    let input = std::fs::read_to_string("tests/fixtures/connection.test")
        .expect("failed to read fixture");
    let result = strict_parse(&input);
    let connect_count = result.statements.iter().filter(|s| matches!(s, Statement::Connect(_))).count();
    assert!(connect_count >= 2, "expected at least 2 connect commands, got {}", connect_count);
}

#[test]
fn test_delimiter_fixture() {
    let input = std::fs::read_to_string("tests/fixtures/delimiter.test")
        .expect("failed to read fixture");
    let result = strict_parse(&input);
    let delim_count = result.statements.iter().filter(|s| matches!(s, Statement::Delimiter(_))).count();
    assert_eq!(delim_count, 2, "expected 2 delimiter commands, got {}", delim_count);
}

#[test]
fn test_write_file_fixture() {
    let input = std::fs::read_to_string("tests/fixtures/write_file.test")
        .expect("failed to read fixture");
    let result = strict_parse(&input);
    assert!(result.statements.iter().any(|s| matches!(s, Statement::WriteFile(_))));
    assert!(result.statements.iter().any(|s| matches!(s, Statement::AppendFile(_))));
}

// --- Lenient mode ---

#[test]
fn test_unknown_command_treated_as_sql() {
    // Unknown commands are treated as SQL (mysqltest behavior)
    let result = strict_parse("unknown_command arg1 arg2;\n");
    assert!(matches!(&result.statements[0], Statement::Sql(_)));
}

// --- Version tests ---

#[test]
fn test_v57_system_command() {
    let config = ParserConfig::new(MysqlVersion::V57, ParseMode::Strict);
    let result = parse("--system ls -la\n", config).expect("parse failed");
    assert!(matches!(&result.statements[0], Statement::System(_)));
}

#[test]
fn test_v80_system_command_fails() {
    let config = ParserConfig::new(MysqlVersion::V80, ParseMode::Strict);
    let result = parse("--system ls -la\n", config);
    assert!(result.is_err(), "system command should fail in V80");
}

// --- File I/O commands ---

#[test]
fn test_mkdir() {
    let result = strict_parse("--mkdir /tmp/test_dir\n");
    match &result.statements[0] {
        Statement::Mkdir(c) => assert_eq!(c.dir.to_raw_string(), "/tmp/test_dir"),
        other => panic!("expected Mkdir, got {:?}", other),
    }
}

#[test]
fn test_remove_file() {
    let result = strict_parse("--remove_file /tmp/test.txt\n");
    match &result.statements[0] {
        Statement::RemoveFile(c) => assert_eq!(c.file.to_raw_string(), "/tmp/test.txt"),
        other => panic!("expected RemoveFile, got {:?}", other),
    }
}

// --- Replace commands ---

#[test]
fn test_replace_column() {
    let result = strict_parse("--replace_column 1 old new\n");
    match &result.statements[0] {
        Statement::ReplaceColumn(c) => {
            assert_eq!(c.replacements.len(), 1);
            assert_eq!(c.replacements[0].column, "1");
            assert_eq!(c.replacements[0].old_value.to_raw_string(), "old");
            assert_eq!(c.replacements[0].new_value.to_raw_string(), "new");
        }
        other => panic!("expected ReplaceColumn, got {:?}", other),
    }
}

#[test]
fn test_replace_regex() {
    let result = strict_parse("--replace_regex /pattern/replacement/i\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "pattern");
            assert_eq!(c.replacement, "replacement");
            assert_eq!(c.flags.as_deref(), Some("i"));
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

// --- Exec ---

#[test]
fn test_exec() {
    let result = strict_parse("--exec ls -la /tmp\n");
    match &result.statements[0] {
        Statement::Exec(c) => assert_eq!(c.command.to_raw_string(), "ls -la /tmp"),
        other => panic!("expected Exec, got {:?}", other),
    }
}

// --- Sleep ---

#[test]
fn test_sleep() {
    let result = strict_parse("--sleep 2\n");
    match &result.statements[0] {
        Statement::Sleep(c) => assert_eq!(c.seconds, "2"),
        other => panic!("expected Sleep, got {:?}", other),
    }
}

// --- Reap ---

#[test]
fn test_reap() {
    let result = strict_parse("reap;\n");
    assert!(matches!(&result.statements[0], Statement::Reap(_)));
}

// --- Server control ---

#[test]
fn test_shutdown_server() {
    let result = strict_parse("--shutdown_server\n");
    assert!(matches!(&result.statements[0], Statement::ShutdownServer(_)));
}

// --- Empty lines ---

#[test]
fn test_multiple_empty_lines() {
    let result = strict_parse("\n\n\n");
    assert_eq!(result.statements.len(), 3);
    assert!(result.statements.iter().all(|s| matches!(s, Statement::Empty)));
}

// --- InterpolatedText variable parsing ---

#[test]
fn test_echo_with_variable() {
    let result = strict_parse("--echo counter is $counter\n");
    match &result.statements[0] {
        Statement::Echo(c) => {
            assert_eq!(c.text.parts().len(), 2);
            assert!(matches!(&c.text.parts()[0], TextPart::Literal(s) if s == "counter is "));
            assert!(matches!(&c.text.parts()[1], TextPart::Variable(v) if v == "counter"));
            assert_eq!(c.text.to_raw_string(), "counter is $counter");
        }
        other => panic!("expected Echo, got {:?}", other),
    }
}

#[test]
fn test_sql_preserves_dollar_var() {
    let result = strict_parse("SELECT $col FROM $table;\n");
    match &result.statements[0] {
        Statement::Sql(s) => {
            assert_eq!(s.sql, "SELECT $col FROM $table");
        }
        other => panic!("expected Sql, got {:?}", other),
    }
}

#[test]
fn test_source_with_variable() {
    let result = strict_parse("--source include/$test.inc\n");
    match &result.statements[0] {
        Statement::Source(c) => {
            assert_eq!(c.file.variable_names(), vec!["test"]);
            assert_eq!(c.file.to_raw_string(), "include/$test.inc");
        }
        other => panic!("expected Source, got {:?}", other),
    }
}

#[test]
fn test_echo_escaped_dollar() {
    let result = strict_parse("--echo price is \\$10\n");
    match &result.statements[0] {
        Statement::Echo(c) => {
            assert!(c.text.is_literal());
            assert_eq!(c.text.to_raw_string(), "price is $10");
        }
        other => panic!("expected Echo, got {:?}", other),
    }
}

// --- Visitor tests ---

/// Collect all variable names from InterpolatedText fields.
struct VarCollector {
    variables: Vec<String>,
}

impl VarCollector {
    fn new() -> Self {
        Self { variables: Vec::new() }
    }

    fn collect_from_text(&mut self, text: &InterpolatedText) {
        for name in text.variable_names() {
            self.variables.push(name.to_string());
        }
    }
}

impl Visitor for VarCollector {
    fn visit_echo(&mut self, cmd: &EchoCmd) -> VisitResult {
        self.collect_from_text(&cmd.text);
        VisitResult::Continue
    }

    fn visit_exec(&mut self, cmd: &ExecCmd) -> VisitResult {
        self.collect_from_text(&cmd.command);
        VisitResult::Continue
    }

    fn visit_execw(&mut self, cmd: &ExecwCmd) -> VisitResult {
        self.collect_from_text(&cmd.command);
        VisitResult::Continue
    }

    fn visit_exec_in_background(&mut self, cmd: &ExecInBackgroundCmd) -> VisitResult {
        self.collect_from_text(&cmd.command);
        VisitResult::Continue
    }

    fn visit_source(&mut self, cmd: &SourceCmd) -> VisitResult {
        self.collect_from_text(&cmd.file);
        VisitResult::Continue
    }

    fn visit_connect(&mut self, cmd: &ConnectCmd) -> VisitResult {
        if let Some(ref name) = cmd.name {
            self.collect_from_text(name);
        }
        if let Some(ref host) = cmd.params.host {
            self.collect_from_text(host);
        }
        if let Some(ref user) = cmd.params.user {
            self.collect_from_text(user);
        }
        if let Some(ref pw) = cmd.params.password {
            self.collect_from_text(pw);
        }
        if let Some(ref db) = cmd.params.database {
            self.collect_from_text(db);
        }
        VisitResult::Continue
    }

    fn visit_connection(&mut self, cmd: &ConnectionCmd) -> VisitResult {
        self.collect_from_text(&cmd.name);
        VisitResult::Continue
    }

    fn visit_query(&mut self, cmd: &QueryCmd) -> VisitResult {
        self.collect_from_text(&cmd.sql);
        VisitResult::Continue
    }

    fn visit_eval(&mut self, cmd: &EvalCmd) -> VisitResult {
        self.collect_from_text(&cmd.sql);
        VisitResult::Continue
    }

    fn visit_send(&mut self, cmd: &SendCmd) -> VisitResult {
        self.collect_from_text(&cmd.sql);
        VisitResult::Continue
    }

    fn visit_send_eval(&mut self, cmd: &SendEvalCmd) -> VisitResult {
        self.collect_from_text(&cmd.sql);
        VisitResult::Continue
    }

    fn visit_system(&mut self, cmd: &SystemCmd) -> VisitResult {
        self.collect_from_text(&cmd.command);
        VisitResult::Continue
    }
}

#[test]
fn test_visitor_basic_traversal() {
    let input = "--echo hello\n--exec ls\nSELECT 1;\n";
    let result = strict_parse(input);
    assert_eq!(result.statements.len(), 3);

    let mut collector = VarCollector::new();
    collector.visit_test_file(&result);
    // echo hello has no variables, exec ls has no variables
    assert!(collector.variables.is_empty());
}

#[test]
fn test_visitor_collects_variables() {
    let input = "\
--echo host=$host
--echo port=$port
--exec ./script --user=$user
SELECT $unresolved;\n";
    let result = strict_parse(input);
    assert_eq!(result.statements.len(), 4);

    let mut collector = VarCollector::new();
    collector.visit_test_file(&result);
    // echo: $host, $port; exec: $user
    // The SQL fallback does NOT parse variables (raw SQL)
    assert_eq!(collector.variables, vec!["host", "port", "user"]);
}

#[test]
fn test_visitor_if_block_traversal() {
    let input = "\
--let $x = 1
--if ($x)
{
  --echo inside
  --exec prog $x
}
--echo outside\n";
    let result = strict_parse(input);

    let mut collector = VarCollector::new();
    collector.visit_test_file(&result);
    // exec prog $x inside the if block
    assert!(collector.variables.contains(&"x".to_string()));
}

#[test]
fn test_visitor_stop_control() {
    let input = "\
--echo first
--echo second
--echo third\n";
    let result = strict_parse(input);

    struct StopAfterFirst;
    impl Visitor for StopAfterFirst {
        fn visit_echo(&mut self, _cmd: &EchoCmd) -> VisitResult {
            VisitResult::Stop
        }
    }

    let mut visitor = StopAfterFirst;
    visitor.visit_test_file(&result);
    // Only first echo should have been visited
}

#[test]
fn test_visitor_connect_variables() {
    let input = "\
--connect(con1, $host, $user, $pass, $db)\n";
    let result = strict_parse(input);

    let mut collector = VarCollector::new();
    collector.visit_test_file(&result);
    assert_eq!(collector.variables, vec!["host", "user", "pass", "db"]);
}

#[test]
fn test_visitor_while_block_traversal() {
    let input = "\
--let $i = 5
--while ($i)
{
  --echo counter is $i
  --dec $i
}\n";
    let result = strict_parse(input);

    let mut collector = VarCollector::new();
    collector.visit_test_file(&result);
    // echo inside while: $i
    assert!(collector.variables.contains(&"i".to_string()));
}

#[test]
fn test_visitor_mixed_commands() {
    let input = "\
--let $tmpdir = /tmp
--source include/$test.inc
--exec $MYSQLD --init-file=$tmpdir/bootstrap.sql
--echo done\n";
    let result = strict_parse(input);

    let mut collector = VarCollector::new();
    collector.visit_test_file(&result);
    assert!(collector.variables.contains(&"tmpdir".to_string()));
    assert!(collector.variables.contains(&"test".to_string()));
    assert!(collector.variables.contains(&"MYSQLD".to_string()));
}

#[test]
fn test_visitor_no_double_count_in_nested_blocks() {
    let input = "\
--echo $a
--if (1)
{
  --echo $a
  --if (1)
  {
    --echo $b
  }
}\n";
    let result = strict_parse(input);

    let mut collector = VarCollector::new();
    collector.visit_test_file(&result);
    // $a appears twice (top-level + inside if), $b once
    let a_count = collector.variables.iter().filter(|v| *v == "a").count();
    let b_count = collector.variables.iter().filter(|v| *v == "b").count();
    assert_eq!(a_count, 2);
    assert_eq!(b_count, 1);
}
