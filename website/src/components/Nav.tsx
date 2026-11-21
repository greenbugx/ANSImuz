import { SECTIONS, SECTION_IDS } from '../content';
import { useActiveSection } from '../lib/useActiveSection';

function HomeIcon() {
  return (
    <svg
      className="nav-item-icon"
      viewBox="0 0 16 16"
      width="15"
      height="15"
      fill="currentColor"
      aria-hidden="true"
    >
      <path d="M8 1.5l6 5.2V14a1 1 0 0 1-1 1h-3.5v-4.5h-3V15H3a1 1 0 0 1-1-1V6.7l6-5.2z" />
    </svg>
  );
}

function FeaturesIcon() {
  return (
    <svg
      className="nav-item-icon"
      viewBox="0 0 16 16"
      width="15"
      height="15"
      fill="currentColor"
      aria-hidden="true"
    >
      <path d="M1.5 9.5a1 1 0 0 1 1-1h1a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1h-1a1 1 0 0 1-1-1v-4zm5-6a1 1 0 0 1 1-1h1a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1h-1a1 1 0 0 1-1-1v-10zm5 3a1 1 0 0 1 1-1h1a1 1 0 0 1 1 1v7a1 1 0 0 1-1 1h-1a1 1 0 0 1-1-1v-7z" />
    </svg>
  );
}

function DocsIcon() {
  return (
    <svg
      className="nav-item-icon"
      viewBox="0 0 16 16"
      width="15"
      height="15"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M2.5 2.5h8a2 2 0 0 1 2 2v9a.5.5 0 0 1-.5.5h-9a.5.5 0 0 1-.5-.5v-10a1 1 0 0 1 1-1z" />
      <path d="M5 5.5h5M5 8.5h5M5 11.5h3" />
    </svg>
  );
}

function DownloadIcon() {
  return (
    <svg
      className="nav-item-icon"
      viewBox="0 0 16 16"
      width="15"
      height="15"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M8 2v8M4.5 7L8 10.5 11.5 7M2.5 13.5h11" />
    </svg>
  );
}

function FaqIcon() {
  return (
    <svg
      className="nav-item-icon"
      viewBox="0 0 16 16"
      width="15"
      height="15"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="8" cy="8" r="6.5" />
      <path d="M6.2 6.3a2 2 0 0 1 3.5.9c0 1.2-1.7 1.5-1.7 2.6" />
      <circle cx="8" cy="12" r="0.75" fill="currentColor" stroke="none" />
    </svg>
  );
}

const NAV_ITEMS = [
  { id: 'top', label: 'Home', icon: <HomeIcon /> },
  { id: 'features', label: 'Features', icon: <FeaturesIcon /> },
  { id: 'docs', label: 'Docs', icon: <DocsIcon /> },
  { id: 'download', label: 'Download', icon: <DownloadIcon /> },
  { id: 'faq', label: 'FAQ', icon: <FaqIcon /> },
];

export default function Nav() {
  const active = useActiveSection(SECTION_IDS);
  const currentActive = active || 'top';

  return (
    <>
      <header className="site-header">
        <nav className="primary-nav" aria-label="Primary">
          {NAV_ITEMS.map((item) => {
            const isActive = currentActive === item.id;
            return (
              <a
                key={item.id}
                href={`#${item.id}`}
                className={`nav-pill-item${isActive ? ' is-active' : ''}`}
                aria-current={isActive ? 'location' : undefined}
                aria-label={item.label}
              >
                <span className="nav-pill-icon">{item.icon}</span>
                <span className="nav-pill-label">{item.label}</span>
              </a>
            );
          })}
        </nav>
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
