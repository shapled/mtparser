import gleam/list
import gleeunit
import gleeunit/should
import mtparser

pub fn main() -> Nil {
  gleeunit.main()
}

pub fn parse_smoke_test() {
  let assert Ok(statements) = mtparser.parse("--echo hi\nSELECT 1;\n")
  statements |> list.length |> should.equal(2)
}

pub fn parse_empty_source_test() {
  mtparser.parse("") |> should.equal(Ok([]))
}
