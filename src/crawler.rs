//! Breadth-first concurrent crawler with per-host politeness and robots.txt.

use crate::extract::{normalize, parse_page, Rule};
use crate::robots::Robots;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use url::Url;

#[derive(Clone)]
pub struct Config {
    pub max_pages: usize,
    pub max_depth: usize,
    pub concurrency: usize,
    pub delay: Duration,
    pub same_host: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub respect_robots: bool,
    pub user_agent: String,
    pub timeout: Duration,
    pub max_body: usize,
    pub rules: Vec<Rule>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            max_pages: 50,
            max_depth: 2,
            concurrency: 8,
            delay: Duration::from_millis(500),
            same_host: true,
            include: vec![],
            exclude: vec![],
            respect_robots: true,
            user_agent: concat!("rust-web-scraper/", env!("CARGO_PKG_VERSION")).into(),
            timeout: Duration::from_secs(15),
            max_body: 5 * 1024 * 1024,
            rules: vec![],
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub fetched: usize,
    pub errors: usize,
    pub blocked_by_robots: usize,
    pub skipped_non_html: usize,
    pub bytes: usize,
}

struct State {
    robots: HashMap<String, Arc<Robots>>,
    next_slot: HashMap<String, Instant>,
}

pub struct Crawler {
    cfg: Config,
    client: reqwest::Client,
    state: Mutex<State>,
}

fn host_key(u: &Url) -> String {
    format!("{}://{}", u.scheme(), u.host_str().unwrap_or("").to_ascii_lowercase()) + &u.port().map(|p| format!(":{p}")).unwrap_or_default()
}

impl Crawler {
    pub fn new(cfg: Config) -> Result<Arc<Self>, String> {
        let client = reqwest::Client::builder()
            .user_agent(cfg.user_agent.clone())
            .timeout(cfg.timeout)
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Arc::new(Crawler { cfg, client, state: Mutex::new(State { robots: HashMap::new(), next_slot: HashMap::new() }) }))
    }

    async fn robots_for(&self, u: &Url) -> Arc<Robots> {
        let key = host_key(u);
        if let Some(r) = self.state.lock().await.robots.get(&key) {
            return r.clone();
        }
        let text = match self.client.get(format!("{key}/robots.txt")).send().await {
            Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
            _ => String::new(), // missing robots.txt means everything is allowed
        };
        let robots = Arc::new(Robots::parse(&text, &self.cfg.user_agent));
        self.state.lock().await.robots.insert(key, robots.clone());
        robots
    }

    /// Reserves the next polite time slot for this host and sleeps until it.
    async fn wait_turn(&self, u: &Url, robots: &Robots) {
        let delay = robots.crawl_delay.map(Duration::from_secs_f64).unwrap_or(Duration::ZERO).max(self.cfg.delay).min(Duration::from_secs(30));
        let at = {
            let mut s = self.state.lock().await;
            let now = Instant::now();
            let slot = s.next_slot.get(&host_key(u)).copied().unwrap_or(now).max(now);
            s.next_slot.insert(host_key(u), slot + delay);
            slot
        };
        tokio::time::sleep_until(at.into()).await;
    }

    fn wanted(&self, seed_hosts: &HashSet<String>, u: &Url) -> bool {
        let s = u.as_str();
        (!self.cfg.same_host || seed_hosts.contains(&host_key(u)))
            && (self.cfg.include.is_empty() || self.cfg.include.iter().any(|p| s.contains(p.as_str())))
            && !self.cfg.exclude.iter().any(|p| s.contains(p.as_str()))
    }

    async fn fetch(self: &Arc<Self>, url: Url, depth: usize) -> Result<(Value, Vec<Url>, usize), (String, &'static str)> {
        let robots = self.robots_for(&url).await;
        let path = match url.query() {
            Some(q) => format!("{}?{q}", url.path()),
            None => url.path().to_string(),
        };
        if self.cfg.respect_robots && !robots.allowed(&path) {
            return Err(("disallowed by robots.txt".into(), "robots"));
        }
        self.wait_turn(&url, &robots).await;
        let started = Instant::now();
        let resp = self.client.get(url.clone()).send().await.map_err(|e| (e.to_string(), "error"))?;
        let status = resp.status().as_u16();
        let final_url = resp.url().clone();
        let ctype = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        if !ctype.is_empty() && !ctype.contains("html") {
            return Err((format!("skipped content-type {ctype}"), "non_html"));
        }
        if resp.content_length().is_some_and(|n| n as usize > self.cfg.max_body) {
            return Err(("body too large".into(), "error"));
        }
        let bytes = resp.bytes().await.map_err(|e| (e.to_string(), "error"))?;
        let body = String::from_utf8_lossy(&bytes[..bytes.len().min(self.cfg.max_body)]);
        let page = parse_page(&final_url, &body, &self.cfg.rules);
        let record = json!({
            "url": url.as_str(),
            "final_url": final_url.as_str(),
            "status": status,
            "depth": depth,
            "title": page.title,
            "links": page.links.len(),
            "ms": started.elapsed().as_millis() as u64,
            "data": page.fields,
        });
        Ok((record, page.links, bytes.len()))
    }

    /// Crawls from `seeds`, calling `sink` with one JSON record per fetched page.
    pub async fn run(self: Arc<Self>, seeds: Vec<Url>, mut sink: impl FnMut(Value)) -> Stats {
        let mut stats = Stats::default();
        let seeds: Vec<Url> = seeds.iter().filter_map(normalize).collect();
        let seed_hosts: HashSet<String> = seeds.iter().map(host_key).collect();
        let mut seen: HashSet<String> = seeds.iter().map(|u| u.to_string()).collect();
        let mut frontier: VecDeque<(Url, usize)> = seeds.into_iter().map(|u| (u, 0)).collect();
        let mut tasks = JoinSet::new();
        let mut started = 0;

        loop {
            while tasks.len() < self.cfg.concurrency && started < self.cfg.max_pages {
                let Some((url, depth)) = frontier.pop_front() else { break };
                started += 1;
                let me = self.clone();
                tasks.spawn(async move { (depth, me.fetch(url, depth).await) });
            }
            let Some(joined) = tasks.join_next().await else { break };
            let Ok((depth, result)) = joined else {
                stats.errors += 1;
                continue;
            };
            match result {
                Ok((record, links, n)) => {
                    stats.fetched += 1;
                    stats.bytes += n;
                    sink(record);
                    if depth < self.cfg.max_depth {
                        for l in links {
                            if self.wanted(&seed_hosts, &l) && seen.insert(l.to_string()) {
                                frontier.push_back((l, depth + 1));
                            }
                        }
                    }
                }
                Err((msg, kind)) => match kind {
                    "robots" => stats.blocked_by_robots += 1,
                    "non_html" => stats.skipped_non_html += 1,
                    _ => {
                        stats.errors += 1;
                        eprintln!("warn: {msg}");
                    }
                },
            }
        }
        stats
    }
}
