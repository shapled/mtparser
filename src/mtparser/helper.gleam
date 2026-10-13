//// Shared construction and manipulation functions for the AST types:
//// spans, lines, interpolated text, expressions, and the text/token
//// utilities the statement parsers build on.

import gleam/int
import gleam/list
import gleam/string
import mtparser/ast.{
  type InterpolatedText, type Line, type Span, Line, Literal, Span, Variable,
}

// ── span ────────────────────────────────────────────────────────────────────

/// Placeholder span for tests or unknown positions.
pub fn dummy_span() -> Span {
  Span(line: 1, column: 1, offset: 0, len: 0)
}

/// Merge two spans into one covering both.
pub fn merge_span(a: Span, b: Span) -> Span {
  let offset = int.min(a.offset, b.offset)
  let end = int.max(a.offset + a.len, b.offset + b.len)
  let line = int.min(a.line, b.line)
  // The merged column belongs to whichever span starts first.
  let column = case a.line < b.line, a.line == b.line {
    True, _ -> a.column
    False, True -> int.min(a.column, b.column)
    False, False -> b.column
  }
  Span(line: line, column: column, offset: offset, len: end - offset)
}

/// "line L col C"
pub fn span_to_string(span: Span) -> String {
  "line " <> int.to_string(span.line) <> " col " <> int.to_string(span.column)
}

// ── lines ───────────────────────────────────────────────────────────────────

/// Split `source` into lines.
///
/// `\n` separates lines; a trailing `\r` (CRLF files) stays in the text and
/// is handled as trailing whitespace by consumers. The empty region after a
/// final `\n` produces no line.
pub fn split_lines(source: String) -> List(Line) {
  case source {
    "" -> []
    _ -> {
      let raw = string.split(source, "\n")
      let raw = case string.ends_with(source, "\n") {
        True -> all_but_last(raw)
        False -> raw
      }
      build_lines(raw, 0, 1, [])
    }
  }
}

fn build_lines(
  raw: List(String),
  offset: Int,
  number: Int,
  acc: List(Line),
) -> List(Line) {
  case raw {
    [] -> list.reverse(acc)
    [text, ..rest] ->
      build_lines(rest, offset + utf16_size(text) + 1, number + 1, [
        Line(text: text, offset: offset, number: number),
        ..acc
      ])
  }
}

// ── interpolated text ───────────────────────────────────────────────────────

/// Raw string form: variable references rendered with their `$` prefix.
pub fn to_raw_string(text: InterpolatedText) -> String {
  text
  |> list.map(fn(part) {
    case part {
      Literal(text, _) -> text
      Variable(name, _) -> "$" <> name
    }
  })
  |> string.concat
}

/// True if the text contains no variable references.
pub fn is_literal(text: InterpolatedText) -> Bool {
  list.all(text, fn(part) {
    case part {
      Literal(_, _) -> True
      Variable(_, _) -> False
    }
  })
}

/// All variable names referenced in this text, in order.
pub fn variable_names(text: InterpolatedText) -> List(String) {
  list.filter_map(text, fn(part) {
    case part {
      Literal(_, _) -> Error(Nil)
      Variable(name, _) -> Ok(name)
    }
  })
}

// ── grapheme-list utilities ─────────────────────────────────────────────────

/// Length of a string in UTF-16 code units — the unit JS string indexing
/// (`String.prototype.length`, `substring`) and editors (VS Code/LSP) use
/// for offsets. Supplementary-plane codepoints count as 2. All span offsets
/// and lengths in this library are in these units.
pub fn utf16_size(s: String) -> Int {
  s
  |> string.to_utf_codepoints
  |> list.fold(0, fn(acc, cp) {
    case string.utf_codepoint_to_int(cp) >= 0x10000 {
      True -> acc + 2
      False -> acc + 1
    }
  })
}

/// Total UTF-16 length of a list of graphemes.
pub fn gs_len(gs: List(String)) -> Int {
  list.fold(gs, 0, fn(acc, g) { acc + utf16_size(g) })
}

pub fn trim_start_list(gs: List(String)) -> #(List(String), Int) {
  case gs {
    [g, ..rest] ->
      case g == " " || g == "\t" {
        True -> {
          let #(trimmed, dropped) = trim_start_list(rest)
          #(trimmed, utf16_size(g) + dropped)
        }
        False -> #(gs, 0)
      }
    [] -> #(gs, 0)
  }
}

pub fn take_while_list(
  gs: List(String),
  pred: fn(String) -> Bool,
) -> #(List(String), List(String)) {
  case gs {
    [g, ..rest] ->
      case pred(g) {
        True -> {
          let #(taken, remaining) = take_while_list(rest, pred)
          #([g, ..taken], remaining)
        }
        False -> #([], gs)
      }
    [] -> #([], gs)
  }
}

/// Graphemes before the first `stop`; Error when `stop` never occurs.
pub fn take_till_list(
  gs: List(String),
  stop: String,
) -> Result(#(List(String), List(String)), Nil) {
  case gs {
    [] -> Error(Nil)
    [g, ..rest] ->
      case g == stop {
        True -> Ok(#([], rest))
        False ->
          case take_till_list(rest, stop) {
            Ok(#(before, after)) -> Ok(#([g, ..before], after))
            Error(Nil) -> Error(Nil)
          }
      }
  }
}

pub fn take_till_any(gs: List(String), stops: List(String)) -> List(String) {
  case gs {
    [] -> []
    [g, ..rest] ->
      case list.contains(stops, g) {
        True -> []
        False -> [g, ..take_till_any(rest, stops)]
      }
  }
}

const alnum_chars =
  "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"

const ident_chars =
  "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-"

pub fn is_alnum(g: String) -> Bool {
  string.contains(alnum_chars, g)
}

pub fn is_ident(g: String) -> Bool {
  string.contains(ident_chars, g)
}

pub fn is_digit(g: String) -> Bool {
  string.contains("0123456789", g)
}

// ── spans over lines and command text ───────────────────────────────────────

/// Span of a line's trimmed content.
pub fn line_span(line: Line) -> Span {
  Span(
    line: line.number,
    column: leading_ws(line) + 1,
    offset: line.offset + leading_ws(line),
    len: utf16_size(string.trim(line.text)),
  )
}

pub fn leading_ws(line: Line) -> Int {
  utf16_size(line.text) - utf16_size(string.trim_start(line.text))
}

/// Span of a range inside `body`, which starts at `base.offset` (with
/// `base.line`/`base.column` pointing at that position). Counts newlines so
/// the line/column are real even when the command spans lines.
pub fn sub_span(base: Span, body: String, start: Int, len: Int) -> Span {
  let #(line_delta, line_start) = locate(body, start)
  case line_delta {
    0 ->
      Span(
        line: base.line,
        column: base.column + start,
        offset: base.offset + start,
        len: len,
      )
    _ ->
      Span(
        line: base.line + line_delta,
        column: start - line_start + 1,
        offset: base.offset + start,
        len: len,
      )
  }
}

/// #(newlines before `pos`, offset of the containing line's start)
fn locate(body: String, pos: Int) -> #(Int, Int) {
  locate_loop(string.split(body, "\n"), 0, 0, pos)
}

fn locate_loop(
  segments: List(String),
  line: Int,
  acc: Int,
  pos: Int,
) -> #(Int, Int) {
  case segments {
    [seg, ..rest] -> {
      let line_end = acc + utf16_size(seg)
      case pos <= line_end {
        True -> #(line, acc)
        False -> locate_loop(rest, line + 1, line_end + 1, pos)
      }
    }
    [] -> #(line, acc)
  }
}

/// The first `n` UTF-16 units of a string.
pub fn prefix_len(s: String, n: Int) -> String {
  prefix_loop(string.to_graphemes(s), n, [])
}

fn prefix_loop(gs: List(String), n: Int, acc: List(String)) -> String {
  case gs {
    [] -> string.concat(list.reverse(acc))
    [g, ..rest] ->
      case n <= 0 {
        True -> string.concat(list.reverse(acc))
        False -> prefix_loop(rest, n - utf16_size(g), [g, ..acc])
      }
  }
}

// ── text & token utilities for statement parsers ────────────────────────────

/// Leading identifier chars `[a-zA-Z0-9_-]` of a string.
pub fn first_word(s: String) -> String {
  s
  |> string.to_graphemes
  |> list.take_while(fn(g) {
    string.contains(
      "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-",
      g,
    )
  })
  |> string.concat
}

pub fn drop_word(s: String) -> String {
  string.remove_prefix(s, first_word(s))
}

/// The first whitespace-delimited token of a string.
pub fn take_token(s: String) -> String {
  s
  |> string.to_graphemes
  |> list.take_while(fn(g) { g != " " && g != "\t" })
  |> string.concat
}

pub fn drop_token(s: String) -> String {
  string.remove_prefix(s, take_token(s))
}

pub fn all_but_last(items: List(a)) -> List(a) {
  list.take(items, list.length(items) - 1)
}

/// Absolute offset of `arg` (a trim_start'ed suffix of `full` whose
/// span is `span`).
pub fn arg_offset(span: Span, full: String, arg: String) -> Int {
  span.offset + utf16_size(full) - utf16_size(arg)
}

/// The n-th element of a list.
pub fn nth(items: List(a), n: Int) -> Result(a, Nil) {
  case n, items {
    _, [] -> Error(Nil)
    0, [x, ..] -> Ok(x)
    _, [_, ..rest] -> nth(rest, n - 1)
  }
}

/// Whitespace-separated tokens of `arg`, each with its UTF-16 offset within
/// `arg` (which must be trim_start'ed — the first token starts at 0).
pub fn ws_tokens(arg: String) -> List(#(String, Int)) {
  ws_tokens_scan(string.to_graphemes(arg), 0, [], [], 0)
}

fn ws_tokens_scan(
  gs: List(String),
  offset: Int,
  acc: List(#(String, Int)),
  cur: List(String),
  cur_start: Int,
) -> List(#(String, Int)) {
  case gs {
    [] -> list.reverse(flush_ws_token(cur, cur_start, acc))
    [g, ..rest] ->
      case g == " " || g == "\t" {
        True ->
          ws_tokens_scan(
            rest,
            offset + utf16_size(g),
            flush_ws_token(cur, cur_start, acc),
            [],
            0,
          )
        False ->
          ws_tokens_scan(
            rest,
            offset + utf16_size(g),
            acc,
            [g, ..cur],
            case cur {
              [] -> offset
              _ -> cur_start
            },
          )
      }
  }
}

fn flush_ws_token(
  cur: List(String),
  cur_start: Int,
  acc: List(#(String, Int)),
) -> List(#(String, Int)) {
  case cur {
    [] -> acc
    _ -> [#(string.concat(list.reverse(cur)), cur_start), ..acc]
  }
}

/// Comma-separated segments of `arg`, each trimmed (with its UTF-16 offset
/// within `arg`); empty segments are kept.
pub fn comma_segments(arg: String) -> List(#(String, Int)) {
  comma_scan(string.to_graphemes(arg), 0, [], [], 0)
}

fn comma_scan(
  gs: List(String),
  offset: Int,
  acc: List(#(String, Int)),
  cur: List(String),
  cur_start: Int,
) -> List(#(String, Int)) {
  case gs {
    [] -> list.reverse(flush_comma(cur, cur_start, acc))
    [g, ..rest] ->
      case g == "," {
        True ->
          comma_scan(rest, offset + 1, flush_comma(cur, cur_start, acc), [], 0)
        False ->
          comma_scan(rest, offset + utf16_size(g), acc, [g, ..cur], case cur {
            [] -> offset
            _ -> cur_start
          })
      }
  }
}

fn flush_comma(
  cur: List(String),
  cur_start: Int,
  acc: List(#(String, Int)),
) -> List(#(String, Int)) {
  case cur {
    [] -> [#("", cur_start), ..acc]
    _ -> {
      // Trim the segment; the offset follows the trimmed start.
      let raw = string.concat(list.reverse(cur))
      let trimmed = string.trim(raw)
      let leading = utf16_size(raw) - utf16_size(string.trim_start(raw))
      [#(trimmed, cur_start + leading), ..acc]
    }
  }
}
