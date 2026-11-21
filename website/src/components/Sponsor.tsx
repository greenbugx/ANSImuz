export default function Sponsor() {
  return (
    <section className="sponsor-section" id="sponsor">
      <div className="container">
        <div className="sponsor-inner">
          <p className="sponsor-heading">SPONSORED BY</p>
          <a
            href="https://github.com/PIYUSH-NEXTGEN/Manabi"
            className="sponsor-brand"
            target="_blank"
            rel="noreferrer"
            aria-label="Manabi on GitHub"
          >
            <img
              src="/sponsor1.webp"
              alt="Manabi"
              className="sponsor-logo"
              width="132"
              height="132"
              loading="lazy"
            />
            <span className="sponsor-name">MANABI</span>
          </a>
        </div>
      </div>
    </section>
  );
}

