//! Crawls a tiny in-process website.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;
use web_scraper::crawler::{Config, Crawler};
use web_scraper::extract::Rule;

async fn site(pages: HashMap<&'static str, (&'static str, &'static str)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let hits = Arc::new(Mutex::new(Vec::new()));
    let h = hits.clone();
    tokio::spawn(async move {
        loop {
            let (mut s, _) = l.accept().await.unwrap();
            let pages = pages.clone();
            let h = h.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                h.lock().unwrap().push(path.clone());
                let resp = match pages.get(path.as_str()) {
                    Some((ctype, body)) => format!("HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()),
                    None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                };
                s.write_all(resp.as_bytes()).await.ok();
            });
        }
    });
    (base, hits)
}

#[tokio::test]
async fn crawls_extracts_and_obeys_robots() {
    let html = "text/html; charset=utf-8";
    let pages = HashMap::from([
        ("/robots.txt", ("text/plain", "User-agent: *\nDisallow: /secret\n")),
        ("/", (html, r#"<title>Home</title><a href="/a">A</a><a href="/b#x">B</a><a href="/secret">S</a><a href="/doc.pdf">pdf</a><a href="https://elsewhere.invalid/">out</a>"#)),
        ("/a", (html, r#"<title>A</title><h1 class="t">Alpha</h1><a href="/">home</a><a href="/c">C</a>"#)),
        ("/b", (html, r#"<title>B</title><h1 class="t">Beta</h1>"#)),
        ("/c", (html, r#"<title>C</title><h1 class="t">Gamma</h1>"#)),
        ("/doc.pdf", ("application/pdf", "%PDF")),
        ("/secret", (html, "<title>nope</title>")),
    ]);
    let (base, hits) = site(pages).await;
    let cfg = Config { delay: Duration::from_millis(1), max_depth: 1, rules: vec![Rule::parse("heading=h1.t").unwrap()], ..Default::default() };
    let crawler = Crawler::new(cfg).unwrap();
    let mut records = Vec::new();
    let stats = crawler.run(vec![Url::parse(&format!("{base}/")).unwrap()], |r| records.push(r)).await;

    let titles: Vec<&str> = records.iter().filter_map(|r| r["title"].as_str()).collect();
    assert!(titles.contains(&"Home") && titles.contains(&"A") && titles.contains(&"B"));
    assert!(!titles.contains(&"C"), "depth limit of 1 must stop at /c");
    assert!(!titles.contains(&"nope"), "robots.txt disallows /secret");
    assert_eq!(stats.blocked_by_robots, 1);
    assert_eq!(stats.skipped_non_html, 1);
    assert!(!hits.lock().unwrap().contains(&"/secret".to_string()));
    let a = records.iter().find(|r| r["title"] == "A").unwrap();
    assert_eq!(a["data"]["heading"], "Alpha");
}
