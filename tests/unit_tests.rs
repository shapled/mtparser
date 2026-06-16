use mtparser::ast::text::TextPart;
use mtparser::ast::*;
use mtparser::parser::{ParserConfig, parse, parse_bytes};
use mtparser::version::MysqlVersion;
use mtparser::visitor::{VisitResult, Visitor, mut_visitor::MutVisitor};

fn strict_parse(input: &str) -> MTFile {
    let config = ParserConfig::new(MysqlVersion::Compatible);
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
    assert!(
        matches!(&result.statements[0], Statement::Comment(c) if c.text == " this is a comment")
    );
}

#[test]
fn test_indented_comment() {
    let result = strict_parse("  # indented comment\n");
    assert!(
        matches!(&result.statements[0], Statement::Comment(c) if c.text == " indented comment")
    );
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
            assert_eq!(
                c.error_codes
                    .iter()
                    .map(|e| e.to_raw_string())
                    .collect::<Vec<_>>(),
                vec!["1064"]
            );
        }
        other => panic!("expected Error, got {:?}", other),
    }
}

#[test]
fn test_error_multiple() {
    let result = strict_parse("--error 1064, 1146\n");
    match &result.statements[0] {
        Statement::Error(c) => {
            assert_eq!(
                c.error_codes
                    .iter()
                    .map(|e| e.to_raw_string())
                    .collect::<Vec<_>>(),
                vec!["1064", "1146"]
            );
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
            assert_eq!(
                c.name.as_ref().map(|m| m.to_raw_string()),
                Some("con1".to_string())
            );
            assert_eq!(
                c.params.host.as_ref().map(|m| m.to_raw_string()),
                Some("localhost".to_string())
            );
            assert_eq!(
                c.params.user.as_ref().map(|m| m.to_raw_string()),
                Some("root".to_string())
            );
            assert_eq!(
                c.params.database.as_ref().map(|m| m.to_raw_string()),
                Some("test".to_string())
            );
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
    let result = strict_parse(
        "SELECT 1;\ndelimiter ||\nCREATE PROCEDURE p1()\nBEGIN\n  SELECT 1;\nEND||\ndelimiter ;\nSELECT 2;\n",
    );
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
    let result =
        strict_parse("--echo before\n--if (`SELECT 1 = 1`)\n{\n  --echo inside\n}\n--echo after\n");
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
            assert!(matches!(&b.condition, Expr::Negated(_)));
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
    let sql_count = result
        .statements
        .iter()
        .filter(|s| matches!(s, Statement::Sql(_)))
        .count();
    assert!(
        sql_count >= 5,
        "expected at least 5 SQL statements, got {}",
        sql_count
    );
}

#[test]
fn test_flow_control_fixture() {
    let input = std::fs::read_to_string("tests/fixtures/flow_control.test")
        .expect("failed to read fixture");
    let result = strict_parse(&input);
    let if_count = result
        .statements
        .iter()
        .filter(|s| matches!(s, Statement::If(_)))
        .count();
    let while_count = result
        .statements
        .iter()
        .filter(|s| matches!(s, Statement::While(_)))
        .count();
    assert!(
        if_count >= 3,
        "expected at least 3 if blocks, got {}",
        if_count
    );
    assert!(
        while_count >= 1,
        "expected at least 1 while block, got {}",
        while_count
    );
}

#[test]
fn test_connection_fixture() {
    let input =
        std::fs::read_to_string("tests/fixtures/connection.test").expect("failed to read fixture");
    let result = strict_parse(&input);
    let connect_count = result
        .statements
        .iter()
        .filter(|s| matches!(s, Statement::Connect(_)))
        .count();
    assert!(
        connect_count >= 2,
        "expected at least 2 connect commands, got {}",
        connect_count
    );
}

#[test]
fn test_delimiter_fixture() {
    let input =
        std::fs::read_to_string("tests/fixtures/delimiter.test").expect("failed to read fixture");
    let result = strict_parse(&input);
    let delim_count = result
        .statements
        .iter()
        .filter(|s| matches!(s, Statement::Delimiter(_)))
        .count();
    assert_eq!(
        delim_count, 2,
        "expected 2 delimiter commands, got {}",
        delim_count
    );
}

#[test]
fn test_write_file_fixture() {
    let input =
        std::fs::read_to_string("tests/fixtures/write_file.test").expect("failed to read fixture");
    let result = strict_parse(&input);
    assert!(
        result
            .statements
            .iter()
            .any(|s| matches!(s, Statement::WriteFile(_)))
    );
    assert!(
        result
            .statements
            .iter()
            .any(|s| matches!(s, Statement::AppendFile(_)))
    );
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
    let config = ParserConfig::new(MysqlVersion::V57);
    let result = parse("--system ls -la\n", config).expect("parse failed");
    assert!(matches!(&result.statements[0], Statement::System(_)));
}

#[test]
fn test_v80_system_command_fails() {
    let config = ParserConfig::new(MysqlVersion::V80);
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
    assert!(matches!(
        &result.statements[0],
        Statement::ShutdownServer(_)
    ));
}

// --- Empty lines ---

#[test]
fn test_multiple_empty_lines() {
    let result = strict_parse("\n\n\n");
    assert_eq!(result.statements.len(), 3);
    assert!(
        result
            .statements
            .iter()
            .all(|s| matches!(s, Statement::Empty))
    );
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
        Self {
            variables: Vec::new(),
        }
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
    collector.visit_mt_file(&result);
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
    collector.visit_mt_file(&result);
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
    collector.visit_mt_file(&result);
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
    visitor.visit_mt_file(&result);
    // Only first echo should have been visited
}

#[test]
fn test_visitor_connect_variables() {
    let input = "\
--connect(con1, $host, $user, $pass, $db)\n";
    let result = strict_parse(input);

    let mut collector = VarCollector::new();
    collector.visit_mt_file(&result);
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
    collector.visit_mt_file(&result);
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
    collector.visit_mt_file(&result);
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
    collector.visit_mt_file(&result);
    // $a appears twice (top-level + inside if), $b once
    let a_count = collector.variables.iter().filter(|v| *v == "a").count();
    let b_count = collector.variables.iter().filter(|v| *v == "b").count();
    assert_eq!(a_count, 2);
    assert_eq!(b_count, 1);
}

// --- Condition expression tests (flow.rs coverage) ---

fn parse_mariadb(input: &str) -> MTFile {
    let config = ParserConfig::new(MysqlVersion::MariaDB);
    parse(input, config).expect("parse failed")
}

#[test]
fn test_condition_integer() {
    let result = strict_parse("--if (0)\n{\n  --echo zero\n}\n");
    match &result.statements[0] {
        Statement::If(b) => assert!(matches!(&b.condition, Expr::Integer(0))),
        other => panic!("expected If, got {:?}", other),
    }
}

#[test]
fn test_condition_comparison_integer_rhs() {
    let result = strict_parse("--if ($x == 1)\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => match &b.condition {
            Expr::Comparison {
                operator, right, ..
            } => {
                assert_eq!(*operator, ComparisonOp::Eq);
                assert!(matches!(right.as_ref(), ComparisonRhs::Integer(1)));
            }
            other => panic!("expected Comparison, got {:?}", other),
        },
        other => panic!("expected If, got {:?}", other),
    }
}

#[test]
fn test_condition_comparison_string_rhs() {
    let result = strict_parse("--if ($x == \"hello\")\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => match &b.condition {
            Expr::Comparison { right, .. } => {
                assert!(matches!(right.as_ref(), ComparisonRhs::String(s) if s == "hello"));
            }
            other => panic!("expected Comparison, got {:?}", other),
        },
        other => panic!("expected If, got {:?}", other),
    }
}

#[test]
fn test_condition_comparison_variable_rhs() {
    let result = strict_parse("--if ($x == $y)\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => match &b.condition {
            Expr::Comparison { right, .. } => {
                assert!(matches!(right.as_ref(), ComparisonRhs::Variable(v) if v.name == "y"));
            }
            other => panic!("expected Comparison, got {:?}", other),
        },
        other => panic!("expected If, got {:?}", other),
    }
}

#[test]
fn test_condition_comparison_all_ops() {
    let ops = [
        ("!=", ComparisonOp::Neq),
        ("<", ComparisonOp::Lt),
        ("<=", ComparisonOp::Le),
        (">", ComparisonOp::Gt),
        (">=", ComparisonOp::Ge),
    ];
    for (op_str, expected_op) in ops {
        let input = format!("--if ($x {} 0)\n{{\n}}\n", op_str);
        let result = strict_parse(&input);
        match &result.statements[0] {
            Statement::If(b) => match &b.condition {
                Expr::Comparison { operator, .. } => assert_eq!(*operator, expected_op),
                other => panic!("op {}: expected Comparison, got {:?}", op_str, other),
            },
            other => panic!("op {}: expected If, got {:?}", op_str, other),
        }
    }
}

#[test]
fn test_condition_empty_error() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--if ()\n{\n}\n", config);
    assert!(result.is_err());
}

#[test]
fn test_condition_invalid_error() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--if (???)\n{\n}\n", config);
    assert!(result.is_err());
}

#[test]
fn test_condition_negated_integer() {
    let result = strict_parse("--if (!0)\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => match &b.condition {
            Expr::Negated(inner) => assert!(matches!(inner.as_ref(), Expr::Integer(0))),
            other => panic!("expected Negated, got {:?}", other),
        },
        other => panic!("expected If, got {:?}", other),
    }
}

#[test]
fn test_condition_negated_query() {
    let result = strict_parse("--if (!`SELECT 0`)\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => match &b.condition {
            Expr::Negated(inner) => assert!(matches!(inner.as_ref(), Expr::Query(_))),
            other => panic!("expected Negated, got {:?}", other),
        },
        other => panic!("expected If, got {:?}", other),
    }
}

// --- MariaDB expression tests ---

#[test]
fn test_mariadb_dollar_paren() {
    let result = parse_mariadb("--if ($(5 == 5))\n{\n  --echo ok\n}\n");
    match &result.statements[0] {
        Statement::If(b) => match &b.condition {
            Expr::MariaDBClosure { expression, .. } => assert_eq!(expression, "5 == 5"),
            other => panic!("expected MariaDBClosure, got {:?}", other),
        },
        other => panic!("expected If, got {:?}", other),
    }
}

#[test]
fn test_mariadb_dollar_paren_variable() {
    let result = parse_mariadb("--if ($($x > 0))\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => match &b.condition {
            Expr::MariaDBClosure { expression, .. } => assert_eq!(expression, "$x > 0"),
            other => panic!("expected MariaDBClosure, got {:?}", other),
        },
        other => panic!("expected If, got {:?}", other),
    }
}

#[test]
fn test_mariadb_and_operator() {
    let result = parse_mariadb("--if (0 && $have_debug)\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => match &b.condition {
            Expr::MariaDBLogical { expression, .. } => assert_eq!(expression, "0 && $have_debug"),
            other => panic!("expected MariaDBLogical, got {:?}", other),
        },
        other => panic!("expected If, got {:?}", other),
    }
}

#[test]
fn test_mariadb_or_operator() {
    let result = parse_mariadb("--if ($x || $y)\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => match &b.condition {
            Expr::MariaDBLogical { expression, .. } => assert_eq!(expression, "$x || $y"),
            other => panic!("expected MariaDBLogical, got {:?}", other),
        },
        other => panic!("expected If, got {:?}", other),
    }
}

#[test]
fn test_mariadb_not_parsed_in_mysql_mode() {
    // $() should NOT be parsed in Compatible (MySQL) mode
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--if ($(5 == 5))\n{\n}\n", config);
    // Should fail because $( is not a recognized expression start in MySQL mode
    assert!(result.is_err());
}

// --- Delimiter-terminated commands (mod.rs coverage) ---

#[test]
fn test_delimiter_terminated_connect() {
    let result = strict_parse("connect(con1, localhost, root, , test);\n");
    match &result.statements[0] {
        Statement::Connect(c) => {
            assert_eq!(
                c.name.as_ref().map(|n| n.to_raw_string()),
                Some("con1".to_string())
            );
        }
        other => panic!("expected Connect, got {:?}", other),
    }
}

#[test]
fn test_delimiter_terminated_disconnect() {
    let result = strict_parse("disconnect con1;\n");
    match &result.statements[0] {
        Statement::Disconnect(c) => assert_eq!(c.name.to_raw_string(), "con1"),
        other => panic!("expected Disconnect, got {:?}", other),
    }
}

#[test]
fn test_delimiter_terminated_connection() {
    let result = strict_parse("connection con1;\n");
    match &result.statements[0] {
        Statement::Connection(c) => assert_eq!(c.name.to_raw_string(), "con1"),
        other => panic!("expected Connection, got {:?}", other),
    }
}

#[test]
fn test_delimiter_terminated_reap() {
    let result = strict_parse("reap;\n");
    assert!(matches!(&result.statements[0], Statement::Reap(_)));
}

#[test]
fn test_delimiter_terminated_shutdown() {
    let result = strict_parse("shutdown_server 10000;\n");
    assert!(matches!(
        &result.statements[0],
        Statement::ShutdownServer(_)
    ));
}

#[test]
fn test_delimiter_terminated_with_comment() {
    let result = strict_parse("--error 1064\nSELECT bad;\n# a comment\n");
    assert_eq!(result.statements.len(), 3);
    assert!(matches!(&result.statements[0], Statement::Error(_)));
    assert!(matches!(&result.statements[1], Statement::Sql(_)));
    assert!(matches!(&result.statements[2], Statement::Comment(_)));
}

// --- write_file / append_file block parsing (mod.rs coverage) ---

#[test]
fn test_write_file_with_content() {
    let result = strict_parse("--write_file /tmp/test.txt\nline1\nline2\nEOF\n");
    match &result.statements[0] {
        Statement::WriteFile(c) => {
            assert_eq!(c.filename.to_raw_string(), "/tmp/test.txt");
            assert_eq!(c.end_marker, "EOF");
            assert_eq!(c.content.to_raw_string(), "line1\nline2");
        }
        other => panic!("expected WriteFile, got {:?}", other),
    }
}

#[test]
fn test_append_file_with_content() {
    let result = strict_parse("--append_file /tmp/test.txt\nextra\nEOF\n");
    match &result.statements[0] {
        Statement::AppendFile(c) => {
            assert_eq!(c.filename.to_raw_string(), "/tmp/test.txt");
            assert_eq!(c.content.to_raw_string(), "extra");
        }
        other => panic!("expected AppendFile, got {:?}", other),
    }
}

#[test]
fn test_write_file_custom_marker() {
    let result = strict_parse("--write_file /tmp/test.txt MY_EOF\ncontent\nMY_EOF\n");
    match &result.statements[0] {
        Statement::WriteFile(c) => {
            assert_eq!(c.end_marker, "MY_EOF");
            assert_eq!(c.content.to_raw_string(), "content");
        }
        other => panic!("expected WriteFile, got {:?}", other),
    }
}

#[test]
fn test_perl_block() {
    let result = strict_parse("--perl\nprint \"hello\";\nEOF\n");
    match &result.statements[0] {
        Statement::Perl(b) => {
            assert!(b.content.contains("print"));
            assert_eq!(b.end_marker, "EOF");
        }
        other => panic!("expected Perl, got {:?}", other),
    }
}

#[test]
fn test_write_file_quoted_filename() {
    // Quotes should be stripped from ARG_STRING filename
    // Note: parse_file_args splits on space, so quoted paths with spaces
    // need to not contain spaces, or the parser needs enhancement
    let result = strict_parse("--write_file \"/tmp/myfile.txt\"\ncontent\nEOF\n");
    match &result.statements[0] {
        Statement::WriteFile(c) => {
            assert_eq!(c.filename.to_raw_string(), "/tmp/myfile.txt");
        }
        other => panic!("expected WriteFile, got {:?}", other),
    }
}

#[test]
fn test_write_file_quoted_filename_with_semicolon() {
    // Filename with semicolon inside quotes - the strip_trailing_delimiter_quoted
    // should handle this, but parse_file_args splits on space which is separate
    let result = strict_parse("--write_file \"/tmp/myfile;test.qp\"\nfoo\nEOF\n");
    match &result.statements[0] {
        Statement::WriteFile(c) => {
            assert_eq!(c.filename.to_raw_string(), "/tmp/myfile;test.qp");
        }
        other => panic!("expected WriteFile, got {:?}", other),
    }
}

#[test]
fn test_bare_if_block() {
    let result = strict_parse("if ($x) {\n  --echo inside\n}\n");
    match &result.statements[0] {
        Statement::If(b) => {
            assert!(matches!(&b.condition, Expr::Variable(v) if v.name == "x"));
            assert_eq!(b.body.len(), 1);
        }
        other => panic!("expected If, got {:?}", other),
    }
}

#[test]
fn test_bare_while_block() {
    let result = strict_parse("while ($i) {\n  --dec $i;\n}\n");
    match &result.statements[0] {
        Statement::While(b) => {
            assert!(matches!(&b.condition, Expr::Variable(v) if v.name == "i"));
            assert_eq!(b.body.len(), 1);
        }
        other => panic!("expected While, got {:?}", other),
    }
}

#[test]
fn test_inline_if_block() {
    let result = strict_parse("if (1) { --echo ok; }\n");
    match &result.statements[0] {
        Statement::If(b) => {
            assert!(matches!(&b.condition, Expr::Integer(1)));
            assert_eq!(b.body.len(), 1);
        }
        other => panic!("expected If, got {:?}", other),
    }
}

#[test]
fn test_inline_while_block() {
    let result = strict_parse("while ($i) { --dec $i; }\n");
    match &result.statements[0] {
        Statement::While(b) => {
            assert_eq!(b.body.len(), 1);
        }
        other => panic!("expected While, got {:?}", other),
    }
}

// --- Additional command tests (command.rs coverage) ---

#[test]
fn test_source_quoted() {
    let result = strict_parse("--source \"include/test.inc\"\n");
    match &result.statements[0] {
        Statement::Source(c) => assert_eq!(c.file.to_raw_string(), "include/test.inc"),
        other => panic!("expected Source, got {:?}", other),
    }
}

#[test]
fn test_mkdir_quoted() {
    let result = strict_parse("--mkdir \"/tmp/new dir\"\n");
    match &result.statements[0] {
        Statement::Mkdir(c) => assert_eq!(c.dir.to_raw_string(), "/tmp/new dir"),
        other => panic!("expected Mkdir, got {:?}", other),
    }
}

#[test]
fn test_rmdir() {
    let result = strict_parse("--rmdir /tmp/olddir\n");
    match &result.statements[0] {
        Statement::Rmdir(c) => assert_eq!(c.dir.to_raw_string(), "/tmp/olddir"),
        other => panic!("expected Rmdir, got {:?}", other),
    }
}

#[test]
fn test_cat_file() {
    let result = strict_parse("--cat_file /tmp/out.txt\n");
    match &result.statements[0] {
        Statement::CatFile(c) => assert_eq!(c.file.to_raw_string(), "/tmp/out.txt"),
        other => panic!("expected CatFile, got {:?}", other),
    }
}

#[test]
fn test_file_exists() {
    let result = strict_parse("--file_exists /tmp/test.txt\n");
    match &result.statements[0] {
        Statement::FileExists(c) => assert_eq!(c.file.to_raw_string(), "/tmp/test.txt"),
        other => panic!("expected FileExists, got {:?}", other),
    }
}

#[test]
fn test_copy_file() {
    let result = strict_parse("--copy_file /tmp/a.txt /tmp/b.txt\n");
    match &result.statements[0] {
        Statement::CopyFile(c) => {
            assert_eq!(c.source.to_raw_string(), "/tmp/a.txt");
            assert_eq!(c.dest.to_raw_string(), "/tmp/b.txt");
        }
        other => panic!("expected CopyFile, got {:?}", other),
    }
}

#[test]
fn test_move_file() {
    let result = strict_parse("--move_file /tmp/a.txt /tmp/b.txt\n");
    match &result.statements[0] {
        Statement::MoveFile(c) => {
            assert_eq!(c.source.to_raw_string(), "/tmp/a.txt");
            assert_eq!(c.dest.to_raw_string(), "/tmp/b.txt");
        }
        other => panic!("expected MoveFile, got {:?}", other),
    }
}

#[test]
fn test_diff_files() {
    let result = strict_parse("--diff_files /tmp/a.txt /tmp/b.txt\n");
    match &result.statements[0] {
        Statement::DiffFiles(c) => {
            assert_eq!(c.file1.to_raw_string(), "/tmp/a.txt");
            assert_eq!(c.file2.to_raw_string(), "/tmp/b.txt");
        }
        other => panic!("expected DiffFiles, got {:?}", other),
    }
}

#[test]
fn test_list_files() {
    let result = strict_parse("--list_files /tmp *.txt\n");
    match &result.statements[0] {
        Statement::ListFiles(c) => {
            assert_eq!(
                c.dir.as_ref().map(|t| t.to_raw_string()),
                Some("/tmp".to_string())
            );
            assert_eq!(
                c.pattern.as_ref().map(|t| t.to_raw_string()),
                Some("*.txt".to_string())
            );
        }
        other => panic!("expected ListFiles, got {:?}", other),
    }
}

#[test]
fn test_list_files_with_quoted_dir() {
    let result = strict_parse("--list_files \"/tmp my dir\" *.txt\n");
    match &result.statements[0] {
        Statement::ListFiles(c) => {
            // strip_quotes removes quotes, but split_whitespace splits on the inner space
            // This tests the current parsing behavior
            assert!(c.dir.is_some());
        }
        other => panic!("expected ListFiles, got {:?}", other),
    }
}

#[test]
fn test_chmod() {
    let result = strict_parse("--chmod 644 /tmp/file.txt\n");
    match &result.statements[0] {
        Statement::Chmod(c) => {
            assert_eq!(c.mode, "644");
            assert_eq!(c.file.to_raw_string(), "/tmp/file.txt");
        }
        other => panic!("expected Chmod, got {:?}", other),
    }
}

#[test]
fn test_character_set() {
    let result = strict_parse("--character_set utf8\n");
    match &result.statements[0] {
        Statement::CharacterSet(c) => assert_eq!(c.charset, "utf8"),
        other => panic!("expected CharacterSet, got {:?}", other),
    }
}

#[test]
fn test_system_command() {
    let config = ParserConfig::new(MysqlVersion::V57);
    let result = parse("--system ls\n", config).expect("parse failed");
    assert!(matches!(&result.statements[0], Statement::System(_)));
}

#[test]
fn test_require_command() {
    let config = ParserConfig::new(MysqlVersion::V57);
    let result = parse("--require some_file.inc\n", config).expect("parse failed");
    match &result.statements[0] {
        Statement::Require(c) => assert_eq!(c.file.to_raw_string(), "some_file.inc"),
        other => panic!("expected Require, got {:?}", other),
    }
}

#[test]
fn test_real_sleep_command() {
    let config = ParserConfig::new(MysqlVersion::V57);
    let result = parse("--real_sleep 0.5\n", config).expect("parse failed");
    assert!(matches!(&result.statements[0], Statement::RealSleep(_)));
}

#[test]
fn test_lowercase_result() {
    let result = strict_parse("--lowercase_result\n");
    assert!(matches!(
        &result.statements[0],
        Statement::LowercaseResult(_)
    ));
}

#[test]
fn test_toggle_once() {
    let result = strict_parse("--disable_warnings ONCE\n");
    match &result.statements[0] {
        Statement::Toggle(c) => {
            assert!(!c.enabled);
            assert!(c.once);
        }
        other => panic!("expected Toggle, got {:?}", other),
    }
}

#[test]
fn test_query_command() {
    let result = strict_parse("--query SELECT 1;\n");
    assert!(matches!(&result.statements[0], Statement::Query(_)));
}

#[test]
fn test_eval_command() {
    let result = strict_parse("--eval SELECT $x;\n");
    assert!(matches!(&result.statements[0], Statement::Eval(_)));
}

#[test]
fn test_send_quit() {
    let result = strict_parse("--send_quit\n");
    assert!(matches!(&result.statements[0], Statement::SendQuit(_)));
}

#[test]
fn test_send_shutdown() {
    let result = strict_parse("--send_shutdown\n");
    assert!(matches!(&result.statements[0], Statement::SendShutdown(_)));
}

#[test]
fn test_execw() {
    let result = strict_parse("--execw ls\n");
    assert!(matches!(&result.statements[0], Statement::Execw(_)));
}

#[test]
fn test_exec_in_background() {
    let result = strict_parse("--exec_in_background sleep 5\n");
    assert!(matches!(
        &result.statements[0],
        Statement::ExecInBackground(_)
    ));
}

#[test]
fn test_horizontal_results() {
    let result = strict_parse("--horizontal_results\n");
    assert!(matches!(
        &result.statements[0],
        Statement::HorizontalResults(_)
    ));
}

#[test]
fn test_vertical_results() {
    let result = strict_parse("--vertical_results\n");
    assert!(matches!(
        &result.statements[0],
        Statement::VerticalResults(_)
    ));
}

#[test]
fn test_sorted_result() {
    let result = strict_parse("--sorted_result\n");
    assert!(matches!(&result.statements[0], Statement::SortedResult(_)));
}

#[test]
fn test_partially_sorted_result() {
    let result = strict_parse("--partially_sorted_result\n");
    assert!(matches!(
        &result.statements[0],
        Statement::PartiallySortedResult(_)
    ));
}

#[test]
fn test_replace_result() {
    let result = strict_parse("--replace_result 1 # 2\n");
    assert!(matches!(&result.statements[0], Statement::ReplaceResult(_)));
}

#[test]
fn test_replace_numeric_round() {
    let result = strict_parse("--replace_numeric_round 2\n");
    assert!(matches!(
        &result.statements[0],
        Statement::ReplaceNumericRound(_)
    ));
}

#[test]
fn test_remove_files_wildcard() {
    let result = strict_parse("--remove_files_wildcard /tmp *.tmp\n");
    assert!(matches!(
        &result.statements[0],
        Statement::RemoveFilesWildcard(_)
    ));
}

#[test]
fn test_copy_files_wildcard() {
    let result = strict_parse("--copy_files_wildcard /tmp/src /tmp/dst *.txt\n");
    assert!(matches!(
        &result.statements[0],
        Statement::CopyFilesWildcard(_)
    ));
}

#[test]
fn test_sync_slave_with_master() {
    let result = strict_parse("--sync_slave_with_master\n");
    assert!(matches!(
        &result.statements[0],
        Statement::SyncSlaveWithMaster(_)
    ));
}

#[test]
fn test_toggle_all_variants() {
    let toggles = [
        ("disable_query_log", ToggleKind::QueryLog, false),
        ("enable_query_log", ToggleKind::QueryLog, true),
        ("disable_result_log", ToggleKind::ResultLog, false),
        ("enable_result_log", ToggleKind::ResultLog, true),
        ("disable_metadata", ToggleKind::Metadata, false),
        ("enable_metadata", ToggleKind::Metadata, true),
        ("disable_ps_protocol", ToggleKind::PsProtocol, false),
        ("enable_ps_protocol", ToggleKind::PsProtocol, true),
        ("disable_reconnect", ToggleKind::Reconnect, false),
        ("enable_reconnect", ToggleKind::Reconnect, true),
        ("disable_connect_log", ToggleKind::ConnectLog, false),
        ("enable_connect_log", ToggleKind::ConnectLog, true),
        (
            "disable_session_track_info",
            ToggleKind::SessionTrackInfo,
            false,
        ),
        (
            "enable_session_track_info",
            ToggleKind::SessionTrackInfo,
            true,
        ),
        ("disable_info", ToggleKind::Info, false),
        ("enable_info", ToggleKind::Info, true),
        ("disable_testcase", ToggleKind::Testcase, false),
        ("enable_testcase", ToggleKind::Testcase, true),
        ("disable_async_client", ToggleKind::AsyncClient, false),
        ("enable_async_client", ToggleKind::AsyncClient, true),
    ];
    for (cmd, kind, enabled) in toggles {
        let result = strict_parse(&format!("--{}\n", cmd));
        match &result.statements[0] {
            Statement::Toggle(c) => {
                assert_eq!(c.kind, kind, "{}: wrong kind", cmd);
                assert_eq!(c.enabled, enabled, "{}: wrong enabled", cmd);
                assert!(!c.once, "{}: should not be ONCE", cmd);
            }
            other => panic!("{}: expected Toggle, got {:?}", cmd, other),
        }
    }
}

#[test]
fn test_change_user() {
    let result = strict_parse("--change_user user\n");
    assert!(matches!(&result.statements[0], Statement::ChangeUser(_)));
}

#[test]
fn test_reset_connection() {
    let result = strict_parse("--reset_connection\n");
    assert!(matches!(
        &result.statements[0],
        Statement::ResetConnection(_)
    ));
}

#[test]
fn test_send_eval() {
    let result = strict_parse("--send_eval SELECT 1;\n");
    assert!(matches!(&result.statements[0], Statement::SendEval(_)));
}

// --- Version tests (version.rs coverage) ---

#[test]
fn test_mariadb_is_mariadb() {
    assert!(MysqlVersion::MariaDB.is_mariadb());
    assert!(MysqlVersion::MariaDB_118.is_mariadb());
    assert!(MysqlVersion::MariaDB_123.is_mariadb());
    assert!(!MysqlVersion::Compatible.is_mariadb());
    assert!(!MysqlVersion::V80.is_mariadb());
    assert!(!MysqlVersion::V57.is_mariadb());
}

#[test]
fn test_mariadb_has_v57_commands() {
    // MariaDB is a superset: supports V57-only commands
    assert!(MysqlVersion::MariaDB.has_command("require"));
    assert!(MysqlVersion::MariaDB.has_command("system"));
    assert!(MysqlVersion::MariaDB.has_command("real_sleep"));
}

#[test]
fn test_mariadb_has_assert() {
    assert!(MysqlVersion::MariaDB.has_assert());
}

#[test]
fn test_compatible_plus_mariadb() {
    let v = MysqlVersion::Compatible | MysqlVersion::MariaDB;
    assert!(v.is_mariadb());
    assert!(v.has_command("require"));
    assert!(v.has_command("assert"));
}

#[test]
fn test_specific_mariadb_version() {
    assert!(MysqlVersion::MariaDB_1011.is_mariadb());
    assert!(MysqlVersion::MariaDB_114.is_mariadb());
    assert!(MysqlVersion::MariaDB_118.is_mariadb());
    assert!(MysqlVersion::MariaDB_123.is_mariadb());
}

#[test]
fn test_assert_version_compatibility() {
    assert!(!MysqlVersion::V57.has_assert());
    assert!(MysqlVersion::V80.has_assert());
    assert!(MysqlVersion::V84.has_assert());
    assert!(MysqlVersion::V97.has_assert());
}

// --- Visitor coverage tests ---

#[test]
fn test_visitor_visit_statement() {
    let input = "--echo hello\nSELECT 1;\n";
    let result = strict_parse(input);
    struct CountVisitor {
        count: usize,
    }
    impl Visitor for CountVisitor {
        fn visit_statement(&mut self, _stmt: &Statement) -> VisitResult {
            self.count += 1;
            VisitResult::Continue
        }
    }
    let mut v = CountVisitor { count: 0 };
    v.visit_mt_file(&result);
    assert_eq!(v.count, 2);
}

#[test]
fn test_visitor_leave_statement() {
    let input = "--echo hello\nSELECT 1;\n";
    let result = strict_parse(input);
    struct LeaveVisitor {
        count: usize,
    }
    impl Visitor for LeaveVisitor {
        fn leave_statement(&mut self, _stmt: &Statement) {
            self.count += 1;
        }
    }
    let mut v = LeaveVisitor { count: 0 };
    v.visit_mt_file(&result);
    assert_eq!(v.count, 2);
}

#[test]
fn test_visitor_visit_mt_file() {
    let input = "--echo hello\n";
    let result = strict_parse(input);
    struct FileVisitor {
        visited: bool,
    }
    impl Visitor for FileVisitor {
        fn visit_mt_file(&mut self, _file: &MTFile) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = FileVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_if() {
    let input = "--if ($x) {\n  --echo inside\n}\n";
    let result = strict_parse(input);
    struct IfVisitor {
        visited: bool,
    }
    impl Visitor for IfVisitor {
        fn visit_if(&mut self, _block: &IfBlock) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = IfVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_while() {
    let input = "--while ($i) {\n  --echo inside\n}\n";
    let result = strict_parse(input);
    struct WhileVisitor {
        visited: bool,
    }
    impl Visitor for WhileVisitor {
        fn visit_while(&mut self, _block: &WhileBlock) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = WhileVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_let() {
    let result = strict_parse("--let $x = 1\n");
    struct LetVisitor {
        visited: bool,
    }
    impl Visitor for LetVisitor {
        fn visit_let(&mut self, _cmd: &LetCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = LetVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_error() {
    let result = strict_parse("--error 1064\n");
    struct ErrorVisitor {
        visited: bool,
    }
    impl Visitor for ErrorVisitor {
        fn visit_error(&mut self, _cmd: &ErrorCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = ErrorVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_sql() {
    let result = strict_parse("SELECT 1;\n");
    struct SqlVisitor {
        visited: bool,
    }
    impl Visitor for SqlVisitor {
        fn visit_sql(&mut self, _stmt: &SqlStatement) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = SqlVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_delimiter() {
    let result = strict_parse("delimiter ||\n");
    struct DelimVisitor {
        visited: bool,
    }
    impl Visitor for DelimVisitor {
        fn visit_delimiter(&mut self, _cmd: &DelimiterCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = DelimVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_comment() {
    let result = strict_parse("# a comment\n");
    struct CommentVisitor {
        visited: bool,
    }
    impl Visitor for CommentVisitor {
        fn visit_comment(&mut self, _node: &CommentNode) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = CommentVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_connect() {
    let result = strict_parse("--connect(con1, localhost, root, , test)\n");
    struct ConnectVisitor {
        visited: bool,
    }
    impl Visitor for ConnectVisitor {
        fn visit_connect(&mut self, _cmd: &ConnectCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = ConnectVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_disconnect() {
    let result = strict_parse("disconnect con1;\n");
    struct DisVisitor {
        visited: bool,
    }
    impl Visitor for DisVisitor {
        fn visit_disconnect(&mut self, _cmd: &DisconnectCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = DisVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_connection_cmd() {
    let result = strict_parse("connection con1;\n");
    struct ConnVisitor {
        visited: bool,
    }
    impl Visitor for ConnVisitor {
        fn visit_connection(&mut self, _cmd: &ConnectionCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = ConnVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_skip() {
    let result = strict_parse("--skip skipping\n");
    struct SkipVisitor {
        visited: bool,
    }
    impl Visitor for SkipVisitor {
        fn visit_skip(&mut self, _cmd: &SkipCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = SkipVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_die() {
    let result = strict_parse("--die fatal\n");
    struct DieVisitor {
        visited: bool,
    }
    impl Visitor for DieVisitor {
        fn visit_die(&mut self, _cmd: &DieCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = DieVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_exit() {
    let result = strict_parse("--exit\n");
    struct ExitVisitor {
        visited: bool,
    }
    impl Visitor for ExitVisitor {
        fn visit_exit(&mut self, _cmd: &ExitCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = ExitVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_inc() {
    let result = strict_parse("--inc $i\n");
    struct IncVisitor {
        visited: bool,
    }
    impl Visitor for IncVisitor {
        fn visit_inc(&mut self, _cmd: &IncCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = IncVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_dec() {
    let result = strict_parse("--dec $i\n");
    struct DecVisitor {
        visited: bool,
    }
    impl Visitor for DecVisitor {
        fn visit_dec(&mut self, _cmd: &DecCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = DecVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_sleep() {
    let result = strict_parse("--sleep 1\n");
    struct SleepVisitor {
        visited: bool,
    }
    impl Visitor for SleepVisitor {
        fn visit_sleep(&mut self, _cmd: &SleepCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = SleepVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_execw() {
    let result = strict_parse("--execw ls\n");
    struct ExecwVisitor {
        visited: bool,
    }
    impl Visitor for ExecwVisitor {
        fn visit_execw(&mut self, _cmd: &ExecwCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = ExecwVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_toggle() {
    let result = strict_parse("--disable_warnings\n");
    struct ToggleVisitor {
        visited: bool,
    }
    impl Visitor for ToggleVisitor {
        fn visit_toggle(&mut self, _cmd: &ToggleCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = ToggleVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_source() {
    let result = strict_parse("--source include/test.inc\n");
    struct SrcVisitor {
        visited: bool,
    }
    impl Visitor for SrcVisitor {
        fn visit_source(&mut self, _cmd: &SourceCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = SrcVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_write_file() {
    let result = strict_parse("--write_file /tmp/test.txt\ncontent\nEOF\n");
    struct WfVisitor {
        visited: bool,
    }
    impl Visitor for WfVisitor {
        fn visit_write_file(&mut self, _cmd: &WriteFileCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = WfVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_mkdir() {
    let result = strict_parse("--mkdir /tmp/dir\n");
    struct MkdirVisitor {
        visited: bool,
    }
    impl Visitor for MkdirVisitor {
        fn visit_mkdir(&mut self, _cmd: &MkdirCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = MkdirVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_remove_file() {
    let result = strict_parse("--remove_file /tmp/f\n");
    struct RmVisitor {
        visited: bool,
    }
    impl Visitor for RmVisitor {
        fn visit_remove_file(&mut self, _cmd: &RemoveFileCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = RmVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

#[test]
fn test_visitor_visit_empty() {
    let result = strict_parse("\n");
    assert!(matches!(&result.statements[0], Statement::Empty));
    // Empty statements don't trigger any visitor hooks, but visit_mt_file and visit_statement cover them
    struct CountVisitor {
        count: usize,
    }
    impl Visitor for CountVisitor {
        fn visit_statement(&mut self, _stmt: &Statement) -> VisitResult {
            self.count += 1;
            VisitResult::Continue
        }
    }
    let mut v = CountVisitor { count: 0 };
    v.visit_mt_file(&result);
    assert_eq!(v.count, 1); // Empty is counted as a statement
}

// --- MariaDB query/eval/send coverage ---

#[test]
fn test_mariadb_parse_basic() {
    let result = parse_mariadb("--echo hello\nSELECT 1;\n");
    assert_eq!(result.statements.len(), 2);
    assert!(matches!(&result.statements[0], Statement::Echo(_)));
    assert!(matches!(&result.statements[1], Statement::Sql(_)));
}

#[test]
fn test_mariadb_write_file_block() {
    let result = parse_mariadb("--write_file \"/tmp/testfile\"\nfoo\nEOF\n");
    match &result.statements[0] {
        Statement::WriteFile(c) => {
            assert_eq!(c.filename.to_raw_string(), "/tmp/testfile");
        }
        other => panic!("expected WriteFile, got {:?}", other),
    }
}

#[test]
fn test_multi_line_delimiter_terminated() {
    let result = strict_parse("--let $x = \n  a\n  b\n;\nSELECT 1;\n");
    assert!(result.statements.len() >= 2);
    assert!(matches!(&result.statements[0], Statement::Let(_)));
}

#[test]
fn test_bare_perl_block() {
    let result = strict_parse("perl\nprint 1;\nEOF\n");
    match &result.statements[0] {
        Statement::Perl(b) => {
            assert!(b.content.contains("print 1"));
            assert_eq!(b.end_marker, "EOF");
        }
        other => panic!("expected Perl, got {:?}", other),
    }
}

#[test]
fn test_unknown_command_as_sql() {
    let result = strict_parse("random_command arg1 arg2;\n");
    match &result.statements[0] {
        Statement::Sql(s) => assert!(s.sql.contains("random_command")),
        other => panic!("expected Sql, got {:?}", other),
    }
}

#[test]
fn test_double_dash_unknown_command_as_sql() {
    let result = parse("--custom_cmd arg;\n", ParserConfig::default());
    assert!(result.is_err(), "expected UnknownCommand error");
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("unknown command 'custom_cmd'"), "got: {}", msg);
}

#[test]
fn test_skip_no_message() {
    let result = strict_parse("--skip\n");
    match &result.statements[0] {
        Statement::Skip(c) => assert!(c.message.is_none()),
        other => panic!("expected Skip, got {:?}", other),
    }
}

#[test]
fn test_die_no_message() {
    let result = strict_parse("--die\n");
    match &result.statements[0] {
        Statement::Die(c) => assert!(c.message.is_none()),
        other => panic!("expected Die, got {:?}", other),
    }
}

#[test]
fn test_send_command() {
    let result = strict_parse("--send SELECT 1;\n");
    assert!(matches!(&result.statements[0], Statement::Send(_)));
}

#[test]
fn test_remove_file_with_timeout() {
    let result = strict_parse("--remove_file /tmp/f 5\n");
    match &result.statements[0] {
        Statement::RemoveFile(c) => {
            assert_eq!(c.file.to_raw_string(), "/tmp/f");
            assert_eq!(c.timeout, Some("5".to_string()));
        }
        other => panic!("expected RemoveFile, got {:?}", other),
    }
}

// --- Additional visitor tests for all visit_xxx methods ---

macro_rules! visitor_test {
    ($name:ident, $input:expr, $method:ident, $cmd_type:ty) => {
        #[test]
        fn $name() {
            let result = strict_parse($input);
            struct V {
                visited: bool,
            }
            impl Visitor for V {
                fn $method(&mut self, _cmd: &$cmd_type) -> VisitResult {
                    self.visited = true;
                    VisitResult::Continue
                }
            }
            let mut v = V { visited: false };
            v.visit_mt_file(&result);
            assert!(v.visited);
        }
    };
}

visitor_test!(
    test_visitor_visit_exec,
    "--exec ls -la\n",
    visit_exec,
    ExecCmd
);
visitor_test!(
    test_visitor_visit_exec_in_background,
    "--exec_in_background sleep 5 &\n",
    visit_exec_in_background,
    ExecInBackgroundCmd
);
// assert is parsed as Sql in the parser, not AssertCmd. Test the SQL fallback visitor.
visitor_test!(
    test_visitor_visit_assert_sql,
    "--assert(`SELECT 1`)\n",
    visit_sql,
    SqlStatement
);
visitor_test!(
    test_visitor_visit_expr,
    "--expr $x = 1 + 2\n",
    visit_expr,
    ExprCmd
);
visitor_test!(
    test_visitor_visit_change_user,
    "--change_user root,,test\n",
    visit_change_user,
    ChangeUserCmd
);
visitor_test!(
    test_visitor_visit_reset_connection,
    "--reset_connection\n",
    visit_reset_connection,
    ResetConnectionCmd
);
visitor_test!(
    test_visitor_visit_query,
    "--query SELECT 1\n",
    visit_query,
    QueryCmd
);
visitor_test!(
    test_visitor_visit_eval,
    "--eval SELECT $x\n",
    visit_eval,
    EvalCmd
);
visitor_test!(
    test_visitor_visit_send,
    "--send SHOW STATUS\n",
    visit_send,
    SendCmd
);
visitor_test!(
    test_visitor_visit_send_eval,
    "--send_eval SELECT 1\n",
    visit_send_eval,
    SendEvalCmd
);
visitor_test!(
    test_visitor_visit_horizontal_results,
    "--horizontal_results\n",
    visit_horizontal_results,
    HorizontalResultsCmd
);
visitor_test!(
    test_visitor_visit_vertical_results,
    "--vertical_results\n",
    visit_vertical_results,
    VerticalResultsCmd
);
visitor_test!(
    test_visitor_visit_replace_result,
    "--replace_result 1 2\n",
    visit_replace_result,
    ReplaceResultCmd
);
visitor_test!(
    test_visitor_visit_replace_column,
    "--replace_column 1 \"a\" \"b\"\n",
    visit_replace_column,
    ReplaceColumnCmd
);
visitor_test!(
    test_visitor_visit_replace_regex,
    "--replace_regex /pattern/replacement/\n",
    visit_replace_regex,
    ReplaceRegexCmd
);
visitor_test!(
    test_visitor_visit_sorted_result,
    "--sorted_result\n",
    visit_sorted_result,
    SortedResultCmd
);
visitor_test!(
    test_visitor_visit_partially_sorted_result,
    "--partially_sorted_result\n",
    visit_partially_sorted_result,
    PartiallySortedResultCmd
);
visitor_test!(
    test_visitor_visit_replace_numeric_round,
    "--replace_numeric_round 2\n",
    visit_replace_numeric_round,
    ReplaceNumericRoundCmd
);
visitor_test!(
    test_visitor_visit_append_file,
    "--append_file /tmp/test.txt\ncontent\nEOF\n",
    visit_append_file,
    AppendFileCmd
);
visitor_test!(
    test_visitor_visit_remove_files_wildcard,
    "--remove_files_wildcard /tmp/*.tmp\n",
    visit_remove_files_wildcard,
    RemoveFilesWildcardCmd
);
visitor_test!(
    test_visitor_visit_copy_file,
    "--copy_file /tmp/a /tmp/b\n",
    visit_copy_file,
    CopyFileCmd
);
visitor_test!(
    test_visitor_visit_move_file,
    "--move_file /tmp/a /tmp/b\n",
    visit_move_file,
    MoveFileCmd
);
visitor_test!(
    test_visitor_visit_rmdir,
    "--rmdir /tmp/dir\n",
    visit_rmdir,
    RmdirCmd
);
visitor_test!(
    test_visitor_visit_chmod,
    "--chmod 644 /tmp/f\n",
    visit_chmod,
    ChmodCmd
);
visitor_test!(
    test_visitor_visit_diff_files,
    "--diff_files /tmp/a /tmp/b\n",
    visit_diff_files,
    DiffFilesCmd
);
visitor_test!(
    test_visitor_visit_file_exists,
    "--file_exists /tmp/f\n",
    visit_file_exists,
    FileExistsCmd
);
visitor_test!(
    test_visitor_visit_cat_file,
    "--cat_file /tmp/f\n",
    visit_cat_file,
    CatFileCmd
);
visitor_test!(
    test_visitor_visit_list_files,
    "--list_files /tmp *.txt\n",
    visit_list_files,
    ListFilesCmd
);
visitor_test!(test_visitor_visit_end, "--end\n", visit_end, EndCmd);
visitor_test!(
    test_visitor_visit_perl,
    "--perl\nprint 1;\nEOF\n",
    visit_perl,
    PerlBlock
);
visitor_test!(
    test_visitor_visit_character_set,
    "--character_set utf8mb4\n",
    visit_character_set,
    CharacterSetCmd
);
visitor_test!(
    test_visitor_visit_system,
    "--system ls\n",
    visit_system,
    SystemCmd
);
visitor_test!(
    test_visitor_visit_real_sleep,
    "--real_sleep 0.5\n",
    visit_real_sleep,
    RealSleepCmd
);
visitor_test!(
    test_visitor_visit_require,
    "--require some_check\n",
    visit_require,
    RequireCmd
);
visitor_test!(
    test_visitor_visit_lowercase_result,
    "--lowercase_result\n",
    visit_lowercase_result,
    LowercaseResultCmd
);
visitor_test!(
    test_visitor_visit_sync_slave_with_master,
    "--sync_slave_with_master\n",
    visit_sync_slave_with_master,
    SyncSlaveWithMasterCmd
);
visitor_test!(
    test_visitor_visit_copy_files_wildcard,
    "--copy_files_wildcard /tmp/*.a /tmp/dest/\n",
    visit_copy_files_wildcard,
    CopyFilesWildcardCmd
);

// --- Visitor: Output command ---
// Note: output is not currently parsed by the parser (classified as SQL).
// Test visitor output by constructing directly.

#[test]
fn test_visitor_visit_output_direct() {
    let file = MTFile::new(vec![Statement::Output(OutputCmd {
        span: Span::dummy(),
        file: "/tmp/out.txt".into(),
    })]);
    struct V {
        visited: bool,
    }
    impl Visitor for V {
        fn visit_output(&mut self, _cmd: &OutputCmd) -> VisitResult {
            self.visited = true;
            VisitResult::Continue
        }
    }
    let mut v = V { visited: false };
    v.visit_mt_file(&file);
    assert!(v.visited);
}

// --- Visitor: visit_mt_file Stop path ---

#[test]
fn test_visitor_test_file_stop_on_statement() {
    let result = strict_parse("--echo first\n--echo second\n");
    struct StopVisitor {
        count: u32,
    }
    impl Visitor for StopVisitor {
        fn visit_statement(&mut self, _stmt: &Statement) -> VisitResult {
            self.count += 1;
            if self.count == 1 {
                VisitResult::Stop
            } else {
                VisitResult::Continue
            }
        }
    }
    let mut v = StopVisitor { count: 0 };
    let r = v.visit_mt_file(&result);
    assert_eq!(r, VisitResult::Stop);
    assert_eq!(v.count, 1);
}

// --- Visitor: if_block Stop on child ---

#[test]
fn test_visitor_if_block_stop_on_child() {
    let result = strict_parse("--if ($x)\n{\n--echo a\n--echo b\n}\n");
    struct StopVisitor {
        count: u32,
    }
    impl Visitor for StopVisitor {
        fn visit_statement(&mut self, _stmt: &Statement) -> VisitResult {
            self.count += 1;
            if self.count >= 2 {
                VisitResult::Stop
            } else {
                VisitResult::Continue
            }
        }
    }
    let mut v = StopVisitor { count: 0 };
    let r = v.visit_mt_file(&result);
    assert_eq!(r, VisitResult::Stop);
}

// --- Visitor: while_block Stop on child ---

#[test]
fn test_visitor_while_block_stop_on_child() {
    let result = strict_parse("--let $i= 5;\n--while ($i)\n{\n--echo a\n--echo b\n--echo c\n}\n");
    struct StopVisitor {
        count: u32,
    }
    impl Visitor for StopVisitor {
        fn visit_statement(&mut self, _stmt: &Statement) -> VisitResult {
            self.count += 1;
            // Stop inside the while body (4th statement is the 2nd child in while)
            if self.count >= 4 {
                VisitResult::Stop
            } else {
                VisitResult::Continue
            }
        }
    }
    let mut v = StopVisitor { count: 0 };
    let r = v.visit_mt_file(&result);
    assert_eq!(r, VisitResult::Stop);
    assert_eq!(v.count, 4);
}

// --- Visitor: visit_if returns Skip ---

#[test]
fn test_visitor_if_skip() {
    let result = strict_parse("--if ($x)\n{\n--echo a\n}\n");
    struct SkipVisitor {
        visited: bool,
    }
    impl Visitor for SkipVisitor {
        fn visit_if(&mut self, _block: &IfBlock) -> VisitResult {
            self.visited = true;
            VisitResult::Skip
        }
    }
    let mut v = SkipVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

// --- Visitor: visit_while returns Skip ---

#[test]
fn test_visitor_while_skip() {
    let result = strict_parse("--while ($i)\n{\n--dec $i;\n}\n");
    struct SkipVisitor {
        visited: bool,
    }
    impl Visitor for SkipVisitor {
        fn visit_while(&mut self, _block: &WhileBlock) -> VisitResult {
            self.visited = true;
            VisitResult::Skip
        }
    }
    let mut v = SkipVisitor { visited: false };
    v.visit_mt_file(&result);
    assert!(v.visited);
}

// --- MutVisitor tests ---

#[test]
fn test_mut_visitor_basic() {
    let mut result = strict_parse("--echo hello\n--let $x = 5;\n");
    struct Counter {
        count: u32,
    }
    impl MutVisitor for Counter {
        fn visit_statement_mut(&mut self, _stmt: &mut Statement) -> VisitResult {
            self.count += 1;
            VisitResult::Continue
        }
    }
    let mut v = Counter { count: 0 };
    v.visit_mt_file_mut(&mut result);
    assert_eq!(v.count, 2);
}

#[test]
fn test_mut_visitor_if_traversal() {
    let mut result = strict_parse("--if ($x)\n{\n--echo inside\n--dec $i;\n}\n");
    struct TraversalTracker {
        visited_if: bool,
        child_count: u32,
    }
    impl MutVisitor for TraversalTracker {
        fn visit_if_mut(&mut self, _block: &mut IfBlock) -> VisitResult {
            self.visited_if = true;
            VisitResult::Continue
        }
        fn visit_statement_mut(&mut self, _stmt: &mut Statement) -> VisitResult {
            self.child_count += 1;
            VisitResult::Continue
        }
    }
    let mut v = TraversalTracker {
        visited_if: false,
        child_count: 0,
    };
    v.visit_mt_file_mut(&mut result);
    assert!(v.visited_if);
    // 3 statements total: If itself, echo, dec
    assert_eq!(v.child_count, 3);
}

#[test]
fn test_mut_visitor_while_traversal() {
    let mut result = strict_parse("--while ($i)\n{\n--dec $i;\n}\n");
    struct TraversalTracker {
        visited_while: bool,
        child_count: u32,
    }
    impl MutVisitor for TraversalTracker {
        fn visit_while_mut(&mut self, _block: &mut WhileBlock) -> VisitResult {
            self.visited_while = true;
            VisitResult::Continue
        }
        fn visit_statement_mut(&mut self, _stmt: &mut Statement) -> VisitResult {
            self.child_count += 1;
            VisitResult::Continue
        }
    }
    let mut v = TraversalTracker {
        visited_while: false,
        child_count: 0,
    };
    v.visit_mt_file_mut(&mut result);
    assert!(v.visited_while);
    // 2: While itself + dec
    assert_eq!(v.child_count, 2);
}

#[test]
fn test_mut_visitor_if_skip() {
    let mut result = strict_parse("--if ($x)\n{\n--echo inside\n}\n");
    struct SkipMutVisitor {
        visited_if: bool,
        child_count: u32,
    }
    impl MutVisitor for SkipMutVisitor {
        fn visit_if_mut(&mut self, _block: &mut IfBlock) -> VisitResult {
            self.visited_if = true;
            VisitResult::Skip
        }
        fn visit_statement_mut(&mut self, _stmt: &mut Statement) -> VisitResult {
            self.child_count += 1;
            VisitResult::Continue
        }
    }
    let mut v = SkipMutVisitor {
        visited_if: false,
        child_count: 0,
    };
    v.visit_mt_file_mut(&mut result);
    assert!(v.visited_if);
    assert_eq!(v.child_count, 1);
}

#[test]
fn test_mut_visitor_if_stop() {
    let mut result = strict_parse("--if ($x)\n{\n--echo inside\n}\n--echo after\n");
    struct StopMutVisitor {
        count: u32,
    }
    impl MutVisitor for StopMutVisitor {
        fn visit_statement_mut(&mut self, _stmt: &mut Statement) -> VisitResult {
            self.count += 1;
            if self.count == 1 {
                VisitResult::Stop
            } else {
                VisitResult::Continue
            }
        }
    }
    let mut v = StopMutVisitor { count: 0 };
    let r = v.visit_mt_file_mut(&mut result);
    assert_eq!(r, VisitResult::Stop);
    assert_eq!(v.count, 1);
}

#[test]
fn test_mut_visitor_while_stop_on_child() {
    let mut result = strict_parse("--while ($i)\n{\n--dec $i;\n--echo x\n}\n");
    struct StopMutVisitor {
        count: u32,
    }
    impl MutVisitor for StopMutVisitor {
        fn visit_statement_mut(&mut self, _stmt: &mut Statement) -> VisitResult {
            self.count += 1;
            if self.count >= 2 {
                VisitResult::Stop
            } else {
                VisitResult::Continue
            }
        }
    }
    let mut v = StopMutVisitor { count: 0 };
    let r = v.visit_mt_file_mut(&mut result);
    assert_eq!(r, VisitResult::Stop);
}

#[test]
fn test_mut_visitor_modify_text() {
    let mut result = strict_parse("--echo hello\n");
    struct TextModifier;
    impl MutVisitor for TextModifier {
        fn visit_statement_mut(&mut self, stmt: &mut Statement) -> VisitResult {
            if let Statement::Echo(cmd) = stmt {
                cmd.text = "modified".into();
            }
            VisitResult::Continue
        }
    }
    let mut v = TextModifier;
    v.visit_mt_file_mut(&mut result);
    match &result.statements[0] {
        Statement::Echo(c) => assert_eq!(c.text.to_raw_string(), "modified"),
        other => panic!("expected Echo, got {:?}", other),
    }
}

// --- Statement::span() coverage ---

macro_rules! span_test {
    ($name:ident, $input:expr, $variant:pat) => {
        #[test]
        fn $name() {
            let result = strict_parse($input);
            assert!(!result.statements.is_empty());
            let span = result.statements[0].span();
            assert_eq!(span.line, 1);
            assert!(span.len > 0);
            match &result.statements[0] {
                $variant => {}
                other => panic!("unexpected: {:?}", other),
            }
        }
    };
}

span_test!(test_span_echo, "--echo hello\n", Statement::Echo(_));
span_test!(test_span_let, "--let $x = 5;\n", Statement::Let(_));
span_test!(
    test_span_error,
    "--error ER_PARSE_ERROR\n",
    Statement::Error(_)
);
span_test!(
    test_span_source,
    "--source test.inc\n",
    Statement::Source(_)
);
span_test!(test_span_skip, "--skip\n", Statement::Skip(_));
span_test!(test_span_die, "--die msg\n", Statement::Die(_));
span_test!(test_span_exit, "--exit\n", Statement::Exit(_));
span_test!(test_span_exec, "--exec ls\n", Statement::Exec(_));
span_test!(test_span_execw, "--execw ls\n", Statement::Execw(_));
span_test!(
    test_span_exec_in_background,
    "--exec_in_background sleep 5 &\n",
    Statement::ExecInBackground(_)
);
span_test!(test_span_sleep, "--sleep 1\n", Statement::Sleep(_));
span_test!(test_span_inc, "--inc $x\n", Statement::Inc(_));
span_test!(test_span_dec, "--dec $x\n", Statement::Dec(_));
span_test!(
    test_span_connect,
    "--connect(con1,localhost,root,,test)\n",
    Statement::Connect(_)
);
span_test!(
    test_span_connection,
    "--connection con1\n",
    Statement::Connection(_)
);
span_test!(
    test_span_disconnect,
    "--disconnect con1\n",
    Statement::Disconnect(_)
);
span_test!(
    test_span_change_user,
    "--change_user root,,test\n",
    Statement::ChangeUser(_)
);
span_test!(
    test_span_reset_connection,
    "--reset_connection\n",
    Statement::ResetConnection(_)
);
span_test!(test_span_query, "--query SELECT 1\n", Statement::Query(_));
span_test!(test_span_eval, "--eval SELECT 1\n", Statement::Eval(_));
span_test!(test_span_send, "--send SHOW STATUS\n", Statement::Send(_));
span_test!(
    test_span_send_eval,
    "--send_eval SELECT 1\n",
    Statement::SendEval(_)
);
span_test!(test_span_reap, "--reap\n", Statement::Reap(_));
span_test!(
    test_span_horizontal_results,
    "--horizontal_results\n",
    Statement::HorizontalResults(_)
);
span_test!(
    test_span_vertical_results,
    "--vertical_results\n",
    Statement::VerticalResults(_)
);
span_test!(
    test_span_replace_result,
    "--replace_result 1 2\n",
    Statement::ReplaceResult(_)
);
span_test!(
    test_span_replace_column,
    "--replace_column 1 \"a\" \"b\"\n",
    Statement::ReplaceColumn(_)
);
span_test!(
    test_span_replace_regex,
    "--replace_regex /a/b/\n",
    Statement::ReplaceRegex(_)
);
span_test!(
    test_span_sorted_result,
    "--sorted_result\n",
    Statement::SortedResult(_)
);
span_test!(
    test_span_partially_sorted_result,
    "--partially_sorted_result\n",
    Statement::PartiallySortedResult(_)
);
span_test!(
    test_span_replace_numeric_round,
    "--replace_numeric_round 2\n",
    Statement::ReplaceNumericRound(_)
);
span_test!(
    test_span_toggle,
    "--disable_warnings\n",
    Statement::Toggle(_)
);
span_test!(
    test_span_delimiter,
    "--delimiter //\n",
    Statement::Delimiter(_)
);
span_test!(
    test_span_write_file,
    "--write_file /tmp/f\ncontent\nEOF\n",
    Statement::WriteFile(_)
);
span_test!(
    test_span_append_file,
    "--append_file /tmp/f\ncontent\nEOF\n",
    Statement::AppendFile(_)
);
span_test!(
    test_span_remove_file,
    "--remove_file /tmp/f\n",
    Statement::RemoveFile(_)
);
span_test!(
    test_span_remove_files_wildcard,
    "--remove_files_wildcard /tmp/*.x\n",
    Statement::RemoveFilesWildcard(_)
);
span_test!(
    test_span_copy_file,
    "--copy_file /tmp/a /tmp/b\n",
    Statement::CopyFile(_)
);
span_test!(
    test_span_move_file,
    "--move_file /tmp/a /tmp/b\n",
    Statement::MoveFile(_)
);
span_test!(test_span_mkdir, "--mkdir /tmp/d\n", Statement::Mkdir(_));
span_test!(test_span_rmdir, "--rmdir /tmp/d\n", Statement::Rmdir(_));
span_test!(test_span_chmod, "--chmod 644 /tmp/f\n", Statement::Chmod(_));
span_test!(
    test_span_diff_files,
    "--diff_files /tmp/a /tmp/b\n",
    Statement::DiffFiles(_)
);
span_test!(
    test_span_file_exists,
    "--file_exists /tmp/f\n",
    Statement::FileExists(_)
);
span_test!(
    test_span_cat_file,
    "--cat_file /tmp/f\n",
    Statement::CatFile(_)
);
span_test!(
    test_span_list_files,
    "--list_files /tmp\n",
    Statement::ListFiles(_)
);
span_test!(
    test_span_shutdown_server,
    "--shutdown_server\n",
    Statement::ShutdownServer(_)
);
span_test!(test_span_send_quit, "--send_quit\n", Statement::SendQuit(_));
span_test!(
    test_span_send_shutdown,
    "--send_shutdown\n",
    Statement::SendShutdown(_)
);
span_test!(
    test_span_perl,
    "--perl\nprint 1;\nEOF\n",
    Statement::Perl(_)
);
span_test!(test_span_if, "--if ($x)\n{\n}\n", Statement::If(_));
span_test!(
    test_span_while,
    "--while ($i)\n{\n--dec $i;\n}\n",
    Statement::While(_)
);
span_test!(test_span_end, "--end\n", Statement::End(_));
span_test!(test_span_sql, "SELECT 1;\n", Statement::Sql(_));
span_test!(
    test_span_character_set,
    "--character_set utf8\n",
    Statement::CharacterSet(_)
);
span_test!(test_span_system, "--system ls\n", Statement::System(_));
span_test!(
    test_span_real_sleep,
    "--real_sleep 0.5\n",
    Statement::RealSleep(_)
);
span_test!(
    test_span_require,
    "--require check\n",
    Statement::Require(_)
);
span_test!(
    test_span_lowercase_result,
    "--lowercase_result\n",
    Statement::LowercaseResult(_)
);
span_test!(
    test_span_sync_slave_with_master,
    "--sync_slave_with_master\n",
    Statement::SyncSlaveWithMaster(_)
);
span_test!(
    test_span_copy_files_wildcard,
    "--copy_files_wildcard /tmp/*.a /tmp/d/\n",
    Statement::CopyFilesWildcard(_)
);

// --- Statement::Empty span ---

#[test]
fn test_span_empty_statement() {
    let span = Statement::Empty.span();
    assert_eq!(span.line, 1);
    assert_eq!(span.offset, 0);
    assert_eq!(span.len, 0);
}

// --- Statement::span for non-parseable variants ---

#[test]
fn test_span_assert_constructed() {
    let stmt = Statement::Assert(AssertCmd {
        span: Span::new(3, 5, 100, 10),
        expression: Expr::Integer(1),
    });
    let span = stmt.span();
    assert_eq!(span.line, 3);
}

#[test]
fn test_span_output_constructed() {
    let stmt = Statement::Output(OutputCmd {
        span: Span::new(7, 2, 50, 8),
        file: "test".into(),
    });
    let span = stmt.span();
    assert_eq!(span.line, 7);
}

// --- Span tests ---

#[test]
fn test_span_new() {
    let span = Span::new(5, 10, 100, 20);
    assert_eq!(span.line, 5);
    assert_eq!(span.column, 10);
    assert_eq!(span.offset, 100);
    assert_eq!(span.len, 20);
}

#[test]
fn test_span_display() {
    let span = Span::new(3, 7, 50, 10);
    assert_eq!(format!("{}", span), "line 3 col 7");
}

#[test]
fn test_span_merge_same_line() {
    let a = Span::new(1, 5, 10, 5);
    let b = Span::new(1, 15, 20, 5);
    let merged = a.merge(&b);
    assert_eq!(merged.line, 1);
    assert_eq!(merged.column, 5);
    assert_eq!(merged.offset, 10);
}

#[test]
fn test_span_merge_different_lines() {
    let a = Span::new(2, 5, 10, 5);
    let b = Span::new(5, 3, 30, 5);
    let merged = a.merge(&b);
    assert_eq!(merged.line, 2);
    assert_eq!(merged.column, 5);
    assert_eq!(merged.offset, 10);
}

#[test]
fn test_span_merge_reverse_lines() {
    let a = Span::new(5, 3, 30, 5);
    let b = Span::new(2, 5, 10, 5);
    let merged = a.merge(&b);
    assert_eq!(merged.line, 2);
    assert_eq!(merged.column, 5);
}

#[test]
fn test_span_dummy() {
    let span = Span::dummy();
    assert_eq!(span.line, 1);
    assert_eq!(span.column, 1);
    assert_eq!(span.offset, 0);
    assert_eq!(span.len, 0);
}

// --- InterpolatedText conversion tests ---

#[test]
fn test_interpolated_text_from_string() {
    let text: InterpolatedText = "hello $world".to_string().into();
    assert_eq!(text.to_raw_string(), "hello $world");
}

#[test]
fn test_interpolated_text_into_string() {
    let text = InterpolatedText::from("hello $world");
    let s: String = text.into();
    assert_eq!(s, "hello $world");
}

#[test]
fn test_interpolated_text_escape_backslash() {
    let text = InterpolatedText::from("\\a");
    assert_eq!(text.to_raw_string(), "\\a");
}

// --- Parser error paths ---

#[test]
fn test_delimiter_no_arg_error() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--delimiter\n", config);
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("delimiter requires an argument"));
}

#[test]
fn test_assert_v57_error() {
    let config = ParserConfig::new(MysqlVersion::V57);
    let result = parse("--assert(`SELECT 1`)\n", config);
    assert!(result.is_err());
}

#[test]
fn test_let_invalid_no_equals() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--let $nonsense\n", config);
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("invalid let syntax"));
}

#[test]
fn test_let_invalid_empty_var() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--let = 5\n", config);
    assert!(result.is_err());
}

#[test]
fn test_expr_invalid_no_dollar() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--expr x = 1\n", config);
    assert!(result.is_err());
}

#[test]
fn test_expr_invalid_no_equals() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--expr $x\n", config);
    assert!(result.is_err());
}

#[test]
fn test_connect_without_parens() {
    let result = strict_parse("connect con1;\n");
    match &result.statements[0] {
        Statement::Connect(c) => {
            assert_eq!(
                c.name.as_ref().map(|t| t.to_raw_string()),
                Some("con1".to_string())
            );
        }
        other => panic!("expected Connect, got {:?}", other),
    }
}

#[test]
fn test_replace_regex_dollar_var() {
    let result = strict_parse("--replace_regex $pattern\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "$pattern");
            assert!(c.replacement.is_empty());
            assert!(c.flags.is_none());
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

#[test]
fn test_replace_regex_s_prefix() {
    let result = strict_parse("--replace_regex s/old/new/\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "old");
            assert_eq!(c.replacement, "new");
            assert!(c.flags.is_none());
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

#[test]
fn test_replace_regex_with_flags() {
    let result = strict_parse("--replace_regex s/old/new/i\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.flags, Some("i".to_string()));
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

#[test]
fn test_replace_regex_empty_replacement() {
    let result = strict_parse("--replace_regex s/old//\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "old");
            assert_eq!(c.replacement, "");
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

#[test]
fn test_replace_regex_no_closing_sep() {
    let result = strict_parse("--replace_regex /pattern_only\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "pattern_only");
            assert_eq!(c.replacement, "");
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

#[test]
fn test_replace_regex_invalid_syntax() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--replace_regex invalid\n", config);
    assert!(result.is_err());
}

// --- replace_regex MariaDB paired delimiters ---

fn mariadb_parse(input: &str) -> MTFile {
    let config = ParserConfig::new(MysqlVersion::MariaDB);
    parse(input, config).expect("parse failed")
}

#[test]
fn test_mariadb_replace_regex_paren() {
    let result = mariadb_parse("--replace_regex (/some/path)</another/path>\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "/some/path");
            assert_eq!(c.replacement, "/another/path");
            assert!(c.flags.is_none());
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

#[test]
fn test_mariadb_replace_regex_bracket() {
    let result = mariadb_parse("--replace_regex [pattern][replacement]\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "pattern");
            assert_eq!(c.replacement, "replacement");
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

#[test]
fn test_mariadb_replace_regex_brace_with_escape() {
    // \} escapes the closing brace, \/ escapes the slash in replacement
    let result = mariadb_parse("--replace_regex {pat\\}tern}/replace\\/ment/i\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "pat}tern");
            assert_eq!(c.replacement, "replace/ment");
            assert_eq!(c.flags, Some("i".to_string()));
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

#[test]
fn test_mariadb_replace_regex_single_char_delimiter() {
    let result = mariadb_parse("--replace_regex !old!new!\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "old");
            assert_eq!(c.replacement, "new");
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

#[test]
fn test_mariadb_replace_regex_same_paired_delimiter() {
    // Both pattern and replacement use () — after pattern's ), the second ( sets new pair
    let result = mariadb_parse("--replace_regex (pat\\)tern)(new)i\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "pat)tern");
            assert_eq!(c.replacement, "new");
            assert_eq!(c.flags, Some("i".to_string()));
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

#[test]
fn test_mariadb_replace_regex_slash_delimiter() {
    let result = mariadb_parse("--replace_regex /old/new/\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "old");
            assert_eq!(c.replacement, "new");
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

#[test]
fn test_mariadb_replace_regex_slash_with_escape() {
    let result = mariadb_parse("--replace_regex /path\\/to/new/i\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "path/to");
            assert_eq!(c.replacement, "new");
            assert_eq!(c.flags, Some("i".to_string()));
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

#[test]
fn test_mariadb_replace_regex_mixed_paired() {
    // Pattern with (), replacement with <> — the </> delimiters
    let result = mariadb_parse("--replace_regex (pattern)<replacement>\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "pattern");
            assert_eq!(c.replacement, "replacement");
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

#[test]
fn test_mariadb_replace_regex_empty_replacement() {
    let result = mariadb_parse("--replace_regex /pattern//\n");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "pattern");
            assert!(c.replacement.is_empty());
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

// MySQL should reject paired delimiter syntax

#[test]
fn test_mysql_rejects_paren_delimiter() {
    let config = ParserConfig::new(MysqlVersion::V80);
    let result = parse("--replace_regex (/pattern)/replacement/\n", config);
    assert!(result.is_err());
}

#[test]
fn test_mysql_rejects_brace_delimiter() {
    let config = ParserConfig::new(MysqlVersion::V80);
    let result = parse("--replace_regex {pattern}/replacement/\n", config);
    assert!(result.is_err());
}

#[test]
fn test_mysql_rejects_arbitrary_delimiter() {
    let config = ParserConfig::new(MysqlVersion::V80);
    let result = parse("--replace_regex !old!new!\n", config);
    assert!(result.is_err());
}

// MySQL still accepts s/ and / syntax

#[test]
fn test_mysql_still_accepts_s_prefix() {
    let config = ParserConfig::new(MysqlVersion::V80);
    let result = parse("--replace_regex s/old/new/\n", config).expect("parse failed");
    match &result.statements[0] {
        Statement::ReplaceRegex(c) => {
            assert_eq!(c.pattern, "old");
            assert_eq!(c.replacement, "new");
        }
        other => panic!("expected ReplaceRegex, got {:?}", other),
    }
}

// --- enable_prepare_warnings / disable_prepare_warnings ---

#[test]
fn test_enable_prepare_warnings() {
    let result = strict_parse("--enable_prepare_warnings\n");
    match &result.statements[0] {
        Statement::Toggle(c) => {
            assert!(matches!(c.kind, ToggleKind::PrepareWarnings));
            assert!(c.enabled);
        }
        other => panic!("expected Toggle, got {:?}", other),
    }
}

#[test]
fn test_disable_prepare_warnings() {
    let result = strict_parse("--disable_prepare_warnings\n");
    match &result.statements[0] {
        Statement::Toggle(c) => {
            assert!(matches!(c.kind, ToggleKind::PrepareWarnings));
            assert!(!c.enabled);
        }
        other => panic!("expected Toggle, got {:?}", other),
    }
}

#[test]
fn test_prepare_warnings_toggle_once() {
    let result = strict_parse("--disable_prepare_warnings ONCE\n");
    match &result.statements[0] {
        Statement::Toggle(c) => {
            assert!(matches!(c.kind, ToggleKind::PrepareWarnings));
            assert!(!c.enabled);
            assert!(c.once);
        }
        other => panic!("expected Toggle, got {:?}", other),
    }
}

// --- parse_bytes ---

#[test]
fn test_parse_bytes_basic() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let input = b"--echo hello\n";
    let result = parse_bytes(input, config).expect("parse failed");
    assert_eq!(result.statements.len(), 1);
}

// --- Unterminated blocks ---

#[test]
fn test_unterminated_write_file() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--write_file /tmp/f\ncontent\n", config);
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("unterminated"));
}

#[test]
fn test_unterminated_perl_block() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--perl\nprint 1;\n", config);
    assert!(result.is_err());
}

// --- Flow: condition with whitespace ---

#[test]
fn test_condition_trimmed_parens() {
    let result = strict_parse("--if (  $x == 1  )\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => {
            assert!(matches!(&b.condition, Expr::Comparison { .. }));
        }
        other => panic!("expected If, got {:?}", other),
    }
}

// --- Flow: comparison with query rhs ---

#[test]
fn test_condition_comparison_query_rhs() {
    let result = strict_parse("--if ($x == `SELECT 1`)\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => match &b.condition {
            Expr::Comparison { right, .. } => {
                assert!(matches!(right.as_ref(), ComparisonRhs::Query(_)));
            }
            other => panic!("expected Comparison, got {:?}", other),
        },
        other => panic!("expected If, got {:?}", other),
    }
}

// --- MariaDB expression parsing edge cases ---

#[test]
fn test_mariadb_dollar_paren_with_spaces() {
    let result = parse_mariadb("--if ($(1 + 2))\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => {
            assert!(matches!(&b.condition, Expr::MariaDBClosure { .. }));
        }
        other => panic!("expected If with MariaDB expr, got {:?}", other),
    }
}

#[test]
fn test_mariadb_backtick_in_logical() {
    let result = parse_mariadb("--if ($x && `SELECT 1`)\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => {
            assert!(matches!(&b.condition, Expr::MariaDBLogical { .. }));
        }
        other => panic!("expected MariaDBLogical, got {:?}", other),
    }
}

#[test]
fn test_mariadb_paren_in_logical() {
    let result = parse_mariadb("--if ($x || ($y && $z))\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => {
            assert!(matches!(&b.condition, Expr::MariaDBLogical { .. }));
        }
        other => panic!("expected MariaDBLogical, got {:?}", other),
    }
}

// --- Flow: edge cases ---

#[test]
fn test_condition_only_parens() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--if ()\n{\n}\n", config);
    assert!(result.is_err());
}

// --- Version tests ---

#[test]
fn test_version_assert_v57() {
    assert!(!MysqlVersion::V57.has_assert());
}

#[test]
fn test_version_assert_v80() {
    assert!(MysqlVersion::V80.has_assert());
}

#[test]
fn test_version_assert_compatible() {
    assert!(MysqlVersion::Compatible.has_assert());
}

#[test]
fn test_version_command_system_on_v80() {
    assert!(!MysqlVersion::V80.has_command("system"));
}

#[test]
fn test_version_command_system_on_compatible() {
    assert!(MysqlVersion::Compatible.has_command("system"));
}

#[test]
fn test_version_command_echo_all_versions() {
    assert!(MysqlVersion::V57.has_command("echo"));
    assert!(MysqlVersion::V80.has_command("echo"));
    assert!(MysqlVersion::V84.has_command("echo"));
    assert!(MysqlVersion::V97.has_command("echo"));
}

#[test]
fn test_version_mariadb_has_all() {
    assert!(MysqlVersion::MariaDB.has_command("system"));
    assert!(MysqlVersion::MariaDB.has_command("require"));
    assert!(MysqlVersion::MariaDB.has_assert());
    assert!(MysqlVersion::MariaDB.has_command("echo"));
}

// --- ParserConfig default ---

#[test]
fn test_parser_config_default() {
    let config = ParserConfig::default();
    assert_eq!(config.version, MysqlVersion::Compatible);
}

// --- Output command (constructed directly, not parsed) ---

#[test]
fn test_output_command_constructed() {
    let stmt = Statement::Output(OutputCmd {
        span: Span::dummy(),
        file: "/tmp/result.txt".into(),
    });
    match &stmt {
        Statement::Output(c) => {
            assert_eq!(c.file.to_raw_string(), "/tmp/result.txt");
        }
        other => panic!("expected Output, got {:?}", other),
    }
}

// --- Error with no codes ---

#[test]
fn test_error_no_codes() {
    let result = strict_parse("--error\n");
    match &result.statements[0] {
        Statement::Error(c) => {
            assert!(c.error_codes.is_empty());
        }
        other => panic!("expected Error, got {:?}", other),
    }
}

// --- Change user with empty args ---

#[test]
fn test_change_user_empty() {
    let result = strict_parse("--change_user\n");
    match &result.statements[0] {
        Statement::ChangeUser(c) => {
            assert!(c.user.is_none());
            assert!(c.password.is_none());
            assert!(c.database.is_none());
        }
        other => panic!("expected ChangeUser, got {:?}", other),
    }
}

// --- Bare if with content after brace ---

#[test]
fn test_bare_if_content_after_brace() {
    let result = strict_parse("if ($x) { --echo inline; }\n");
    match &result.statements[0] {
        Statement::If(b) => {
            assert_eq!(b.body.len(), 1);
        }
        other => panic!("expected If, got {:?}", other),
    }
}

// --- While block ---

#[test]
fn test_while_block() {
    let result = strict_parse("--let $i= 5;\n--while ($i)\n{\n--dec $i;\n}\n");
    assert!(matches!(&result.statements[1], Statement::While(_)));
}

// --- Bare write_file/append_file (delimiter-aware) ---

#[test]
fn test_bare_write_file_with_delimiter() {
    let result = strict_parse("write_file \"/tmp/test.txt\"\ncontent\nEOF\n");
    match &result.statements[0] {
        Statement::WriteFile(c) => {
            assert_eq!(c.filename.to_raw_string(), "/tmp/test.txt");
            assert_eq!(c.end_marker, "EOF");
            assert!(c.content.to_raw_string().contains("content"));
        }
        other => panic!("expected WriteFile, got {:?}", other),
    }
}

#[test]
fn test_bare_append_file_block() {
    let result = strict_parse("append_file /tmp/f\nmore content\nEOF\n");
    match &result.statements[0] {
        Statement::AppendFile(c) => {
            assert!(c.content.to_raw_string().contains("more content"));
        }
        other => panic!("expected AppendFile, got {:?}", other),
    }
}

// --- Unterminated flow control ---

#[test]
fn test_unterminated_if_no_brace() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--if ($x)\n--echo test\n", config);
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("unterminated"));
}

#[test]
fn test_unterminated_bare_if_no_brace() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("if ($x)\n--echo test\n", config);
    assert!(result.is_err());
}

// --- Inline if/while with empty body ---

#[test]
fn test_inline_if_empty_body() {
    let result = strict_parse("if ($x) {}\n");
    match &result.statements[0] {
        Statement::If(b) => {
            assert_eq!(b.body.len(), 1);
            assert!(matches!(&b.body[0], Statement::Empty));
        }
        other => panic!("expected If, got {:?}", other),
    }
}

// --- Inline if with error fallback body ---

#[test]
fn test_inline_if_error_fallback_body() {
    // Body text that doesn't match any command — falls back to SQL
    let result = strict_parse("if ($x) { random_stuff; }\n");
    match &result.statements[0] {
        Statement::If(b) => {
            assert_eq!(b.body.len(), 1);
            assert!(matches!(&b.body[0], Statement::Sql(_)));
        }
        other => panic!("expected If, got {:?}", other),
    }
}

// --- strip_trailing_delimiter_quoted edge cases ---

#[test]
fn test_strip_delimiter_no_quotes() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    // Simple case: no quotes, strip delimiter normally
    let result = parse("write_file /tmp/test.txt\ncontent\nEOF\n", config).expect("parse failed");
    match &result.statements[0] {
        Statement::WriteFile(c) => {
            assert_eq!(c.filename.to_raw_string(), "/tmp/test.txt");
        }
        other => panic!("expected WriteFile, got {:?}", other),
    }
}

#[test]
fn test_write_file_unclosed_quotes() {
    // Unclosed quotes — delimiter not stripped
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("write_file \"/tmp/test\ncontent\nEOF\n", config).expect("parse failed");
    match &result.statements[0] {
        Statement::WriteFile(c) => {
            // Unclosed quote means delimiter not stripped, but filename parsing still works
            assert!(c.filename.to_raw_string().contains("test"));
        }
        other => panic!("expected WriteFile, got {:?}", other),
    }
}

// --- parse_file_args empty filename ---

#[test]
fn test_write_file_empty_filename() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--write_file\ncontent\nEOF\n", config);
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("filename must not be empty"));
}

// --- Let with open backtick ---

#[test]
fn test_let_open_backtick() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--let $x = `query\n", config);
    // Open backtick without close — parsed as literal (stripped trailing newline by parse)
    let result = result.expect("parse failed");
    match &result.statements[0] {
        Statement::Let(c) => {
            assert_eq!(c.variable, "x");
            match &c.value {
                LetValue::Literal(s) => assert!(s.contains('`')),
                other => panic!("expected Literal, got {:?}", other),
            }
        }
        other => panic!("expected Let, got {:?}", other),
    }
}

// --- Inc without $ prefix ---

#[test]
fn test_inc_no_dollar() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--inc xyz\n", config);
    // Should parse — inc gets empty variable name
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("requires a variable"));
}

// --- Dec without $ prefix ---

#[test]
fn test_dec_no_dollar() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--dec xyz\n", config);
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("requires a variable"));
}

// --- Delimiter terminated with comment ---

#[test]
fn test_delimiter_terminated_with_trailing_comment() {
    let result = strict_parse("SELECT 1;\n# comment\n");
    assert_eq!(result.statements.len(), 2);
    assert!(matches!(&result.statements[0], Statement::Sql(_)));
    assert!(matches!(&result.statements[1], Statement::Comment(_)));
}

// --- Flow condition: parse_rhs with integer ---

#[test]
fn test_condition_comparison_integer_rhs_direct() {
    let result = strict_parse("--if ($x == 42)\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => match &b.condition {
            Expr::Comparison { right, .. } => {
                assert!(matches!(right.as_ref(), ComparisonRhs::Integer(42)));
            }
            other => panic!("expected Comparison, got {:?}", other),
        },
        other => panic!("expected If, got {:?}", other),
    }
}

// --- Flow condition edge cases ---

#[test]
fn test_condition_open_paren_no_close() {
    // Open paren but no closing — falls back to full input
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--if ($x == 1\n{\n}\n", config);
    // May parse or fail depending on condition handling
    // The important thing is it doesn't panic
    let _ = result;
}

#[test]
fn test_condition_no_paren() {
    // No paren at all — condition is the full text
    let result = strict_parse("--if $x\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => {
            assert!(matches!(&b.condition, Expr::Variable(_)));
        }
        other => panic!("expected If, got {:?}", other),
    }
}

#[test]
fn test_condition_backtick_no_close() {
    // Backtick without closing — falls through to string comparison
    let result = strict_parse("--if ($x == `query)\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => match &b.condition {
            Expr::Comparison { right, .. } => {
                assert!(matches!(right.as_ref(), ComparisonRhs::String(_)));
            }
            other => panic!("expected Comparison, got {:?}", other),
        },
        other => panic!("expected If, got {:?}", other),
    }
}

#[test]
fn test_mariadb_dollar_paren_no_close() {
    // $( without closing ) — results in error
    let config = ParserConfig::new(MysqlVersion::MariaDB);
    let result = parse("--if ($(1 + 2\n{\n}\n", config);
    assert!(result.is_err());
}

#[test]
fn test_condition_query_no_close_backtick() {
    // Backtick query without closing backtick at top level
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--if (`SELECT 1\n{\n}\n", config);
    // Should not panic — may error
    let _ = result;
}

// --- find_logical_op edge cases ---

#[test]
fn test_mariadb_or_at_end() {
    let result = parse_mariadb("--if ($a || $b)\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => {
            assert!(matches!(&b.condition, Expr::MariaDBLogical { .. }));
        }
        other => panic!("expected MariaDBLogical, got {:?}", other),
    }
}

#[test]
fn test_mariadb_and_no_op() {
    // No logical operator — should be parsed as variable
    let result = parse_mariadb("--if ($x)\n{\n}\n");
    match &result.statements[0] {
        Statement::If(b) => {
            assert!(matches!(&b.condition, Expr::Variable(_)));
        }
        other => panic!("expected Variable, got {:?}", other),
    }
}

// --- Flow: parse_inner_expr with query that has no close ---

#[test]
fn test_condition_backtick_query_in_inner_no_close() {
    let config = ParserConfig::new(MysqlVersion::Compatible);
    let result = parse("--if (`SELECT *\n{\n}\n", config);
    assert!(result.is_err());
}

// --- Comprehensive visitor coverage: exercise all visit_statement_inner match arms ---

#[test]
fn test_visitor_exhaustive() {
    let mut input = String::new();
    // All commands that produce distinct Statement variants
    input.push_str("--echo hello\n");
    input.push_str("--let $x = 5;\n");
    input.push_str("--error ER_PARSE_ERROR\n");
    input.push_str("--source test.inc\n");
    input.push_str("--skip test\n");
    input.push_str("--die msg\n");
    input.push_str("--exit\n");
    input.push_str("--exec ls\n");
    input.push_str("--execw ls\n");
    input.push_str("--exec_in_background sleep 5 &\n");
    input.push_str("--sleep 1\n");
    input.push_str("--inc $x\n");
    input.push_str("--dec $x\n");
    input.push_str("--expr $x = 1 + 2\n");
    input.push_str("--connect(con1,localhost,root,,test)\n");
    input.push_str("--connection con1\n");
    input.push_str("--disconnect con1\n");
    input.push_str("--change_user root,,test\n");
    input.push_str("--reset_connection\n");
    input.push_str("--query SELECT 1\n");
    input.push_str("--eval SELECT $x\n");
    input.push_str("--send SHOW STATUS\n");
    input.push_str("--send_eval SELECT 1\n");
    input.push_str("--reap\n");
    input.push_str("--horizontal_results\n");
    input.push_str("--vertical_results\n");
    input.push_str("--replace_result 1 2\n");
    input.push_str("--replace_column 1 \"a\" \"b\"\n");
    input.push_str("--replace_regex /a/b/\n");
    input.push_str("--sorted_result\n");
    input.push_str("--partially_sorted_result\n");
    input.push_str("--replace_numeric_round 2\n");
    input.push_str("--disable_warnings\n");
    input.push_str("--delimiter //\n");
    input.push_str("--write_file /tmp/wf\ncontent\nEOF\n");
    input.push_str("--append_file /tmp/af\ncontent\nEOF\n");
    input.push_str("--remove_file /tmp/f\n");
    input.push_str("--remove_files_wildcard /tmp/*.x\n");
    input.push_str("--copy_file /tmp/a /tmp/b\n");
    input.push_str("--move_file /tmp/a /tmp/b\n");
    input.push_str("--mkdir /tmp/d\n");
    input.push_str("--rmdir /tmp/d\n");
    input.push_str("--chmod 644 /tmp/f\n");
    input.push_str("--diff_files /tmp/a /tmp/b\n");
    input.push_str("--file_exists /tmp/f\n");
    input.push_str("--cat_file /tmp/f\n");
    input.push_str("--list_files /tmp *.txt\n");
    input.push_str("--shutdown_server\n");
    input.push_str("--send_quit\n");
    input.push_str("--send_shutdown\n");
    input.push_str("--end\n");
    input.push_str("--perl\nprint 1;\nEOF\n");
    input.push_str("# comment\n");
    input.push_str("--character_set utf8\n");
    input.push_str("--system ls\n");
    input.push_str("--real_sleep 0.5\n");
    input.push_str("--require check\n");
    input.push_str("--lowercase_result\n");
    input.push_str("--sync_slave_with_master\n");
    input.push_str("--copy_files_wildcard /tmp/*.a /tmp/d/\n");
    input.push_str("--if ($x)\n{\n--echo in_if\n}\n");
    input.push_str("--let $i = 1;\n--while ($i)\n{\n--dec $i;\n}\n");
    // SQL fallback
    input.push_str("SELECT 1;\n");

    let result = strict_parse(&input);

    // Construct with Assert and Output (not parseable)
    let mut all_stmts: Vec<Statement> = result.statements.clone();
    all_stmts.push(Statement::Output(OutputCmd {
        span: Span::dummy(),
        file: "/tmp/out".into(),
    }));
    all_stmts.push(Statement::Assert(AssertCmd {
        span: Span::dummy(),
        expression: Expr::Integer(1),
    }));
    all_stmts.push(Statement::Empty);

    let file = MTFile::new(all_stmts);

    struct ExhaustiveVisitor {
        count: u32,
        leave_count: u32,
    }
    impl Visitor for ExhaustiveVisitor {
        fn visit_statement(&mut self, _stmt: &Statement) -> VisitResult {
            self.count += 1;
            VisitResult::Continue
        }
        fn leave_statement(&mut self, _stmt: &Statement) {
            self.leave_count += 1;
        }
    }
    let mut v = ExhaustiveVisitor {
        count: 0,
        leave_count: 0,
    };
    v.visit_mt_file(&file);
    // All statements should be visited
    assert!(v.count > 60);
    assert_eq!(v.count, v.leave_count);
}

// --- MutVisitor exhaustive ---

#[test]
fn test_mut_visitor_exhaustive() {
    let mut input = String::new();
    input.push_str("--echo hello\n");
    input.push_str("--let $x = 5;\n");
    input.push_str("--if ($x)\n{\n--echo in_if\n--dec $y;\n}\n");
    input.push_str("--while ($i)\n{\n--dec $i;\n}\n");
    input.push_str("SELECT 1;\n");

    let mut result = strict_parse(&input);

    // Use default trait methods (no override) to cover default paths
    struct DefaultV;
    impl MutVisitor for DefaultV {}
    let mut v = DefaultV;
    v.visit_mt_file_mut(&mut result);
}

// MutVisitor with overridden visit_statement_mut only
#[test]
fn test_mut_visitor_default_inner() {
    let mut result = strict_parse("--echo hello\n--let $x = 1;\nSELECT 1;\n");
    struct Counter {
        count: u32,
    }
    impl MutVisitor for Counter {
        fn visit_statement_mut(&mut self, _stmt: &mut Statement) -> VisitResult {
            self.count += 1;
            VisitResult::Continue
        }
    }
    let mut v = Counter { count: 0 };
    v.visit_mt_file_mut(&mut result);
    assert_eq!(v.count, 3);
}

// MutVisitor: Stop in if child triggers Stop path (L30)
#[test]
fn test_mut_visitor_stop_in_if_child() {
    let mut result = strict_parse("--if ($x)\n{\n--echo a\n--echo b\n}\n");
    struct StopOnEcho;
    impl MutVisitor for StopOnEcho {
        fn visit_statement_mut(&mut self, stmt: &mut Statement) -> VisitResult {
            // Stop on Echo — the default visit_statement_inner_mut will
            // recurse into If and call visit_statement_mut on children
            if let Statement::Echo(_) = stmt {
                return VisitResult::Stop;
            }
            VisitResult::Continue
        }
    }
    let mut v = StopOnEcho;
    let r = v.visit_mt_file_mut(&mut result);
    assert_eq!(r, VisitResult::Stop);
}

// MutVisitor: Stop in while child triggers Stop path (L45)
#[test]
fn test_mut_visitor_stop_in_while_child() {
    let mut result = strict_parse("--while ($i)\n{\n--echo a\n--echo b\n}\n");
    struct StopOnEcho;
    impl MutVisitor for StopOnEcho {
        fn visit_statement_mut(&mut self, stmt: &mut Statement) -> VisitResult {
            if let Statement::Echo(_) = stmt {
                return VisitResult::Stop;
            }
            VisitResult::Continue
        }
    }
    let mut v = StopOnEcho;
    let r = v.visit_mt_file_mut(&mut result);
    assert_eq!(r, VisitResult::Stop);
}
