import { LINKS, PLATFORMS } from '../content';
import CopyButton from './CopyButton';
import SectionHeader from './SectionHeader';

function CommandList({ commands, label }: { commands: string[]; label?: string }) {
  return (
    <div className="command-list">
      {label ? <span className="command-label">{label}</span> : null}
      {commands.map((command) => (
        <div className="command-row" key={command}>
          <code>{command}</code>
          <CopyButton value={command} />
        </div>
      ))}
    </div>
  );
}

export default function Download() {
  return (
    <section className="section" id="download">
      <div className="container">
        <SectionHeader
          index="03"
          label="Download"
          title="Install. Run. Play."
          intro="A portable Windows build, native Linux packages, or a source build. Extract or install, then start listening."
        />

        <div className="download-note">
          <span className="note-label">batteries included</span>
          <p>
            No standalone yt-dlp binary is required. Audio plays natively via{' '}
            <a href={LINKS.mpv} target="_blank" rel="noreferrer">libmpv</a>, and the Windows
            portable build bundles Python, yt-dlp modules, and pinned QuickJS out of the box.
          </p>
        </div>

        <ul className="download-list">
          {PLATFORMS.map((platform, index) => (
            <li className="download-row" key={platform.name}>
              <div className="download-head">
                <span className="download-index">{String(index + 1).padStart(2, '0')}</span>
                <div>
                  <h3>{platform.name}</h3>
                  <p>{platform.meta}</p>
                </div>
              </div>

              <div className="download-body">
                {platform.commands ? (
                  <CommandList
                    commands={platform.commands}
                    label={platform.commandLabel}
                  />
                ) : null}
                {platform.note ? <p className="download-caption">{platform.note}</p> : null}
              </div>

              <div className="download-action">
                <a
                  className={`button button-small ${
                    platform.primary ? 'button-primary' : 'button-secondary'
                  }`}
                  href={platform.href}
                  target="_blank"
                  rel="noreferrer"
                >
                  {platform.action} <span aria-hidden="true">↗</span>
                </a>
                {platform.secondary ? (
                  <a
                    className="text-link text-link-small"
                    href={platform.secondary.href}
                    target="_blank"
                    rel="noreferrer"
                  >
                    {platform.secondary.label} <span aria-hidden="true">↗</span>
                  </a>
                ) : null}
              </div>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
