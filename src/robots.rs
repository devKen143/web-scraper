//! robots.txt parsing (REP / RFC 9309 subset): user-agent groups, allow/disallow
//! with `*` and `$` wildcards, longest-match wins, and crawl-delay.

#[derive(Debug, Clone, Default)]
pub struct Robots {
    rules: Vec<(bool, String)>, // (allow, pattern)
    pub crawl_delay: Option<f64>,
    pub sitemaps: Vec<String>,
}

impl Robots {
    /// Parses robots.txt, keeping the group that best matches `agent` (falls back to `*`).
    pub fn parse(text: &str, agent: &str) -> Self {
        let agent = agent.to_ascii_lowercase();
        struct Group {
            agents: Vec<String>,
            rules: Vec<(bool, String)>,
            delay: Option<f64>,
        }
        let mut groups: Vec<Group> = Vec::new();
        let mut sitemaps = Vec::new();
        let mut last_was_agent = false;
        for raw in text.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            let Some((k, v)) = line.split_once(':') else { continue };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            match k.as_str() {
                "user-agent" => {
                    if !last_was_agent || groups.is_empty() {
                        groups.push(Group { agents: vec![], rules: vec![], delay: None });
                    }
                    groups.last_mut().unwrap().agents.push(v.to_ascii_lowercase());
                    last_was_agent = true;
                    continue;
                }
                "allow" | "disallow" => {
                    if let Some(g) = groups.last_mut() {
                        if !v.is_empty() {
                            g.rules.push((k == "allow", v.to_string()));
                        }
                    }
                }
                "crawl-delay" => {
                    if let Some(g) = groups.last_mut() {
                        g.delay = v.parse().ok();
                    }
                }
                "sitemap" => sitemaps.push(v.to_string()),
                _ => {}
            }
            last_was_agent = false;
        }
        let specific = groups.iter().find(|g| g.agents.iter().any(|a| a != "*" && agent.contains(a.as_str())));
        let chosen = specific.or_else(|| groups.iter().find(|g| g.agents.iter().any(|a| a == "*")));
        match chosen {
            Some(g) => Robots { rules: g.rules.clone(), crawl_delay: g.delay, sitemaps },
            None => Robots { sitemaps, ..Default::default() },
        }
    }

    /// Is `path` (path + query) allowed? Longest matching pattern wins; ties go to allow.
    pub fn allowed(&self, path: &str) -> bool {
        let mut best: Option<(usize, bool)> = None;
        for (allow, pat) in &self.rules {
            if matches(pat, path) {
                let len = pat.len();
                best = match best {
                    Some((l, a)) if l > len || (l == len && a) => Some((l, a)),
                    _ => Some((len, *allow)),
                };
            }
        }
        best.map_or(true, |(_, a)| a)
    }
}

/// Glob match where `*` is any run of characters and a trailing `$` anchors the end.
fn matches(pattern: &str, path: &str) -> bool {
    // An unanchored pattern is a prefix match, i.e. the pattern followed by an implicit `*`.
    let full = match pattern.strip_suffix('$') {
        Some(p) => p.to_string(),
        None => format!("{pattern}*"),
    };
    wildcard(full.as_bytes(), path.as_bytes())
}

/// Classic greedy wildcard matcher with backtracking to the last `*`.
fn wildcard(p: &[u8], s: &[u8]) -> bool {
    let (mut pi, mut si) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while si < s.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some((pi, si));
            pi += 1;
        } else if pi < p.len() && p[pi] == s[si] {
            pi += 1;
            si += 1;
        } else if let Some((sp, ss)) = star {
            pi = sp + 1;
            si = ss + 1;
            star = Some((sp, ss + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == b'*')
}

#[cfg(test)]
mod tests {
    use super::*;

    const TXT: &str = "
        # comment
        User-agent: *
        Disallow: /private/
        Allow: /private/public-page
        Disallow: /*.pdf$
        Crawl-delay: 2

        User-agent: badbot
        Disallow: /

        Sitemap: https://example.com/sitemap.xml
    ";

    #[test]
    fn star_group() {
        let r = Robots::parse(TXT, "rust-scraper/0.1");
        assert!(r.allowed("/"));
        assert!(!r.allowed("/private/secret"));
        assert!(r.allowed("/private/public-page"));
        assert!(!r.allowed("/docs/file.pdf"));
        assert!(r.allowed("/docs/file.pdf?download=1"));
        assert_eq!(r.crawl_delay, Some(2.0));
        assert_eq!(r.sitemaps, vec!["https://example.com/sitemap.xml"]);
    }

    #[test]
    fn specific_group_wins() {
        let r = Robots::parse(TXT, "BadBot/2.0");
        assert!(!r.allowed("/anything"));
    }

    #[test]
    fn empty_disallow_allows_everything() {
        let r = Robots::parse("User-agent: *\nDisallow:\n", "x");
        assert!(r.allowed("/private"));
    }
}
