# mtparser

Rust 编写的 MySQL mysqltest 测试文件解析器，将 `.test` / `.inc` 文件解析为类型化的 AST。

## 特性

- **完整的命令支持** — 覆盖 mysqltest 全部 60+ 命令（echo、let、connect、query、if/while 等）
- **版本感知** — 支持 MySQL 5.7 / 8.0 / 8.4 / 9.7 四个版本，自动处理命令集差异
- **变量插值** — `InterpolatedText` 类型保留 `$variable` 引用结构，支持精确的变量分析
- **Visitor 模式** — 提供不可变和可变 AST 遍历 trait，支持自定义访问逻辑
- **基于 winnow** — 使用 winnow 组合子库进行 token 级解析，`dispatch!` 宏用于运算符分发

## 版本差异

| 版本 | 说明 |
|------|------|
| MySQL 5.7 | 命令超集，额外支持 `require`、`system`、`real_sleep`、`disable_parsing`/`enable_parsing` |
| MySQL 8.0+ | 新增 `assert` 命令 |
| MySQL 8.4 / 9.7 | 命令集与 8.0 基本一致 |

Strict 模式下版本不匹配会报错，Lenient 模式下降级为 SQL 语句。

## 用法

### 解析测试文件

```rust
use mtparser::parser::{parse, ParserConfig};
use mtparser::version::MysqlVersion;

let config = ParserConfig::new(MysqlVersion::V80, mtparser::error::ParseMode::Strict);
let test_file = parse(include_str!("example.test"), config)?;
println!("Parsed {} statements", test_file.statements.len());
```

### Visitor 遍历

```rust
use mtparser::visitor::{Visitor, VisitResult};

struct VarCollector {
    variables: Vec<String>,
}

impl Visitor for VarCollector {
    fn visit_echo(&mut self, cmd: &mtparser::ast::commands::EchoCmd) -> VisitResult {
        self.variables.extend(cmd.text.variable_names().into_iter().map(String::from));
        VisitResult::Continue
    }
}
```

### 版本感知解析

```rust
use mtparser::parser::{parse_bytes, ParserConfig};
use mtparser::version::MysqlVersion;

// 5.7 模式：支持 system、real_sleep 等 5.7 专属命令
let config_57 = ParserConfig::new(MysqlVersion::V57, mtparser::error::ParseMode::Strict);

// 8.0 模式：require、system 等命令会报版本不匹配错误
let config_80 = ParserConfig::new(MysqlVersion::V80, mtparser::error::ParseMode::Strict);
```

## CLI

```bash
# 单文件解析（输出 AST）
mtparser path/to/test.test

# 目录分析（统计解析成功率）
mtparser --analyze mysql-test/t/ --version 8.0

# 版本错误扫描
mtparser --version-errors mysql-test/t/ --version 8.0
```

## 解析的命令

<details>
<summary>完整命令列表（60+）</summary>

**输出**: echo, source
**流程控制**: if, while, end, assert
**变量**: let, inc, dec, expr
**连接**: connect, connection, disconnect, change_user, reset_connection
**SQL 执行**: query, eval, send, send_eval, reap
**结果格式化**: horizontal_results, vertical_results, sorted_result, replace_result, replace_column, replace_regex, partially_sorted_result, replace_numeric_round, lowercase_result
**错误处理**: error, die, exit, skip
**文件 I/O**: write_file, append_file, remove_file, remove_files_wildcard, copy_file, move_file, mkdir, rmdir, chmod, diff_files, file_exists, cat_file, list_files, copy_files_wildcard
**服务器控制**: shutdown_server, send_quit, send_shutdown
**开关**: disable/enable_warnings, query_log, result_log, info, metadata, ps_protocol, reconnect, connect_log, session_track_info, testcase, parsing, async_client
**其他**: sleep, exec, execw, exec_in_background, delimiter, character_set, system, real_sleep, require, sync_slave_with_master, output

</details>

## 构建和测试

```bash
cargo build                    # 编译
cargo test                     # 运行 56 个单元测试 + fixture 测试
cargo test -- --ignored        # 运行集成测试（需 MYSQL_TEST_DIR 环境变量）
cargo doc --no-deps            # 生成文档
```

## License

Apache-2.0
