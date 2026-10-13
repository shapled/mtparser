//// Integration-test harness entry point: extracts the `mysql-test` tree of
//// each supported MySQL/MariaDB version from the local git repos (no
//// network, no repo mutation — `git archive` + `tar`), parses every
//// `.test`/`.inc` file, and reports pass rates.
////
//// Run: gleam run -m mtparser/integration -- --mysql <repo> --mariadb <repo>
////
//// Pure logic (args, ref selection, counting, rendering) lives in
//// `mtparser_tools/harness` and is unit-tested on both targets. Parallelism uses
//// a tiny Erlang FFI (`mtparser_integration_ffi`); those functions simply
//// don't exist in the JavaScript build.

import argv
import gleam/int
import gleam/io
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import mtparser
import mtparser/error
import mtparser_tools/harness
import shellout
import simplifile

// ── erlang process FFI ───────────────────────────────────────────────────────

@target(erlang)
/// Opaque Erlang pid. Only meaningful on the Erlang target.
pub type Pid

@target(erlang)
@external(erlang, "erlang", "self")
fn self_erlang() -> Pid

@target(erlang)
@external(erlang, "mtparser_tools_ffi", "spawn_fun")
fn spawn_fun(fun: fn() -> Nil) -> Pid

@target(erlang)
@external(erlang, "mtparser_tools_ffi", "send_msg")
fn send_msg(pid: Pid, msg: a) -> Nil

@target(erlang)
@external(erlang, "mtparser_tools_ffi", "recv_msg")
fn recv_msg(timeout_ms: Int) -> Result(a, Nil)

@target(erlang)
type Message(a) {
  Message(index: Int, value: a)
}

@target(erlang)
/// Run `f` over `items` with up to `jobs` concurrent workers, preserving
/// order. Falls back to a serial map when `jobs` is 1.
fn map_parallel(items: List(a), f: fn(a) -> b, jobs: Int) -> List(b) {
  case jobs <= 1 {
    True -> list.map(items, f)
    False ->
      items
      |> chunk(jobs)
      |> list.flat_map(fn(group) { parallel_chunk(group, f) })
  }
}

@target(erlang)
fn parallel_chunk(items: List(a), f: fn(a) -> b) -> List(b) {
  let coordinator = self_erlang()
  let _ =
    list.index_map(items, fn(item, index) {
      spawn_fun(fn() {
        send_msg(coordinator, Message(index: index, value: f(item)))
      })
      Nil
    })
  collect_messages(list.length(items), [])
}

@target(erlang)
fn collect_messages(expected: Int, acc: List(Message(b))) -> List(b) {
  case expected {
    0 ->
      acc
      |> list.sort(fn(a, b) { int.compare(a.index, b.index) })
      |> list.map(fn(m) { m.value })
    _ ->
      case recv_msg(120_000) {
        Ok(msg) -> collect_messages(expected - 1, [msg, ..acc])
        Error(Nil) -> panic as "integration worker timed out"
      }
  }
}

@target(erlang)
fn chunk(items: List(a), size: Int) -> List(List(a)) {
  case items {
    [] -> []
    _ -> {
      let #(head, tail) = list.split(items, size)
      [head, ..chunk(tail, size)]
    }
  }
}

// ── main ─────────────────────────────────────────────────────────────────────

@target(erlang)
pub fn main() {
  let args = argv.load().arguments
  case harness.wants_help(args) {
    True -> {
      io.println(harness.usage())
      shellout.exit(0)
    }
    False ->
      case harness.parse_args(args) {
        Ok(config) -> run(config)
        Error(message) -> {
          io.println("error: " <> message)
          io.println("")
          io.println(harness.usage())
          shellout.exit(1)
        }
      }
  }
}

@target(erlang)
fn run(config: harness.Config) {
  io.println("mtparser integration harness")
  io.println(
    "mysql repo: "
    <> config.mysql_repo
    <> "   mariadb repo: "
    <> config.mariadb_repo,
  )

  let #(work_dir, is_temp) = case setup_work_dir(config) {
    Ok(x) -> x
    Error(message) -> die(message)
  }
  io.println(
    "work dir: " <> work_dir <> "   jobs: " <> int.to_string(config.jobs),
  )
  io.println("")

  // Phase 1: resolve refs and extract every version's tree in parallel.
  let extractions =
    map_parallel(
      harness.versions,
      fn(spec) { extract_version(config, spec, work_dir) },
      config.jobs,
    )

  // Phase 2: walk and parse every version's tree in parallel.
  let outcomes =
    map_parallel(
      extractions,
      fn(extraction) {
        case extraction {
          Ok(#(spec, ref, tree)) ->
            case walk_and_parse(spec, ref, tree, work_dir, config.jobs) {
              Ok(report) -> harness.Ran(report)
              Error(message) -> harness.Errored(spec, message)
            }
          Error(#(spec, message)) -> harness.Errored(spec, message)
        }
      },
      config.jobs,
    )

  // Print in version order.
  list.each(outcomes, fn(outcome) {
    io.println(case outcome {
      harness.Ran(report) -> harness.render_version_report(report)
      harness.Errored(spec, message) ->
        "ERROR "
        <> harness.repo_name(spec.repo)
        <> "-"
        <> spec.version
        <> ": "
        <> message
    })
    io.println("")
  })

  io.println(harness.render_overall(outcomes))

  let report_path = case config.report {
    Some(path) -> path
    None -> "mtparser-integration-report.txt"
  }
  let contents =
    harness.render_overall(outcomes)
    <> "\n\n"
    <> harness.render_failure_file(outcomes)
  case simplifile.write(to: report_path, contents: contents) {
    Ok(_) -> io.println("report: " <> report_path)
    Error(e) ->
      io.println(
        "warning: cannot write report: " <> simplifile.describe_error(e),
      )
  }

  case is_temp && !config.keep {
    True -> {
      let _ = simplifile.delete(work_dir)
      Nil
    }
    False -> Nil
  }

  case harness.should_fail(outcomes, config.fail_under) {
    True -> {
      io.println(
        "FAIL: versions below threshold "
        <> fail_under_string(config.fail_under),
      )
      shellout.exit(1)
    }
    False -> shellout.exit(0)
  }
}

@target(erlang)
/// #(work dir, is_temp); the temp dir is removed after the run unless
/// `--keep`. A relative `--work-dir` is absolutized against the cwd so the
/// tar output path (passed to git, which runs in the repo dir) stays valid.
fn setup_work_dir(config: harness.Config) -> Result(#(String, Bool), String) {
  let work = case config.work_dir {
    Some(dir) -> Ok(#(absolutize(dir), False))
    None ->
      case
        shellout.command(
          run: "mktemp",
          with: ["-d", "-t", "mtparser-harness"],
          in: ".",
          opt: [],
        )
      {
        Ok(out) -> Ok(#(string.trim(out), True))
        Error(#(status, message)) ->
          Error("mktemp failed (" <> int.to_string(status) <> "): " <> message)
      }
  }
  case work {
    Ok(#(dir, is_temp)) ->
      case simplifile.create_directory_all(dir) {
        Ok(_) -> Ok(#(dir, is_temp))
        Error(e) ->
          Error("cannot create work dir: " <> simplifile.describe_error(e))
      }
    Error(_) -> work
  }
}

@target(erlang)
fn absolutize(dir: String) -> String {
  case string.starts_with(dir, "/") {
    True -> dir
    False ->
      case shellout.command(run: "pwd", with: [], in: ".", opt: []) {
        Ok(cwd) -> string.trim(cwd) <> "/" <> dir
        Error(_) -> dir
      }
  }
}

@target(erlang)
fn extract_version(
  config: harness.Config,
  spec: harness.VersionSpec,
  work_dir: String,
) -> Result(
  #(harness.VersionSpec, String, String),
  #(harness.VersionSpec, String),
) {
  let repo_path = case spec.repo {
    harness.Mysql -> config.mysql_repo
    harness.Mariadb -> config.mariadb_repo
  }
  case resolve_ref(repo_path, spec) {
    Error(message) -> Error(#(spec, message))
    Ok(ref) ->
      case extract(repo_path, ref, work_dir, spec) {
        Ok(tree) -> Ok(#(spec, ref, tree))
        Error(message) -> Error(#(spec, message))
      }
  }
}

@target(erlang)
/// Branch `<version>` wins; else the newest local tag `<prefix><version>.*`
/// (covers 26.7 → tag `mysql-26.7.0`, which has no branch).
fn resolve_ref(
  repo_path: String,
  spec: harness.VersionSpec,
) -> Result(String, String) {
  case
    shellout.command(
      run: "git",
      with: ["show-ref", "--verify", "refs/heads/" <> spec.version],
      in: repo_path,
      opt: [],
    )
  {
    Ok(_) -> Ok(spec.version)
    Error(_) ->
      case
        shellout.command(
          run: "git",
          with: ["tag", "-l", "--sort=v:refname", harness.tag_glob(spec)],
          in: repo_path,
          opt: [],
        )
      {
        Ok(out) -> {
          let tags =
            out
            |> string.trim
            |> string.split("\n")
            |> list.filter(fn(t) { t != "" })
          harness.select_ref(spec.version, False, tags)
        }
        Error(#(status, message)) ->
          Error(
            "git tag -l failed (" <> int.to_string(status) <> "): " <> message,
          )
      }
  }
}

@target(erlang)
/// Archive just the `mysql-test` tree into a tar, then extract it.
fn extract(
  repo_path: String,
  ref: String,
  work_dir: String,
  spec: harness.VersionSpec,
) -> Result(String, String) {
  let name = harness.repo_name(spec.repo) <> "-" <> spec.version
  let version_dir = work_dir <> "/" <> name
  let tar_path = work_dir <> "/" <> name <> ".tar"
  let _ = simplifile.delete(version_dir)
  let _ = simplifile.delete(tar_path)
  use _ <- result.try(case simplifile.create_directory_all(version_dir) {
    Ok(_) -> Ok(Nil)
    Error(e) ->
      Error(
        "cannot create " <> version_dir <> ": " <> simplifile.describe_error(e),
      )
  })
  use _ <- result.try(
    case
      shellout.command(
        run: "git",
        with: [
          "archive",
          "--format=tar",
          "--output=" <> tar_path,
          ref,
          "mysql-test",
        ],
        in: repo_path,
        opt: [],
      )
    {
      Ok(_) -> Ok(Nil)
      Error(#(status, message)) ->
        Error(
          "git archive failed (" <> int.to_string(status) <> "): " <> message,
        )
    },
  )
  use _ <- result.try(
    case
      shellout.command(
        run: "tar",
        with: ["-xf", tar_path, "-C", version_dir],
        in: work_dir,
        opt: [],
      )
    {
      Ok(_) -> Ok(Nil)
      Error(#(_, message)) -> Error("tar -xf failed: " <> message)
    },
  )
  let _ = simplifile.delete(tar_path)
  Ok(version_dir <> "/mysql-test")
}

@target(erlang)
fn walk_and_parse(
  spec: harness.VersionSpec,
  ref: String,
  tree: String,
  work_dir: String,
  jobs: Int,
) -> Result(harness.VersionReport, String) {
  case simplifile.get_files(in: tree) {
    Error(e) ->
      Error("cannot list " <> tree <> ": " <> simplifile.describe_error(e))
    Ok(paths) -> {
      let tests =
        paths
        |> list.filter(harness.is_test_file)
        |> list.sort(string.compare)
      let results = map_parallel(tests, fn(rel) { parse_file(tree, rel) }, jobs)
      let report = harness.build_report(spec, ref, results)
      let name = harness.repo_name(spec.repo) <> "-" <> spec.version
      let failures_path = work_dir <> "/" <> name <> "/failures.txt"
      let _ =
        simplifile.write(
          to: failures_path,
          contents: harness.render_failure_file([harness.Ran(report)]),
        )
      Ok(report)
    }
  }
}

@target(erlang)
fn parse_file(tree: String, rel: String) -> harness.FileResult {
  let path = case string.starts_with(rel, "/") {
    True -> rel
    False -> tree <> "/" <> rel
  }
  case simplifile.read(from: path) {
    Error(simplifile.NotUtf8) -> harness.NonUtf8(rel)
    Error(e) -> harness.ReadFailed(rel, simplifile.describe_error(e))
    Ok(source) ->
      case mtparser.parse(source) {
        Ok(_) -> harness.ParsedOk(rel)
        Error(err) -> harness.ParseFailed(rel, error.to_string(err))
      }
  }
}

@target(erlang)
/// Print the message and halt with exit code 1.
fn die(message: String) -> a {
  io.println("error: " <> message)
  shellout.exit(1)
  panic as message
}

@target(erlang)
fn fail_under_string(fail_under: Option(Int)) -> String {
  case fail_under {
    Some(n) -> int.to_string(n) <> "%"
    None -> ""
  }
}
