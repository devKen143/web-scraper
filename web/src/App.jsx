import meta from './meta.js'
import Demo from './Demo.jsx'

export default function App() {
  return (
    <div className="page">
      <header className="hero">
        <div className="wrap">
          <span className="badge">{meta.stack}</span>
          <h1>{meta.name}</h1>
          <p className="tagline">{meta.tagline}</p>
          <div className="actions">
            <a className="btn primary" href="#demo">Try the demo</a>
            <a className="btn" href={meta.repo} target="_blank" rel="noreferrer">View source on GitHub</a>
          </div>
        </div>
      </header>
      <main className="wrap">
        <section>
          <h2>What it does</h2>
          <p>{meta.description}</p>
          <ul className="features">{meta.features.map((f) => <li key={f}>{f}</li>)}</ul>
        </section>
        <section id="demo">
          <h2>Interactive demo</h2>
          <p className="muted">{meta.demoNote}</p>
          <div className="card demo"><Demo /></div>
        </section>
        <section>
          <h2>How it works</h2>
          <div className="grid">
            {meta.architecture.map((a) => (
              <div className="card" key={a.title}><h3>{a.title}</h3><p>{a.text}</p></div>
            ))}
          </div>
        </section>
        <section>
          <h2>Run it locally</h2>
          <pre><code>{meta.usage}</code></pre>
        </section>
      </main>
      <footer className="wrap muted">Built in Rust · <a href={meta.repo}>{meta.repo.replace('https://', '')}</a></footer>
    </div>
  )
}
