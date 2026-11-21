import { useEffect, useState } from 'react';
import { LINKS, SECTIONS, SECTION_IDS, VERSION } from '../content';
import { useActiveSection } from '../lib/useActiveSection';

export default function Nav() {
  const [scrolled, setScrolled] = useState(false);
  const active = useActiveSection(SECTION_IDS);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 8);
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => window.removeEventListener('scroll', onScroll);
  }, []);

  return (
    <>
      <header className={`site-header${scrolled ? ' is-scrolled' : ''}`}>
        <div className="container header-inner">
          <a className="brand" href="#top" aria-label="ANSImuz home">
            <span className="brand-mark">
              <img src="/logo.png" alt="" width="30" height="30" />
            </span>
          </a>

          <nav className="primary-nav" aria-label="Primary">
            {SECTIONS.map((section) => (
              <a
                key={section.id}
                href={`#${section.id}`}
                className={active === section.id ? 'is-active' : undefined}
                aria-current={active === section.id ? 'location' : undefined}
              >
                <span className="nav-index">{section.index}</span>
                {section.label}
              </a>
            ))}
          </nav>

          <div className="header-actions">
            <a
              className="version-link"
              href={LINKS.releases}
              target="_blank"
              rel="noreferrer"
            >
              v{VERSION}
            </a>
            <a
              className="github-link"
              href={LINKS.repo}
              target="_blank"
              rel="noreferrer"
            >
              GitHub <span aria-hidden="true">↗</span>
            </a>
          </div>
        </div>
      </header>

      <nav className="mobile-nav" aria-label="Sections">
        {SECTIONS.map((section) => (
          <a
            key={section.id}
            href={`#${section.id}`}
            className={active === section.id ? 'is-active' : undefined}
            aria-current={active === section.id ? 'location' : undefined}
          >
            <span>{section.index}</span>
            {section.label}
          </a>
        ))}
      </nav>
    </>
  );
}
