# web-scraper

A polite, concurrent web crawler and CSS-selector scraper in Rust (tokio + reqwest + scraper).

```bash
scrape https://books.toscrape.com --depth 1 \
  -e "book=article.product_pod h3 a@title" -e "price=.price_color" --format csv -o books.csv
```

## Features
- Breadth-first crawl with bounded concurrency, depth and page limits
- **robots.txt**: user-agent groups, Allow/Disallow with `*`/`$`, longest-match, `Crawl-delay`
- Per-host politeness: requests to one host are spaced by `max(--delay-ms, Crawl-delay)`
- URL normalisation and de-duplication, `<base href>`, `rel=nofollow`
- Same-host by default, `--any-host`, `--include`/`--exclude` substring filters
- Non-HTML responses skipped by Content-Type; 5 MB body cap
- Extraction rules `name=selector` (text) or `name=selector@attr` (attribute; href/src made absolute)
- Output as JSON Lines (default) or CSV

## Options
Run `scrape --help` for the full list.

## Tests
`cargo test` — unit tests for robots matching, normalisation and extraction, plus an end-to-end crawl of an in-process website.

## License
MIT
