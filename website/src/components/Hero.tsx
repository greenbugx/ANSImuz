import { VERSION, WAVE } from '../content';

export default function Hero() {
  return (
    <section className="hero" id="top">
      <div className="container">
        <div className="hero-meta">
          <span className="eyebrow">
            <span className="dot" aria-hidden="true" />
            terminal music player
          </span>
          <span className="hero-meta-right">open source / mit</span>
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
              Stream from YouTube Music and Spotify, follow synchronized lyrics, and keep
              your queue moving without leaving the keyboard. Album art is drawn in the
              terminal, one block at a time.
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

          <figure className="hero-poster">
            <div className="poster-corners" aria-hidden="true">
              <span>+</span>
              <span>+</span>
              <span>+</span>
              <span>+</span>
            </div>
            <div className="poster-frame">
              <img
                src="/logo.png"
                alt="ANSImuz logo: a pixel dinosaur beside a vinyl record"
                width="1439"
                height="1325"
              />
            </div>
            <figcaption>
              <span>fig. 01</span>
              <span>8-bit / ANSI</span>
            </figcaption>
          </figure>
        </div>

        <div className="hero-wave">
          <pre className="hero-wave-art" aria-hidden="true">
            {WAVE}
          </pre>
          <span>search / stream / queue / repeat</span>
        </div>
      </div>
    </section>
  );
}
