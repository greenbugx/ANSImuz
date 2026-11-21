import { useState } from 'react';
import { LINKS, VERSION } from '../content';

const SCREENSHOTS = [
  {
    id: 'back',
    className: 'deck-card-back',
    src: '/screenshots/2.webp',
    alt: 'ANSImuz playing Without Me - Eminem with lyrics and album cover',
    width: 1881,
    height: 933,
  },
  {
    id: 'middle',
    className: 'deck-card-middle',
    src: '/screenshots/3.webp',
    alt: 'ANSImuz command pallete',
    width: 1882,
    height: 933,
  },
  {
    id: 'front',
    className: 'deck-card-front',
    src: '/screenshots/1.webp',
    alt: 'ANSImuz main screen',
    width: 1882,
    height: 936,
  },
];

export default function Hero() {
  const [activeCard, setActiveCard] = useState<number | null>(null);

  return (
    <section className="hero" id="top">
      <div className="container hero-container">
        <div className="hero-top-actions">
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
          <a
            className="hero-brand-mark"
            href="#top"
            aria-label="ANSImuz home"
          >
            <img
              src="/logo.webp"
              alt="ANSImuz logo"
              width="46"
              height="46"
            />
          </a>
        </div>

        <h1 className="hero-title">
          <span className="hero-title-accent">ANSI</span>muz
          <span className="cursor" aria-hidden="true">
            _
          </span>
        </h1>

        <div className="hero-body">
          <div className="hero-copy">
            <p className="hero-tagline">
              A lightweight terminal music player with retro 8-bit ANSI vibes.
            </p>
            <p className="hero-description">
              Stream your favourite tracks from YouTube Music and Spotify, follow lyrics
              in sync, and manage your queue effortlessly. All without ever leaving
              the terminal.
            </p>
            <div className="hero-actions">
              <a className="button button-primary" href="#download">
                Get ANSImuz <span aria-hidden="true">↓</span>
              </a>
              <a className="button button-secondary" href="#docs">
                Read the docs <span aria-hidden="true">→</span>
              </a>
            </div>
            <dl className="hero-specs">
              <div>
                <dt>Platform</dt>
                <dd>Linux / Windows</dd>
              </div>
              <div>
                <dt>License</dt>
                <dd>MIT</dd>
              </div>
              <div>
                <dt>Version</dt>
                <dd>v{VERSION}</dd>
              </div>
            </dl>
          </div>

          <div
            className="hero-screenshots"
            aria-label="ANSImuz terminal interface screenshots"
          >
            <div className="screenshot-deck">
              {SCREENSHOTS.map((s, idx) => {
                const isDimmed = activeCard !== null && activeCard !== idx;
                const isActive = activeCard === idx;
                return (
                  <div
                    key={s.id}
                    className={`deck-card ${s.className}${isDimmed ? ' is-dimmed' : ''}${isActive ? ' is-active' : ''}`}
                    tabIndex={0}
                    role="button"
                    aria-label={s.alt}
                    onMouseEnter={() => setActiveCard(idx)}
                    onMouseLeave={() => setActiveCard(null)}
                    onFocus={() => setActiveCard(idx)}
                    onBlur={() => setActiveCard(null)}
                    onClick={() => setActiveCard(activeCard === idx ? null : idx)}
                  >
                    <img
                      src={s.src}
                      alt={s.alt}
                      width={s.width}
                      height={s.height}
                      loading="eager"
                    />
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
