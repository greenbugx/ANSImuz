import { useState } from 'react';
import Docs from './components/Docs';
import Download from './components/Download';
import Faq from './components/Faq';
import Features from './components/Features';
import Footer from './components/Footer';
import Hero from './components/Hero';
import Loader from './components/Loader';
import Nav from './components/Nav';
import Sponsor from './components/Sponsor';
import { useLenis } from './lib/useLenis';

export default function App() {
  const [isLoading, setIsLoading] = useState(true);
  useLenis();

  return (
    <>
      {isLoading && <Loader onComplete={() => setIsLoading(false)} />}
      <a className="skip-link" href="#main">
        Skip to content
      </a>
      <Nav />
      <main id="main" tabIndex={-1}>
        <Hero />
        <Features />
        <Docs />
        <Download />
        <Faq />
        <Sponsor />
      </main>
      <Footer />
    </>
  );
}
