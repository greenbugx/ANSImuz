import { CONTROLS, LINKS } from '../content';
import SectionHeader from './SectionHeader';

export default function Docs() {
  return (
    <section className="section" id="docs">
      <div className="container">
        <SectionHeader
          index="02"
          label="Docs"
          title="Everything is a keypress."
          intro="A short guide to the controls, the command palette, and the lyrics view. The full reference is in the README."
        />

        <div className="docs-grid">
          <aside className="docs-aside">
            <p className="eyebrow">quick start</p>
            <ol className="steps">
              <li>
                <span className="step-index">01</span>
                <div>
                  <h3>Install the players</h3>
                  <p>
                    ANSImuz needs <a href={LINKS.mpv} target="_blank" rel="noreferrer">mpv</a>{' '}
                    and <a href={LINKS.ytdlp} target="_blank" rel="noreferrer">yt-dlp</a> on
                    your PATH.
                  </p>
                </div>
              </li>
              <li>
                <span className="step-index">02</span>
                <div>
                  <h3>Install ANSImuz</h3>
                  <p>Use the package for your system. The download section has the commands.</p>
                </div>
              </li>
              <li>
                <span className="step-index">03</span>
                <div>
                  <h3>Run it</h3>
                  <p>
                    Type <code>ansimuz</code> in a terminal and press <kbd>?</kbd> for the
                    full keymap.
                  </p>
                </div>
              </li>
            </ol>
            <a
              className="text-link"
              href={LINKS.readme}
              target="_blank"
              rel="noreferrer"
            >
              Read the full docs <span aria-hidden="true">↗</span>
            </a>
          </aside>

          <div className="docs-main">
            <div className="docs-block">
              <div className="docs-block-head">
                <span className="eyebrow">keymap</span>
                <span className="docs-note">default bindings</span>
              </div>
              <dl className="keymap">
                {CONTROLS.map((control) => (
                  <div className="keymap-row" key={control.action}>
                    <dt>
                      <kbd>{control.key}</kbd>
                    </dt>
                    <dd>{control.action}</dd>
                  </div>
                ))}
              </dl>
            </div>

            <div className="docs-block">
              <div className="docs-block-head">
                <span className="eyebrow">lyrics view</span>
                <span className="docs-note">press L</span>
              </div>
              <div className="lyric-demo">
                <p className="lyric-line lyric-active">
                  <span aria-hidden="true">▶</span> lead vocal, left aligned
                </p>
                <p className="lyric-line lyric-duet">
                  <span aria-hidden="true">◀</span> duet vocal, right aligned
                </p>
                <p className="lyric-line lyric-bg">
                  <span aria-hidden="true">▷</span> background vocal, dim
                </p>
              </div>
              <p className="docs-caption">
                Syllables highlight in time with the audio. Scroll with j and k, or press c
                to recenter the active line.
              </p>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
