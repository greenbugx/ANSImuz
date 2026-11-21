import { useEffect, useRef } from 'react';
import { CONTROLS, LINKS } from '../content';
import { getAssetBlob } from '../lib/assetCache';
import SectionHeader from './SectionHeader';

export default function Docs() {
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    let isIntersecting = false;

    const syncPlayback = () => {
      if (document.hidden || !isIntersecting) {
        if (!video.paused) {
          video.pause();
        }
      } else {
        if (video.paused) {
          video.play().catch(() => {});
        }
      }
    };

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          isIntersecting = entry.isIntersecting;
          syncPlayback();
        }
      },
      { threshold: 0.1 }
    );

    observer.observe(video);

    const onVisibilityChange = () => {
      syncPlayback();
    };

    document.addEventListener('visibilitychange', onVisibilityChange);

    return () => {
      observer.disconnect();
      document.removeEventListener('visibilitychange', onVisibilityChange);
    };
  }, []);
  return (
    <section className="section" id="docs">
      <div className="container">
        <SectionHeader
          index="02"
          label="Docs"
          title="THE ANSImuz field guide."
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
                    Type <code>ansimuz</code> in a terminal and press <kbd>:</kbd> for the
                    command pallete
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
              </div>
              <div className="docs-video-frame">
                <video
                  ref={videoRef}
                  src={getAssetBlob('/recordings/1.webm')}
                  autoPlay
                  loop
                  muted
                  playsInline
                  preload="metadata"
                  disablePictureInPicture
                  onEnded={(e) => {
                    e.currentTarget.currentTime = 0;
                    e.currentTarget.play().catch(() => {});
                  }}
                >
                  Your browser does not support the video tag.
                </video>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
