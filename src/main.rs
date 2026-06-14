use std::collections::HashMap;
use std::env;
use std::fs;

use mtparser::error::ParseMode;
use mtparser::parser::ParserConfig;
use mtparser::version::MysqlVersion;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() >= 3 && args[1] == "--version-errors" {
        let version = if args.len() >= 5 && args[3] == "--version" {
            match args[4].as_str() {
                "5.7" => MysqlVersion::V57,
                "8.0" => MysqlVersion::V80,
                "8.4" => MysqlVersion::V84,
                "9.7" => MysqlVersion::V97,
                _ => MysqlVersion::V57,
            }
        } else {
            MysqlVersion::V80
        };
        show_version_errors(&args[2], version);
        return;
    }

    if args.len() >= 3 && args[1] == "--analyze" {
        let version = if args.len() >= 5 && args[3] == "--version" {
            match args[4].as_str() {
                "5.7" => MysqlVersion::V57,
                "8.0" => MysqlVersion::V80,
                "8.4" => MysqlVersion::V84,
                "9.7" => MysqlVersion::V97,
                _ => MysqlVersion::V57,
            }
        } else {
            MysqlVersion::V57
        };
        analyze_directory(&args[2], version);
        return;
    }

    if args.len() < 2 {
        eprintln!("Usage:");
        eprintln!("  mtparser [--strict] <test_file>          Parse a single file");
        eprintln!("  mtparser --analyze <dir> [--version X.Y]  Analyze test files");
        eprintln!("  mtparser --version-errors <dir> [--version X.Y]  Version errors");
        std::process::exit(1);
    }

    let strict = args[1] == "--strict";
    let filepath = if strict { &args[2] } else { &args[1] };
    let content = match fs::read_to_string(filepath) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading {}: {}", filepath, e);
            std::process::exit(1);
        }
    };

    let config = ParserConfig::new(MysqlVersion::V80, if strict { ParseMode::Strict } else { ParseMode::Lenient });
    let result = mtparser::parser::parse(&content, config);

    match result {
        Ok(ast) => println!("{:#?}", ast),
        Err(e) => {
            eprintln!("Parse error: {}", e);
            std::process::exit(1);
        }
    }
}

fn show_version_errors(dir: &str, version: MysqlVersion) {
    use std::collections::HashSet;
    let mut cmd_files: HashMap<String, Vec<String>> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();

    for path in find_test_files(dir) {
        let content = match fs::read(&path) { Ok(c) => c, Err(_) => continue };
        let config = ParserConfig::new(version, ParseMode::Strict);
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
    let mut lenient_cats: HashMap<&'static str, Vec<String>> = HashMap::new();
    let mut strict_cats: HashMap<&'static str, Vec<String>> = HashMap::new();
    let mut total = 0usize;
    let mut lenient_ok = 0usize;
    let mut strict_ok = 0usize;

    for path in find_test_files(dir) {
        total += 1;
        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        let content = match fs::read(&path) {
            Ok(c) => c,
            Err(_) => {
                lenient_cats.entry("utf8").or_default().push(filename.clone());
                strict_cats.entry("utf8").or_default().push(filename.clone());
                continue;
            }
        };

        // Lenient mode
        let config = ParserConfig::new(version, ParseMode::Lenient);
        match mtparser::parser::parse_bytes(&content, config) {
            Ok(_) => lenient_ok += 1,
            Err(e) => { lenient_cats.entry(categorize_error(&format!("{}", e))).or_default().push(filename.clone()); }
        }

        // Strict mode
        let config = ParserConfig::new(version, ParseMode::Strict);
        match mtparser::parser::parse_bytes(&content, config) {
            Ok(_) => strict_ok += 1,
            Err(e) => { strict_cats.entry(categorize_error(&format!("{}", e))).or_default().push(filename.clone()); }
        }
    }

    let ver_str = format!("{:?}", version);
    println!("=== {} (version {}) ===", dir, ver_str);
    println!(
        "Total: {}, Lenient: {}/{} ({:.1}%), Strict: {}/{} ({:.1}%)",
        total, lenient_ok, total, lenient_ok as f64 / total as f64 * 100.0,
        strict_ok, total, strict_ok as f64 / total as f64 * 100.0,
    );

    if !strict_cats.is_empty() {
        println!("\nStrict failures:");
        let mut sorted: Vec<_> = strict_cats.iter().collect();
        sorted.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
        for (cat, files) in sorted {
            println!("  {:4}  {} ({})", files.len(), cat, files[..3.min(files.len())].join(", "));
            if files.len() > 3 { println!("       +{} more", files.len() - 3); }
        }
    }

    // Report files that fail in BOTH lenient and strict (real parser bugs)
    let mut both_fail: Vec<&String> = Vec::new();
    for (cat, files) in &lenient_cats {
        if strict_cats.contains_key(*cat) {
            both_fail.extend(files.iter());
        }
    }
    if !both_fail.is_empty() {
        both_fail.sort();
        both_fail.dedup();
        println!("\nFiles failing in BOTH modes ({}):", both_fail.len());
        for f in &both_fail {
            println!("  {}", f);
        }
    }

    if !lenient_cats.is_empty() && lenient_cats != strict_cats {
        println!("\nLenient failures (beyond strict):");
        let mut sorted: Vec<_> = lenient_cats.iter().collect();
        sorted.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
        for (cat, files) in sorted {
            if strict_cats.contains_key(*cat) { continue; }
            println!("  {:4}  {} ({})", files.len(), cat, files[..3.min(files.len())].join(", "));
            if files.len() > 3 { println!("       +{} more", files.len() - 3); }
        }
    }
}
