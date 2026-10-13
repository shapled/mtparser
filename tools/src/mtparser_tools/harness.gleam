//// Pure logic for the integration-test harness: argument parsing, git ref
//// selection, result counting, and report rendering. No IO, no parser
//// imports — only `gleam_stdlib`, so the unit tests in `test/harness_test.gleam`
//// run on both targets. The IO pipeline lives in `mtparser/integration`
//// (`@target(erlang)`).

import gleam/float
import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string

// ── configuration & version specs ────────────────────────────────────────────

pub type Repo {
  Mysql
  Mariadb
}

pub type VersionSpec {
  VersionSpec(repo: Repo, version: String)
}

pub type Config {
  Config(
    mysql_repo: String,
    mariadb_repo: String,
    work_dir: Option(String),
    keep: Bool,
    fail_under: Option(Int),
    report: Option(String),
    jobs: Int,
  )
}

pub const versions: List(VersionSpec) = [
  VersionSpec(repo: Mysql, version: "5.7"),
  VersionSpec(repo: Mysql, version: "8.0"),
  VersionSpec(repo: Mysql, version: "8.4"),
  VersionSpec(repo: Mysql, version: "9.7"),
  VersionSpec(repo: Mysql, version: "26.7"),
  VersionSpec(repo: Mariadb, version: "10.11"),
  VersionSpec(repo: Mariadb, version: "11.4"),
  VersionSpec(repo: Mariadb, version: "11.8"),
  VersionSpec(repo: Mariadb, version: "12.3"),
]

pub fn repo_name(repo: Repo) -> String {
  case repo {
    Mysql -> "mysql"
    Mariadb -> "mariadb"
  }
}

pub fn repo_prefix(repo: Repo) -> String {
  repo_name(repo) <> "-"
}

pub fn tag_glob(spec: VersionSpec) -> String {
  repo_prefix(spec.repo) <> spec.version <> ".*"
}

// ── argument parsing ─────────────────────────────────────────────────────────

pub fn usage() -> String {
  "mtparser integration harness — parse MySQL/MariaDB test suites across versions

Usage:
  gleam run -m mtparser/integration -- --mysql <mysql-server-repo> --mariadb <mariadb-repo> [options]

Options:
  --mysql <path>        Local mysql-server git repo (required)
  --mariadb <path>      Local mariadb git repo (required)
  --work-dir <path>     Work dir for extracted trees (default: fresh temp dir, removed after run)
  --keep                Keep the work dir after the run
  --fail-under <0-100>  Exit 1 if any version's pass rate is below this percent
  --report <path>       Report file (default: ./mtparser-integration-report.txt)
  --jobs <N>            Parallel workers for extraction and parsing (default: 12)
  -h, --help            Show this help

Versions: mysql 5.7, 8.0, 8.4, 9.7, 26.7; mariadb 10.11, 11.4, 11.8, 12.3
Ref per version: local branch <version>, else newest local tag <prefix><version>.*
"
}

pub fn wants_help(args: List(String)) -> Bool {
  list.contains(args, "--help") || list.contains(args, "-h")
}

pub fn parse_args(args: List(String)) -> Result(Config, String) {
  parse_args_loop(
    args,
    Config(
      mysql_repo: "",
      mariadb_repo: "",
      work_dir: None,
      keep: False,
      fail_under: None,
      report: None,
      jobs: 12,
    ),
  )
}

fn parse_args_loop(
  args: List(String),
  config: Config,
) -> Result(Config, String) {
  case args {
    [] ->
      case config.mysql_repo, config.mariadb_repo {
        "", _ -> Error("missing required argument: --mysql <path>")
        _, "" -> Error("missing required argument: --mariadb <path>")
        _, _ -> Ok(config)
      }
    ["--keep", ..rest] -> parse_args_loop(rest, Config(..config, keep: True))
    ["--fail-under", n, ..rest] ->
      case int.parse(n) {
        Ok(p) if p >= 0 && p <= 100 ->
          parse_args_loop(rest, Config(..config, fail_under: Some(p)))
        _ -> Error("--fail-under must be an integer 0..100, got: " <> n)
      }
    ["--mysql", path, ..rest] ->
      parse_args_loop(rest, Config(..config, mysql_repo: path))
    ["--mariadb", path, ..rest] ->
      parse_args_loop(rest, Config(..config, mariadb_repo: path))
    ["--work-dir", path, ..rest] ->
      parse_args_loop(rest, Config(..config, work_dir: Some(path)))
    ["--report", path, ..rest] ->
      parse_args_loop(rest, Config(..config, report: Some(path)))
    ["--jobs", n, ..rest] ->
      case int.parse(n) {
        Ok(j) if j >= 1 -> parse_args_loop(rest, Config(..config, jobs: j))
        _ -> Error("--jobs must be a positive integer, got: " <> n)
      }
    [other, ..] -> Error("unknown argument: " <> other)
  }
}

// ── git ref selection ────────────────────────────────────────────────────────

/// Branch `<version>` wins; otherwise the newest of `tags` (pre-sorted
/// ascending by version, so the last is newest); otherwise a fetch hint.
pub fn select_ref(
  version: String,
  has_branch: Bool,
  tags: List(String),
) -> Result(String, String) {
  case has_branch, list.last(tags) {
    True, _ -> Ok(version)
    False, Ok(tag) -> Ok(tag)
    False, Error(_) ->
      Error(
        "no local branch '"
        <> version
        <> "' and no local tag matching '"
        <> version
        <> ".*' — try: git fetch --tags",
      )
  }
}

// ── file filtering & result counting ─────────────────────────────────────────

pub fn is_test_file(path: String) -> Bool {
  string.ends_with(path, ".test") || string.ends_with(path, ".inc")
}

pub type FileResult {
  ParsedOk(path: String)
  ParseFailed(path: String, message: String)
  ReadFailed(path: String, message: String)
  /// Legacy latin1-era files — unrepresentable as UTF-8; skipped, not
  /// counted against the pass rate (a parser-side concern never arises:
  /// consumers hand us decoded text).
  NonUtf8(path: String)
}

pub type VersionReport {
  VersionReport(
    spec: VersionSpec,
    resolved_ref: String,
    files: Int,
    ok: Int,
    parse_failed: Int,
    read_failed: Int,
    non_utf8: Int,
    failures: List(FileResult),
  )
}

pub type VersionOutcome {
  Ran(report: VersionReport)
  Errored(spec: VersionSpec, message: String)
}

pub fn build_report(
  spec: VersionSpec,
  ref: String,
  results: List(FileResult),
) -> VersionReport {
  let files = list.length(results)
  let ok =
    results
    |> list.filter(fn(r) {
      case r {
        ParsedOk(_) -> True
        _ -> False
      }
    })
    |> list.length
  let parse_failed =
    results
    |> list.filter(fn(r) {
      case r {
        ParseFailed(_, _) -> True
        _ -> False
      }
    })
    |> list.length
  let read_failed =
    results
    |> list.filter(fn(r) {
      case r {
        ReadFailed(_, _) -> True
        _ -> False
      }
    })
    |> list.length
  let non_utf8 =
    results
    |> list.filter(fn(r) {
      case r {
        NonUtf8(_) -> True
        _ -> False
      }
    })
    |> list.length
  let failures =
    list.filter(results, fn(r) {
      case r {
        ParsedOk(_) | NonUtf8(_) -> False
        _ -> True
      }
    })
  VersionReport(
    spec: spec,
    resolved_ref: ref,
    files: files,
    ok: ok,
    parse_failed: parse_failed,
    read_failed: read_failed,
    non_utf8: non_utf8,
    failures: failures,
  )
}

/// 0.0..100.0; 0.0 when `whole` is 0.
pub fn percent(part: Int, whole: Int) -> Float {
  case whole {
    0 -> 0.0
    _ -> 100.0 *. int.to_float(part) /. int.to_float(whole)
  }
}

/// One decimal place, e.g. "92.9".
pub fn percent_string(part: Int, whole: Int) -> String {
  case whole {
    0 -> "0.0"
    _ -> {
      let scaled = float.round(percent(part, whole) *. 10.0)
      let whole_part = scaled / 10
      let frac = scaled - whole_part * 10
      int.to_string(whole_part) <> "." <> int.to_string(frac)
    }
  }
}

/// Files the parser can meaningfully be judged against.
pub fn parseable(report: VersionReport) -> Int {
  report.files - report.non_utf8
}

/// True when a threshold is set and any version errored or fell below it.
pub fn should_fail(
  outcomes: List(VersionOutcome),
  fail_under: Option(Int),
) -> Bool {
  case fail_under {
    None -> False
    Some(threshold) ->
      list.any(outcomes, fn(outcome) {
        case outcome {
          Errored(_, _) -> True
          // ok/parseable < threshold/100 ⇔ exact integer comparison
          Ran(report) -> report.ok * 100 < threshold * parseable(report)
        }
      })
  }
}

// ── rendering ────────────────────────────────────────────────────────────────

pub fn render_version_report(report: VersionReport) -> String {
  let label = repo_name(report.spec.repo) <> " " <> report.spec.version
  let header = "== " <> label <> " (ref " <> report.resolved_ref <> ") =="
  let stats =
    "files "
    <> int.to_string(report.files)
    <> " | ok "
    <> int.to_string(report.ok)
    <> " | parse failed "
    <> int.to_string(report.parse_failed)
    <> " | read failed "
    <> int.to_string(report.read_failed)
    <> " | non-utf8 "
    <> int.to_string(report.non_utf8)
    <> " | pass "
    <> percent_string(report.ok, parseable(report))
    <> "%"
  case report.failures {
    [] -> header <> "\n" <> stats
    _ -> {
      let shown = list.take(report.failures, 20)
      let lines = list.map(shown, render_file_result)
      let more = case list.length(report.failures) > 20 {
        True -> [
          "  ... and "
          <> int.to_string(list.length(report.failures) - 20)
          <> " more",
        ]
        False -> []
      }
      header
      <> "\n"
      <> stats
      <> "\nfirst 20 failures:\n"
      <> string.join(list.append(lines, more), "\n")
    }
  }
}

pub fn render_overall(outcomes: List(VersionOutcome)) -> String {
  let lines =
    list.map(outcomes, fn(outcome) {
      case outcome {
        Ran(report) -> {
          repo_name(report.spec.repo)
          <> "-"
          <> report.spec.version
          <> ": "
          <> percent_string(report.ok, parseable(report))
          <> "%  ("
          <> int.to_string(report.ok)
          <> "/"
          <> int.to_string(parseable(report))
          <> ")"
        }
        Errored(spec, message) ->
          "ERROR "
          <> repo_name(spec.repo)
          <> "-"
          <> spec.version
          <> ": "
          <> message
      }
    })
  "== overall ==\n" <> string.join(lines, "\n") <> "\n" <> summarize(outcomes)
}

fn summarize(outcomes: List(VersionOutcome)) -> String {
  let #(files, ok, failed) =
    list.fold(outcomes, #(0, 0, 0), fn(acc, outcome) {
      let #(f, o, x) = acc
      case outcome {
        Ran(report) -> #(
          f + parseable(report),
          o + report.ok,
          x + parseable(report) - report.ok,
        )
        Errored(_, _) -> #(f, o, x)
      }
    })
  "total: "
  <> int.to_string(list.length(outcomes))
  <> " versions | files "
  <> int.to_string(files)
  <> " | ok "
  <> int.to_string(ok)
  <> " | failed "
  <> int.to_string(failed)
  <> " | "
  <> percent_string(ok, files)
  <> "%"
}

/// All failures across all versions (for the report file).
pub fn render_failure_file(outcomes: List(VersionOutcome)) -> String {
  let sections =
    list.flat_map(outcomes, fn(outcome) {
      case outcome {
        Ran(report) -> [
          "=== "
            <> repo_name(report.spec.repo)
            <> "-"
            <> report.spec.version
            <> " (ref "
            <> report.resolved_ref
            <> ") ===",
          ..list.map(report.failures, render_file_result)
        ]
        Errored(spec, message) -> [
          "=== " <> repo_name(spec.repo) <> "-" <> spec.version <> " ===",
          "ERROR: " <> message,
        ]
      }
    })
  case sections {
    [] -> "no failures"
    _ -> string.join(sections, "\n") <> "\n"
  }
}

fn render_file_result(result: FileResult) -> String {
  case result {
    ParsedOk(_) -> ""
    ParseFailed(path, message) -> "  [parse] " <> path <> ": " <> message
    ReadFailed(path, message) -> "  [read ] " <> path <> ": " <> message
    NonUtf8(path) -> "  [utf8 ] " <> path <> ": not UTF-8 encoded (skipped)"
  }
}
