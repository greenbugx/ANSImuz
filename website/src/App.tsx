import Docs from './components/Docs';
import Download from './components/Download';
import Faq from './components/Faq';
import Features from './components/Features';
import Footer from './components/Footer';
import Hero from './components/Hero';
import Nav from './components/Nav';
import { useLenis } from './lib/useLenis';

export default function App() {
  useLenis();

  return (
    <>
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
      </main>
      <Footer />
    </>
  );
}
