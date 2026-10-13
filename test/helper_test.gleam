import gleeunit/should
import mtparser/ast.{Span}
import mtparser/helper

pub fn merge_same_line_test() {
  helper.merge_span(Span(1, 5, 4, 2), Span(1, 9, 8, 3))
  |> should.equal(Span(1, 5, 4, 7))
}

pub fn merge_across_lines_test() {
  helper.merge_span(Span(2, 3, 10, 4), Span(1, 7, 0, 2))
  |> should.equal(Span(1, 7, 0, 14))
}

pub fn to_string_test() {
  helper.span_to_string(Span(3, 7, 20, 5))
  |> should.equal("line 3 col 7")
}
