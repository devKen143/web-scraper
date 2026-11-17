//! Link discovery, URL normalisation and CSS-selector field extraction.

use scraper::{Html, Selector};
use serde_json::{Map, Value};
use url::Url;

/// A named field to pull out of each page: `name=selector` or `name=selector@attr`.
#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub selector: Selector,
    pub attr: Option<String>,
    pub raw: String,
}

impl Rule {
    pub fn parse(spec: &str) -> Result<Rule, String> {
        let (name, rest) = spec.split_once('=').ok_or_else(|| format!("rule '{spec}' must look like name=selector[@attr]"))?;
        let (sel, attr) = match rest.rsplit_once('@') {
            Some((s, a)) if !a.contains(']') && !a.contains(' ') => (s, Some(a.to_string())),
            _ => (rest, None),
        };
        let selector = Selector::parse(sel).map_err(|e| format!("bad selector '{sel}': {e:?}"))?;
        Ok(Rule { name: name.trim().to_string(), selector, attr, raw: spec.to_string() })
    }
}

/// Strips fragments and default ports, lowercases the host and drops empty queries,
/// so the same page is never queued twice under two spellings.
pub fn normalize(u: &Url) -> Option<Url> {
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    let mut u = u.clone();
    u.set_fragment(None);
    if u.query() == Some("") {
        u.set_query(None);
    }
    // the url crate already drops default ports (":443" on https), so nothing to do there
    Some(u)
}

pub struct Page {
    pub title: Option<String>,
    pub links: Vec<Url>,
    pub fields: Map<String, Value>,
}

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse_page(base: &Url, html: &str, rules: &[Rule]) -> Page {
    let doc = Html::parse_document(html);
    let a = Selector::parse("a[href]").unwrap();
    let base_sel = Selector::parse("base[href]").unwrap();
    let title_sel = Selector::parse("title").unwrap();
    let base = doc
        .select(&base_sel)
        .next()
        .and_then(|b| b.value().attr("href"))
        .and_then(|h| base.join(h).ok())
        .unwrap_or_else(|| base.clone());

    let mut links = Vec::new();
    for el in doc.select(&a) {
        let rel = el.value().attr("rel").unwrap_or("");
        if rel.split_whitespace().any(|r| r.eq_ignore_ascii_case("nofollow")) {
            continue;
        }
        if let Some(u) = el.value().attr("href").and_then(|h| base.join(h.trim()).ok()).and_then(|u| normalize(&u)) {
            if !links.contains(&u) {
                links.push(u);
            }
        }
    }

    let mut fields = Map::new();
    for r in rules {
        let values: Vec<Value> = doc
            .select(&r.selector)
            .filter_map(|el| match &r.attr {
                Some(attr) => el.value().attr(attr).map(|v| {
                    // resolve URL-ish attributes so output is directly usable
                    if matches!(attr.as_str(), "href" | "src") {
                        base.join(v).map(|u| u.to_string()).unwrap_or_else(|_| v.to_string())
                    } else {
                        v.to_string()
                    }
                }),
                None => Some(clean(&el.text().collect::<String>())),
            })
            .filter(|s| !s.is_empty())
            .map(Value::String)
            .collect();
        let v = match values.len() {
            0 => Value::Null,
            1 => values.into_iter().next().unwrap(),
            _ => Value::Array(values),
        };
        fields.insert(r.name.clone(), v);
    }
    Page { title: doc.select(&title_sel).next().map(|t| clean(&t.text().collect::<String>())), links, fields }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = r#"<html><head><title>  Shop
        Home </title></head><body>
        <a href="/products/1#reviews">One</a>
        <a href="products/2?">Two</a>
        <a href="https://EXAMPLE.com:443/products/1">Dup</a>
        <a href="mailto:hi@example.com">mail</a>
        <a href="/admin" rel="nofollow">admin</a>
        <div class="product"><h2>Kettle</h2><span class="price">$25</span><img src="/k.png"></div>
        <div class="product"><h2>Toaster</h2><span class="price">$40</span></div>
    </body></html>"#;

    #[test]
    fn finds_and_normalises_links() {
        let base = Url::parse("https://example.com/shop/").unwrap();
        let p = parse_page(&base, HTML, &[]);
        let links: Vec<String> = p.links.iter().map(|u| u.to_string()).collect();
        assert_eq!(links, vec!["https://example.com/products/1", "https://example.com/shop/products/2"]);
        assert_eq!(p.title.as_deref(), Some("Shop Home"));
    }

    #[test]
    fn extracts_fields() {
        let base = Url::parse("https://example.com/").unwrap();
        let rules = ["name=.product h2", "price=.product .price", "image=.product img@src", "missing=.nope"].map(|r| Rule::parse(r).unwrap());
        let p = parse_page(&base, HTML, &rules);
        assert_eq!(p.fields["name"], serde_json::json!(["Kettle", "Toaster"]));
        assert_eq!(p.fields["image"], "https://example.com/k.png");
        assert_eq!(p.fields["missing"], Value::Null);
    }

    #[test]
    fn rule_parsing() {
        assert!(Rule::parse("nosel").is_err());
        assert_eq!(Rule::parse("l=a[href]@href").unwrap().attr.as_deref(), Some("href"));
        assert!(Rule::parse("x=input[type=email]").unwrap().attr.is_none());
    }
}
