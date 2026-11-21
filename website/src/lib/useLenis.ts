import { useEffect } from 'react';
import Lenis from 'lenis';

export function useLenis() {
  useEffect(() => {
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
    const initialHash = window.location.hash;
    const initialTarget =
      initialHash && initialHash !== '#'
        ? document.querySelector<HTMLElement>(initialHash)
        : null;

    if (reducedMotion.matches) {
      initialTarget?.scrollIntoView();
      return;
    }

    const lenis = new Lenis({
      autoRaf: true,
      duration: 1.15,
      smoothWheel: true,
    });

    if (initialTarget) {
      lenis.scrollTo(initialTarget, { immediate: true });
    }

    const handleClick = (event: MouseEvent) => {
      const target = event.target;
      if (!(target instanceof Element)) return;

      const anchor = target.closest<HTMLAnchorElement>('a[href^="#"]');
      if (!anchor) return;
      if (anchor.classList.contains('skip-link')) return;

      const hash = anchor.getAttribute('href');
      if (!hash || hash === '#') return;

      const section = document.querySelector<HTMLElement>(hash);
      if (!section) return;

      event.preventDefault();
      lenis.scrollTo(section, { duration: 1.1 });
      window.history.pushState(null, '', hash);
    };

    document.addEventListener('click', handleClick);

    return () => {
      document.removeEventListener('click', handleClick);
      lenis.destroy();
    };
  }, []);
}
