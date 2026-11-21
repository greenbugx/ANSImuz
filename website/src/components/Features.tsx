import { FEATURES } from '../content';
import SectionHeader from './SectionHeader';

export default function Features() {
  return (
    <section className="section" id="features">
      <div className="container">
        <SectionHeader
          index="01"
          label="Features"
          title="Built for the keyboard."
          intro="Search, queue, lyrics, and artwork in one terminal window. No browser, no mouse, no extra tabs."
        />

        <ul className="feature-grid">
          {FEATURES.map((feature, index) => (
            <li className="feature-item" key={feature.title}>
              <div className="feature-head">
                <span className="feature-index">{String(index + 1).padStart(2, '0')}</span>
                <h3>{feature.title}</h3>
              </div>
              <p>{feature.description}</p>
            </li>
          ))}
        </ul>

        <p className="feature-footnote">
          <span>Also included</span>
          Shuffle and repeat modes, volume control, and Discord Rich Presence
          (experimental).
        </p>
      </div>
    </section>
  );
}
