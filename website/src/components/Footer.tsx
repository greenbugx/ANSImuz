import { ASCII_LOGO, LINKS, SECTIONS } from '../content';

const year = new Date().getFullYear();

export default function Footer() {
  return (
    <footer className="site-footer">
      <div className="container">
        <div className="footer-top">
          <div className="footer-brand">
            <a className="brand" href="#top" aria-label="ANSImuz home">
              <span className="brand-mark">
                <img src="/logo.webp" alt="" width="30" height="30" />
              </span>
              <span className="brand-name">ANSImuz</span>
            </a>
            <p>
              A terminal music player with 8-bit ANSI vibes. Built in Rust & Python
            </p>
          </div>

          <div className="footer-columns">
            <div className="footer-column">
              <h3>Product</h3>
              <ul>
                {SECTIONS.map((section) => (
                  <li key={section.id}>
                    <a href={`#${section.id}`}>{section.label}</a>
                  </li>
                ))}
              </ul>
            </div>
            <div className="footer-column">
              <h3>Project</h3>
              <ul>
                <li>
                  <a href={LINKS.repo} target="_blank" rel="noreferrer">
                    GitHub
                  </a>
                </li>
                <li>
                  <a href={LINKS.releases} target="_blank" rel="noreferrer">
                    Releases
                  </a>
                </li>
                <li>
                  <a href={LINKS.issues} target="_blank" rel="noreferrer">
                    Issues
                  </a>
                </li>
                <li>
                  <a href={LINKS.contributing} target="_blank" rel="noreferrer">
                    Contributing
                  </a>
                </li>
              </ul>
            </div>
            <div className="footer-column">
              <h3>Legal</h3>
              <ul>
                <li>
                  <a href={LINKS.license} target="_blank" rel="noreferrer">
                    MIT license
                  </a>
                </li>
                <li>
                  <a href={LINKS.thirdParty} target="_blank" rel="noreferrer">
                    Third-party licenses
                  </a>
                </li>
                <li>
                  <a href={LINKS.conduct} target="_blank" rel="noreferrer">
                    Code of conduct
                  </a>
                </li>
              </ul>
            </div>
          </div>
        </div>

        <pre className="footer-ascii" aria-hidden="true">
          {ASCII_LOGO}
        </pre>

        <div className="footer-bottom">
          <p>© {year} ANSImuz</p>
          <div className="footer-bottom-links">
            <a className="text-link" href="#top">
              Back to top <span aria-hidden="true">↑</span>
            </a>
          </div>
        </div>
      </div>
    </footer>
  );
}
