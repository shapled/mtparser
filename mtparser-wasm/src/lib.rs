use mtparser::parser::{parse, ParserConfig};
use mtparser::version::MysqlVersion;
use wasm_bindgen::prelude::*;

/// Parse a mysqltest/mariadb-test file.
///
/// @param input - File content (`.test` or `.inc`)
/// @param version - Optional version: "5.7", "8.0", "8.4", "9.7", "mariadb", "compatible", or omit for all MySQL
/// @returns Array of Statement objects, or throws on parse error
#[wasm_bindgen]
pub fn parse_test(input: &str, version: Option<String>) -> Result<JsValue, JsValue> {
    let v = match version.as_deref() {
        Some("mariadb") => MysqlVersion::MariaDB,
        Some("5.7") => MysqlVersion::V57,
        Some("8.0") => MysqlVersion::V80,
        Some("8.4") => MysqlVersion::V84,
        Some("9.7") => MysqlVersion::V97,
        Some("compatible") => MysqlVersion::Compatible,
        _ => MysqlVersion::MySQL,
    };

    match parse(input, ParserConfig::new(v)) {
        Ok(statements) => serde_wasm_bindgen::to_value(&statements)
            .map_err(|e| JsValue::from_str(&format!("serialization error: {}", e))),
        Err(e) => Err(JsValue::from_str(&format!("{}", e))),
    }
}
