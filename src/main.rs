use std::collections::HashMap;
use std::env;
use std::fs;

use mtparser::parser::ParserConfig;
use mtparser::version::MysqlVersion;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() >= 3 && args[1] == "--version-errors" {
        let version = parse_version_flag(&args, 3);
        show_version_errors(&args[2], version);
        return;
    }

    if args.len() >= 3 && args[1] == "--analyze" {
        let version = parse_version_flag(&args, 3);
        analyze_directory(&args[2], version);
        return;
    }

    if args.len() < 2 {
        eprintln!("Usage:");
        eprintln!("  mtparser <test_file>                         Parse a single file");
        eprintln!("  mtparser --analyze <dir> [--version X.Y]      Analyze test files");
        eprintln!("  mtparser --version-errors <dir> [--version X.Y]  Version errors");
        std::process::exit(1);
    }

    let filepath = &args[1];
    let content = match fs::read_to_string(filepath) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading {}: {}", filepath, e);
            std::process::exit(1);
        }
    };

    let config = ParserConfig::default();
    let result = mtparser::parser::parse(&content, config);

    match result {
        Ok(ast) => println!("{:#?}", ast),
        Err(e) => {
            eprintln!("Parse error: {}", e);
            std::process::exit(1);
        }
    }
}

fn parse_version_flag(args: &[String], base: usize) -> MysqlVersion {
    if args.len() > base + 1 && args[base] == "--version" {
        match args[base + 1].as_str() {
            "5.7" => MysqlVersion::V57,
            "8.0" => MysqlVersion::V80,
            "8.4" => MysqlVersion::V84,
            "9.7" => MysqlVersion::V97,
            "mariadb-10.11" => MysqlVersion::MariaDB_1011,
            "mariadb-11.4" => MysqlVersion::MariaDB_114,
            "mariadb-11.8" => MysqlVersion::MariaDB_118,
            "mariadb-12.3" => MysqlVersion::MariaDB_123,
            "mariadb" => MysqlVersion::MariaDB,
            _ => MysqlVersion::Compatible,
        }
    } else {
        MysqlVersion::Compatible
    }
}

fn show_version_errors(dir: &str, version: MysqlVersion) {
    use std::collections::HashSet;
    let mut cmd_files: HashMap<String, Vec<String>> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();

    for path in find_test_files(dir) {
        let content = match fs::read(&path) { Ok(c) => c, Err(_) => continue };
        let config = ParserConfig::new(version);
        if let Err(e) = mtparser::parser::parse_bytes(&content, config) {
            let msg = format!("{}", e);
            if let Some(rest) = msg.strip_prefix("version error: command '") {
                if let Some(cmd) = rest.split('\'').next() {
                    let fname = path.file_name().unwrap().to_string_lossy().to_string();
                    cmd_files.entry(cmd.to_string()).or_default().push(fname.clone());
                    if !seen.contains(cmd) {
                        seen.insert(cmd.to_string());
                        println!("[{}] in {} -- {}", cmd, fname, msg);
                    }
                }
            }
        }
    }

    println!("\nSummary (version {:?}):", version);
    let mut sorted: Vec<_> = cmd_files.iter().collect();
    sorted.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    for (cmd, files) in sorted {
        println!("  {:4}  {} ({})", files.len(), cmd, files[..3.min(files.len())].join(", "));
    }
}

fn categorize_error(msg: &str) -> &'static str {
    if msg.contains("expected delimiter") { "delimiter" }
    else if msg.contains("let syntax") { "let_syntax" }
    else if msg.contains("replace_regex") || msg.contains("replace_column") || msg.contains("replace_result") { "replace" }
    else if msg.contains("version error") { "version" }
    else if msg.contains("unterminated block") { "unterminated_block" }
    else if msg.contains("unterminated") { "unterminated" }
    else if msg.contains("unknown command") { "unknown_cmd" }
    else if msg.contains("write_file") || msg.contains("append_file") { "file_io" }
    else { "syntax" }
}

/// Recursively find all .test and .inc files under a directory.
fn find_test_files(dir: &str) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if !std::path::Path::new(dir).exists() { return files; }
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() { files.extend(find_test_files(path.to_str().unwrap())); continue; }
        match path.extension().and_then(|e| e.to_str()) {
            Some("test") | Some("inc") => files.push(path),
            _ => {}
        }
    }
    files
}

fn analyze_directory(dir: &str, version: MysqlVersion) {
    let mut fail_cats: HashMap<&'static str, Vec<String>> = HashMap::new();
    let mut total = 0usize;
    let mut ok = 0usize;

    for path in find_test_files(dir) {
        total += 1;
        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        let content = match fs::read(&path) {
            Ok(c) => c,
            Err(_) => {
                fail_cats.entry("utf8").or_default().push(filename.clone());
                continue;
            }
        };

        let config = ParserConfig::new(version);
        match mtparser::parser::parse_bytes(&content, config) {
            Ok(_) => ok += 1,
            Err(e) => { fail_cats.entry(categorize_error(&format!("{}", e))).or_default().push(filename.clone()); }
        }
    }

    let ver_str = format!("{:?}", version);
    println!("=== {} (version {}) ===", dir, ver_str);
    println!(
        "Total: {}, Parsed: {}/{} ({:.1}%)",
        total, ok, total, ok as f64 / total as f64 * 100.0,
    );

    if !fail_cats.is_empty() {
        println!("\nFailures:");
        let mut sorted: Vec<_> = fail_cats.iter().collect();
        sorted.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
        for (cat, files) in sorted {
            println!("  {:4}  {} ({})", files.len(), cat, files[..3.min(files.len())].join(", "));
            if files.len() > 3 { println!("       +{} more", files.len() - 3); }
        }
    }
}
