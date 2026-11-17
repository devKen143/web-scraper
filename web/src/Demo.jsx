import { useMemo, useState } from 'react'

const SAMPLE_HTML = `<html><head><title>Corner Bookshop</title></head><body>
  <article class="book"><h3><a href="/b/1" title="The Rust Book">The Rust Bo…</a></h3>
    <p class="price">£29.99</p><img src="/covers/1.jpg"></article>
  <article class="book"><h3><a href="/b/2" title="Designing Data-Intensive Applications">Designing…</a></h3>
    <p class="price">£41.50</p><img src="/covers/2.jpg"></article>
  <article class="book"><h3><a href="/b/3" title="Crafting Interpreters">Crafting…</a></h3>
    <p class="price">£35.00</p><img src="/covers/3.jpg"></article>
  <a href="/page/2">next</a> <a href="/login" rel="nofollow">login</a>
</body></html>`
const SAMPLE_RULES = `title=article.book h3 a@title
price=article.book .price
cover=article.book img@src
next=a[href*=page]@href`
const SAMPLE_ROBOTS = `User-agent: *
Disallow: /private/
Allow: /private/press-kit
Disallow: /*.pdf$
Crawl-delay: 1`

function parseRule(line) {
  const eq = line.indexOf('=')
  if (eq < 0) return null
  const name = line.slice(0, eq).trim()
  let sel = line.slice(eq + 1).trim()
  let attr = null
  const at = sel.lastIndexOf('@')
  if (at > 0 && !sel.slice(at).includes(']') && !sel.slice(at).includes(' ')) { attr = sel.slice(at + 1); sel = sel.slice(0, at) }
  return { name, sel, attr }
}

function extract(html, rulesText, base) {
  const doc = new DOMParser().parseFromString(html, 'text/html')
  const out = {}
  const errors = []
  for (const line of rulesText.split('\n').map((l) => l.trim()).filter(Boolean)) {
    const r = parseRule(line)
    if (!r) { errors.push(`"${line}" must look like name=selector[@attr]`); continue }
    let els
    try { els = [...doc.querySelectorAll(r.sel)] } catch { errors.push(`bad selector: ${r.sel}`); continue }
    const vals = els.map((el) => {
      if (!r.attr) return el.textContent.split(/\s+/).filter(Boolean).join(' ')
      const v = el.getAttribute(r.attr)
      if (v != null && (r.attr === 'href' || r.attr === 'src')) { try { return new URL(v, base).href } catch { return v } }
      return v
    }).filter((v) => v)
    out[r.name] = vals.length === 0 ? null : vals.length === 1 ? vals[0] : vals
  }
  const links = [...doc.querySelectorAll('a[href]')].filter((a) => !/\bnofollow\b/i.test(a.getAttribute('rel') || '')).map((a) => { try { const u = new URL(a.getAttribute('href'), base); u.hash = ''; return u.href } catch { return null } }).filter(Boolean)
  return { title: doc.title, data: out, links: [...new Set(links)], errors }
}

// Same algorithm as robots.rs
function wildcard(p, s) {
  let pi = 0, si = 0, star = null
  while (si < s.length) {
    if (pi < p.length && p[pi] === '*') { star = [pi, si]; pi++ }
    else if (pi < p.length && p[pi] === s[si]) { pi++; si++ }
    else if (star) { pi = star[0] + 1; si = star[1] + 1; star = [star[0], star[1] + 1] }
    else return false
  }
  return p.slice(pi).split('').every((c) => c === '*')
}
const matches = (pat, path) => (pat.endsWith('$') ? wildcard(pat.slice(0, -1), path) : wildcard(pat + '*', path))
function robotsCheck(text, path) {
  let inStar = false, best = null
  for (const raw of text.split('\n')) {
    const line = raw.split('#')[0].trim()
    const i = line.indexOf(':')
    if (i < 0) continue
    const k = line.slice(0, i).trim().toLowerCase(), v = line.slice(i + 1).trim()
    if (k === 'user-agent') inStar = v === '*'
    else if (inStar && (k === 'allow' || k === 'disallow') && v && matches(v, path)) {
      if (!best || v.length > best.pat.length || (v.length === best.pat.length && k === 'allow')) best = { pat: v, allow: k === 'allow' }
    }
  }
  return best ? best : { pat: '(no rule)', allow: true }
}

export default function Demo() {
  const [tab, setTab] = useState('extract')
  const [html, setHtml] = useState(SAMPLE_HTML)
  const [rules, setRules] = useState(SAMPLE_RULES)
  const [robots, setRobots] = useState(SAMPLE_ROBOTS)
  const [path, setPath] = useState('/private/report.pdf')
  const result = useMemo(() => extract(html, rules, 'https://shop.example/'), [html, rules])
  const verdict = robotsCheck(robots, path)
  const cli = `scrape https://shop.example/ ${rules.split('\n').filter((l) => l.includes('=')).map((l) => `-e "${l.trim()}"`).join(' ')}`

  return (
    <div>
      <div className="row">
        <button className={tab === 'extract' ? 'primary' : ''} onClick={() => setTab('extract')}>Selector playground</button>
        <button className={tab === 'robots' ? 'primary' : ''} onClick={() => setTab('robots')}>robots.txt tester</button>
      </div>
      {tab === 'extract' ? (
        <div className="grid">
          <div>
            <h3>Page HTML</h3>
            <textarea rows={12} value={html} onChange={(e) => setHtml(e.target.value)} spellCheck={false} aria-label="HTML" />
            <h3 style={{ marginTop: 10 }}>Rules (name=selector[@attr])</h3>
            <textarea rows={5} value={rules} onChange={(e) => setRules(e.target.value)} spellCheck={false} aria-label="rules" />
          </div>
          <div>
            <h3>Output record</h3>
            {result.errors.map((e) => <p key={e} className="pill err">{e}</p>)}
            <pre style={{ margin: 0, maxHeight: 330, overflow: 'auto' }}>{JSON.stringify({ url: 'https://shop.example/', title: result.title, data: result.data }, null, 2)}</pre>
            <p className="muted" style={{ fontSize: 14 }}>{result.links.length} followable link(s): {result.links.join(', ')}</p>
            <h3>Equivalent command</h3>
            <pre style={{ whiteSpace: 'pre-wrap' }}>{cli}</pre>
          </div>
        </div>
      ) : (
        <div className="grid">
          <div><h3>robots.txt</h3><textarea rows={9} value={robots} onChange={(e) => setRobots(e.target.value)} spellCheck={false} aria-label="robots.txt" /></div>
          <div>
            <h3>Path to test</h3>
            <input value={path} onChange={(e) => setPath(e.target.value)} style={{ width: '100%' }} aria-label="path" />
            <p style={{ marginTop: 12 }}><span className={`pill ${verdict.allow ? 'ok' : 'err'}`}>{verdict.allow ? 'ALLOWED' : 'BLOCKED'}</span> by <code>{verdict.pat}</code></p>
            <p className="muted" style={{ fontSize: 14 }}>Try /private/press-kit, /docs/a.pdf?x=1, /about.</p>
          </div>
        </div>
      )}
    </div>
  )
}
