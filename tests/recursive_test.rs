use mtparser::error::ParseMode;
use mtparser::parser::{parse, ParserConfig};
use mtparser::version::MysqlVersion;
use std::fs;
use std::path::Path;

/// Recursively find all .test and .inc files in a directory.
fn find_test_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if !dir.exists() {
        return files;
    }
    for entry in fs::read_dir(dir).expect("failed to read dir") {
        let entry = entry.expect("failed to read entry");
        let path = entry.path();
        if path.is_dir() {
            files.extend(find_test_files(&path));
        } else {
            match path.extension().and_then(|e| e.to_str()) {
                Some("test") | Some("inc") => files.push(path),
                _ => {}
            }
        }
    }
    files
}

/// Parse all .test/.inc files in a directory in strict mode.
/// Returns (total_files, success_count, failure_files_with_errors).
fn test_directory_strict(dir: &Path, version: MysqlVersion) -> (usize, usize, Vec<(std::path::PathBuf, String)>) {
    let files = find_test_files(dir);
    let total = files.len();
    let mut success = 0usize;
    let mut failures = Vec::new();

    for filepath in &files {
        let config = ParserConfig::new(version, ParseMode::Strict);
        let content = match fs::read_to_string(filepath) {
            Ok(c) => c,
            Err(e) => {
                failures.push((filepath.clone(), format!("read error: {}", e)));
                continue;
            }
        };

        match parse(&content, config) {
            Ok(_ast) => success += 1,
            Err(e) => {
                failures.push((filepath.clone(), format!("{}", e)));
            }
        }
    }

    (total, success, failures)
}

// --- Tests against fixture files ---

#[test]
fn test_fixtures_directory_strict() {
    let dir = Path::new("tests/fixtures");
    let (total, success, failures) = test_directory_strict(dir, MysqlVersion::V80);
    assert_eq!(total, success, "all fixtures should parse successfully:\n{:#?}", failures);
}

// --- Tests against real MySQL test directory ---

/// Test all files in mysql-test/t/ directory (5.7 branch).
/// This test is only run when the MYSQL_TEST_DIR env var is set.
#[test]
#[ignore] // Run with: cargo test test_mysql_test_dir -- --ignored
fn test_mysql_test_dir() {
    let dir = std::env::var("MYSQL_TEST_DIR")
        .expect("MYSQL_TEST_DIR env var not set. Usage: MYSQL_TEST_DIR=/path/to/mysql-server/mysql-test cargo test test_mysql_test_dir -- --ignored");
    let dir = Path::new(&dir);
    let (total, success, failures) = test_directory_strict(dir, MysqlVersion::V57);

    println!("\n=== MySQL Test Directory Results ===");
    println!("Total files: {}", total);
    println!("Success: {}", success);
    println!("Failed: {}", failures.len());

    // Print first 20 failures
    for (path, error) in failures.iter().take(20) {
        println!("  FAIL: {} - {}", path.display(), error);
    }
    if failures.len() > 20 {
        println!("  ... and {} more failures", failures.len() - 20);
    }

    // We don't assert all pass - this is a discovery test
    // Use it to identify patterns of failures to fix
    let pass_rate = success as f64 / total as f64 * 100.0;
    println!("Pass rate: {:.1}%", pass_rate);
    assert!(pass_rate > 90.0, "pass rate too low: {:.1}%", pass_rate);
}

/// Test files in mysql-test/include/ directory.
#[test]
#[ignore]
fn test_mysql_include_dir() {
    let dir = std::env::var("MYSQL_INCLUDE_DIR")
        .expect("MYSQL_INCLUDE_DIR env var not set. Usage: MYSQL_INCLUDE_DIR=/path/to/mysql-server/mysql-test/include cargo test test_mysql_include_dir -- --ignored");
    let dir = Path::new(&dir);
    let (total, success, failures) = test_directory_strict(dir, MysqlVersion::V80);

    println!("\n=== MySQL Include Directory Results ===");
    println!("Total files: {}", total);
    println!("Success: {}", success);
    println!("Failed: {}", failures.len());

    for (path, error) in failures.iter().take(20) {
        println!("  FAIL: {} - {}", path.display(), error);
    }
    if failures.len() > 20 {
        println!("  ... and {} more failures", failures.len() - 20);
    }

    let pass_rate = success as f64 / total as f64 * 100.0;
    println!("Pass rate: {:.1}%", pass_rate);
    assert!(pass_rate > 90.0, "pass rate too low: {:.1}%", pass_rate);
}
