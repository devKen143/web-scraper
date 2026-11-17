use serde_json::Value;
use std::io::Write;
use std::time::Duration;
use url::Url;
use web_scraper::crawler::{Config, Crawler};
use web_scraper::extract::Rule;

const USAGE: &str = "\
scrape — polite concurrent crawler + CSS-selector extractor

USAGE:
  scrape <url>... [options]

OPTIONS:
  -e, --extract name=selector[@attr]   field to extract (repeatable)
  --max-pages N        stop after N pages (default 50)
  --depth N            link depth from the seeds (default 2, 0 = seeds only)
  --concurrency N      parallel requests (default 8)
  --delay-ms N         min delay between requests to one host (default 500)
  --any-host           follow links to other hosts
  --include TEXT       only follow URLs containing TEXT (repeatable)
  --exclude TEXT       never follow URLs containing TEXT (repeatable)
  --ignore-robots      do not read robots.txt
  --format jsonl|csv   output format (default jsonl)
  -o FILE              write to FILE instead of stdout

EXAMPLE:
  scrape https://books.toscrape.com --depth 1 -e title=h3 -e price=.price_color
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprint!("{USAGE}");
        std::process::exit(2);
    }
    if let Err(e) = tokio::runtime::Runtime::new().unwrap().block_on(run(args)) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run(args: Vec<String>) -> Result<(), String> {
    let mut cfg = Config::default();
    let mut seeds = Vec::new();
    let mut format = "jsonl".to_string();
    let mut out: Option<String> = None;
    let mut i = 0;
    let num = |v: Option<&String>, f: &str| -> Result<u64, String> { v.ok_or(format!("{f} needs a value"))?.parse().map_err(|_| format!("bad value for {f}")) };
    while i < args.len() {
        let a = args[i].as_str();
        let v = args.get(i + 1);
        let mut takes = true;
        match a {
            "-e" | "--extract" => cfg.rules.push(Rule::parse(v.ok_or("--extract needs a value")?)?),
            "--max-pages" => cfg.max_pages = num(v, a)? as usize,
            "--depth" => cfg.max_depth = num(v, a)? as usize,
            "--concurrency" => cfg.concurrency = num(v, a)?.max(1) as usize,
            "--delay-ms" => cfg.delay = Duration::from_millis(num(v, a)?),
            "--include" => cfg.include.push(v.ok_or("--include needs a value")?.clone()),
            "--exclude" => cfg.exclude.push(v.ok_or("--exclude needs a value")?.clone()),
            "--format" => format = v.ok_or("--format needs a value")?.clone(),
            "-o" => out = Some(v.ok_or("-o needs a value")?.clone()),
            "--any-host" => {
                cfg.same_host = false;
                takes = false
            }
            "--ignore-robots" => {
                cfg.respect_robots = false;
                takes = false
            }
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            s => {
                seeds.push(Url::parse(s).map_err(|e| format!("{s}: {e}"))?);
                takes = false
            }
        }
        i += if takes { 2 } else { 1 };
    }
    if seeds.is_empty() {
        return Err("no seed URLs".into());
    }
    let rule_names: Vec<String> = cfg.rules.iter().map(|r| r.name.clone()).collect();
    let mut w: Box<dyn Write> = match &out {
        Some(p) => Box::new(std::io::BufWriter::new(std::fs::File::create(p).map_err(|e| e.to_string())?)),
        None => Box::new(std::io::stdout()),
    };
    let csv = format == "csv";
    if csv {
        let mut header = vec!["url".to_string(), "status".into(), "title".into()];
        header.extend(rule_names.iter().cloned());
        writeln!(w, "{}", header.iter().map(|h| csv_cell(h)).collect::<Vec<_>>().join(",")).ok();
    }
    let crawler = Crawler::new(cfg)?;
    let t0 = std::time::Instant::now();
    let stats = crawler
        .run(seeds, |rec| {
            if csv {
                let mut row = vec![rec["url"].as_str().unwrap_or("").to_string(), rec["status"].to_string(), rec["title"].as_str().unwrap_or("").to_string()];
                for n in &rule_names {
                    row.push(match &rec["data"][n] {
                        Value::Null => String::new(),
                        Value::String(s) => s.clone(),
                        Value::Array(a) => a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(" | "),
                        other => other.to_string(),
                    });
                }
                writeln!(w, "{}", row.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(",")).ok();
            } else {
                writeln!(w, "{rec}").ok();
            }
        })
        .await;
    w.flush().ok();
    eprintln!(
        "fetched {} pages ({} KB) in {:.1?}; {} errors, {} blocked by robots.txt, {} non-HTML skipped",
        stats.fetched,
        stats.bytes / 1024,
        t0.elapsed(),
        stats.errors,
        stats.blocked_by_robots,
        stats.skipped_non_html
    );
    Ok(())
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
