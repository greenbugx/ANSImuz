import { FAQS } from '../content';
import SectionHeader from './SectionHeader';

export default function Faq() {
  return (
    <section className="section" id="faq">
      <div className="container">
        <SectionHeader
          index="04"
          label="FAQ"
          title="Questions, answered."
          intro="The things people ask before they install. If something is missing, open an issue on GitHub."
        />

        <div className="faq-list">
          {FAQS.map((faq, index) => (
            <details className="faq-item" key={faq.question}>
              <summary>
                <span className="faq-index">{String(index + 1).padStart(2, '0')}</span>
                <span className="faq-question">{faq.question}</span>
                <span className="faq-marker" aria-hidden="true">
                  +
                </span>
              </summary>
              <div className="faq-answer">
                <p>{faq.answer}</p>
              </div>
            </details>
          ))}
        </div>
      </div>
    </section>
  );
}
