export default {
  name: 'web-scraper',
  stack: 'Rust · tokio · reqwest · scraper',
  tagline: 'A polite, concurrent crawler that follows links breadth-first and extracts structured data with CSS selectors.',
  repo: 'https://github.com/devKen143/web-scraper',
  description:
    'Give it seed URLs and extraction rules like price=.price_color or image=img@src. It crawls breadth-first with a bounded number of parallel requests, honours robots.txt and Crawl-delay, spaces requests to each host, de-duplicates normalised URLs, and writes one JSON object (or CSV row) per page.',
  features: [
    'robots.txt: user-agent groups, Allow/Disallow with * and $ wildcards, longest match wins, Crawl-delay',
    'Per-host politeness slots so concurrency never means hammering one server',
    'URL normalisation (fragments, empty queries, default ports) and <base href> support',
    'rel="nofollow" respected, non-HTML responses skipped by Content-Type, body size cap',
    'Depth and page limits, same-host by default, --include / --exclude filters',
    'Extraction rules: text content or any attribute, href/src resolved to absolute URLs',
    'JSON Lines or CSV output with per-page status, timing and link counts',
  ],
  demoNote: 'Try the extraction rules on any HTML (parsed by your browser, the Rust tool uses the html5ever parser), and test robots.txt rules with the same wildcard matcher.',
  architecture: [
    { title: 'crawler.rs', text: 'A frontier queue plus a tokio JoinSet capped at --concurrency. Results feed new links back into the frontier until depth or page limits are hit.' },
    { title: 'Politeness', text: 'Each host has a "next slot" timestamp. A task reserves a slot (max of --delay and Crawl-delay) and sleeps until it, so per-host spacing holds even with many tasks.' },
    { title: 'robots.rs', text: 'Groups are matched by user-agent with * as the fallback. Patterns compile to a greedy wildcard matcher with backtracking.' },
    { title: 'extract.rs', text: 'html5ever via the scraper crate. Links are resolved against <base>, normalised and de-duplicated; rules become JSON strings, arrays or null.' },
  ],
  usage: `git clone https://github.com/devKen143/web-scraper && cd web-scraper
cargo build --release

./target/release/scrape https://books.toscrape.com --depth 1 \\
    -e "book=article.product_pod h3 a@title" \\
    -e "price=.price_color" --format csv -o books.csv

./target/release/scrape https://example.com --max-pages 200 --concurrency 16 --delay-ms 250`,
}
