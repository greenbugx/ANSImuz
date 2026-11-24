import { FEATURES } from '../content';
import SectionHeader from './SectionHeader';

export default function Features() {
  return (
    <section className="section" id="features">
      <div className="container">
        <SectionHeader
          index="01"
          label="Features"
          title="The sound of Simplicity."
          intro="Everything you need to enjoy music, right at your fingertips. Search, play, queue, and explore, all from one terminal."
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
          Adaptive layout across terminal sizes, shuffle and repeat modes (Off, All, One),
          and optional CLI mpv fallback.
        </p>
      </div>
    </section>
  );
}
