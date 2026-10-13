import gleam/list
import gleam/option.{None, Some}
import gleam/string
import gleeunit/should

import mtparser_tools/harness

// ── parse_args ───────────────────────────────────────────────────────────────

pub fn parse_args_all_flags_test() {
  let args = [
    "--mysql", "/tmp/m", "--mariadb", "/tmp/d", "--work-dir", "/tmp/w", "--keep",
    "--fail-under", "90", "--report", "/tmp/r.txt", "--jobs", "4",
  ]
  let assert Ok(config) = harness.parse_args(args)
  config.mysql_repo |> should.equal("/tmp/m")
  config.mariadb_repo |> should.equal("/tmp/d")
  config.work_dir |> should.equal(Some("/tmp/w"))
  config.keep |> should.equal(True)
  config.fail_under |> should.equal(Some(90))
  config.report |> should.equal(Some("/tmp/r.txt"))
  config.jobs |> should.equal(4)
}

pub fn parse_args_jobs_default_test() {
  let assert Ok(config) =
    harness.parse_args(["--mysql", "/m", "--mariadb", "/d"])
  config.jobs |> should.equal(12)
}

pub fn parse_args_jobs_invalid_test() {
  harness.parse_args(["--mysql", "/m", "--mariadb", "/d", "--jobs", "0"])
  |> should.be_error
  harness.parse_args(["--mysql", "/m", "--mariadb", "/d", "--jobs", "-3"])
  |> should.be_error
  harness.parse_args(["--mysql", "/m", "--mariadb", "/d", "--jobs", "abc"])
  |> should.be_error
}

pub fn parse_args_minimal_test() {
  let assert Ok(config) =
    harness.parse_args(["--mysql", "/m", "--mariadb", "/d"])
  config.mysql_repo |> should.equal("/m")
  config.mariadb_repo |> should.equal("/d")
  config.work_dir |> should.equal(None)
  config.keep |> should.equal(False)
  config.fail_under |> should.equal(None)
  config.report |> should.equal(None)
}

pub fn parse_args_missing_mysql_test() {
  harness.parse_args(["--mariadb", "/d"])
  |> should.be_error
}

pub fn parse_args_missing_mariadb_test() {
  harness.parse_args(["--mysql", "/m"])
  |> should.be_error
}

pub fn parse_args_unknown_flag_test() {
  harness.parse_args(["--mysql", "/m", "--mariadb", "/d", "--bogus"])
  |> should.be_error
}

pub fn parse_args_fail_under_non_numeric_test() {
  harness.parse_args(["--mysql", "/m", "--mariadb", "/d", "--fail-under", "abc"])
  |> should.be_error
}

pub fn parse_args_fail_under_out_of_range_test() {
  harness.parse_args(["--mysql", "/m", "--mariadb", "/d", "--fail-under", "101"])
  |> should.be_error
}

pub fn parse_args_fail_under_negative_test() {
  harness.parse_args(["--mysql", "/m", "--mariadb", "/d", "--fail-under", "-1"])
  |> should.be_error
}

pub fn parse_args_duplicate_last_wins_test() {
  let assert Ok(config) =
    harness.parse_args(["--mysql", "/a", "--mysql", "/b", "--mariadb", "/d"])
  config.mysql_repo |> should.equal("/b")
}

pub fn wants_help_test() {
  harness.wants_help(["--help"]) |> should.equal(True)
  harness.wants_help(["-h"]) |> should.equal(True)
  harness.wants_help(["--mysql", "/m"]) |> should.equal(False)
  harness.wants_help([]) |> should.equal(False)
}

// ── select_ref ───────────────────────────────────────────────────────────────

pub fn select_ref_branch_wins_test() {
  harness.select_ref("8.0", True, ["mysql-8.0.45", "mysql-8.0.46"])
  |> should.equal(Ok("8.0"))
}

pub fn select_ref_tag_fallback_test() {
  // tags arrive ascending by version; the release beats the -rc
  harness.select_ref("26.7", False, ["mysql-26.7.0-rc", "mysql-26.7.0"])
  |> should.equal(Ok("mysql-26.7.0"))
}

pub fn select_ref_missing_test() {
  let assert Error(message) = harness.select_ref("26.7", False, [])
  string.contains(message, "git fetch")
  |> should.equal(True)
}

// ── is_test_file ─────────────────────────────────────────────────────────────

pub fn is_test_file_test() {
  harness.is_test_file("mysql-test/t/foo.test") |> should.equal(True)
  harness.is_test_file("mysql-test/include/bar.inc") |> should.equal(True)
  harness.is_test_file("mysql-test/r/foo.result") |> should.equal(False)
  harness.is_test_file("mysql-test/t/foo.opt") |> should.equal(False)
  harness.is_test_file("mysql-test/t/foo.cnf") |> should.equal(False)
  harness.is_test_file("README") |> should.equal(False)
  harness.is_test_file("foo.TEST") |> should.equal(False)
}

// ── build_report / percent / should_fail ─────────────────────────────────────

pub fn build_report_counts_test() {
  let spec = harness.VersionSpec(repo: harness.Mysql, version: "8.0")
  let results = [
    harness.ParsedOk("a.test"),
    harness.ParsedOk("b.test"),
    harness.ParseFailed("c.test", "boom"),
    harness.ReadFailed("d.inc", "eacces"),
    harness.NonUtf8("e.test"),
  ]
  let report = harness.build_report(spec, "8.0", results)
  report.files |> should.equal(5)
  report.ok |> should.equal(2)
  report.parse_failed |> should.equal(1)
  report.read_failed |> should.equal(1)
  report.non_utf8 |> should.equal(1)
  // Non-UTF-8 files are skips: not in the failure list, excluded from the
  // pass-rate denominator.
  report.failures |> list.length |> should.equal(2)
  harness.parseable(report) |> should.equal(4)
}

pub fn render_version_report_nonutf8_test() {
  let report =
    harness.build_report(
      harness.VersionSpec(repo: harness.Mariadb, version: "12.3"),
      "12.3",
      [harness.ParsedOk("a.test"), harness.NonUtf8("latin1.test")],
    )
  let text = harness.render_version_report(report)
  text |> string.contains("non-utf8 1") |> should.equal(True)
  // 1/1 parseable, not 1/2 files.
  text |> string.contains("pass 100.0%") |> should.equal(True)
}

pub fn percent_zero_denominator_test() {
  harness.percent(0, 0) |> should.equal(0.0)
  harness.percent_string(0, 0) |> should.equal("0.0")
}

pub fn percent_string_test() {
  harness.percent_string(2710, 2917) |> should.equal("92.9")
  harness.percent_string(100, 100) |> should.equal("100.0")
  harness.percent_string(0, 10) |> should.equal("0.0")
  harness.percent_string(1, 3) |> should.equal("33.3")
}

pub fn should_fail_test() {
  let ran = fn(ok: Int, files: Int) {
    harness.Ran(harness.build_report(
      harness.VersionSpec(repo: harness.Mysql, version: "8.0"),
      "8.0",
      list.append(
        list.repeat(harness.ParsedOk("x.test"), ok),
        list.repeat(harness.ParseFailed("y.test", "boom"), files - ok),
      ),
    ))
  }
  // no threshold -> never fail
  harness.should_fail([ran(50, 100)], None) |> should.equal(False)
  // above threshold
  harness.should_fail([ran(95, 100)], Some(90)) |> should.equal(False)
  // below threshold
  harness.should_fail([ran(80, 100)], Some(90)) |> should.equal(True)
  // exact boundary is not a failure (80/100 = 80.0, threshold 80)
  harness.should_fail([ran(80, 100)], Some(80)) |> should.equal(False)
  // errored versions always fail under a threshold
  harness.should_fail(
    [
      harness.Errored(
        harness.VersionSpec(repo: harness.Mysql, version: "8.0"),
        "x",
      ),
    ],
    Some(90),
  )
  |> should.equal(True)
}

// ── rendering ────────────────────────────────────────────────────────────────

pub fn render_version_report_test() {
  let report =
    harness.build_report(
      harness.VersionSpec(repo: harness.Mysql, version: "8.0"),
      "8.0",
      [
        harness.ParsedOk("a.test"),
        harness.ParseFailed("b.test", "unknown command '--bogus' at line 1"),
      ],
    )
  let text = harness.render_version_report(report)
  text
  |> string.contains("== mysql 8.0 (ref 8.0) ==")
  |> should.equal(True)
  text
  |> string.contains("files 2 | ok 1 | parse failed 1 | read failed 0")
  |> should.equal(True)
  text
  |> string.contains("pass 50.0%")
  |> should.equal(True)
  text
  |> string.contains("[parse] b.test: unknown command '--bogus' at line 1")
  |> should.equal(True)
}

pub fn render_overall_test() {
  let outcomes = [
    harness.Ran(
      harness.build_report(
        harness.VersionSpec(repo: harness.Mysql, version: "8.0"),
        "8.0",
        [harness.ParsedOk("a.test")],
      ),
    ),
    harness.Errored(
      harness.VersionSpec(repo: harness.Mariadb, version: "12.3"),
      "boom",
    ),
  ]
  let text = harness.render_overall(outcomes)
  text |> string.contains("mysql-8.0: 100.0%  (1/1)") |> should.equal(True)
  text |> string.contains("ERROR mariadb-12.3: boom") |> should.equal(True)
  text
  |> string.contains("total: 2 versions | files 1 | ok 1 | failed 0")
  |> should.equal(True)
}
