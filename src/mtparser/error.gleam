//// Error types for the parser.

import gleam/option.{type Option}
import mtparser/ast.{type Span}
import mtparser/helper.{span_to_string}

/// Parse errors.
pub type ParserError {
  Syntax(message: String, span: Span)
  UnexpectedEof(context: String)
  UnknownCommand(command: String, span: Span, suggestion: Option(String))
  UnterminatedBlock(command: String, marker: String, span: Span)
  UnterminatedFlowControl(kind: String, span: Span)
  UnexpectedToken(token: String, expected: String, span: Span)
  InvalidConnectParams(message: String, span: Span)
  InvalidExpression(message: String, span: Span)
}

/// Render an error message with location.
pub fn to_string(error: ParserError) -> String {
  case error {
    Syntax(message, span) -> "syntax error at " <> at(span) <> ": " <> message
    UnexpectedEof(context) -> "unexpected end of file while parsing " <> context
    UnknownCommand(command, span, _) ->
      "unknown command '" <> command <> "' at " <> at(span)
    UnterminatedBlock(command, marker, span) ->
      "unterminated block command '"
      <> command
      <> "', expected end marker '"
      <> marker
      <> "' at "
      <> at(span)
    UnterminatedFlowControl(kind, span) ->
      "unterminated flow control '" <> kind <> "' at " <> at(span)
    UnexpectedToken(token, expected, span) ->
      "unexpected '"
      <> token
      <> "' at "
      <> at(span)
      <> ", expected "
      <> expected
    InvalidConnectParams(message, span) ->
      "invalid connect parameters at " <> at(span) <> ": " <> message
    InvalidExpression(message, span) ->
      "invalid expression at " <> at(span) <> ": " <> message
  }
}

fn at(span: Span) -> String {
  span_to_string(span)
}

/// A non-fatal diagnostic with severity level.
pub type Diagnostic {
  Diagnostic(message: String, span: Span)
}
