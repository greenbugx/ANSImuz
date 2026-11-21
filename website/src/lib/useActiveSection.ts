import { useEffect, useState } from 'react';

export function useActiveSection(ids: readonly string[]) {
  const [active, setActive] = useState('');

  useEffect(() => {
    const elements = ids
      .map((id) => document.getElementById(id))
      .filter((element): element is HTMLElement => element !== null);

    if (elements.length === 0) return;

    const visible = new Map<string, number>();

    const updateActive = () => {
      if (window.scrollY < 120) {
        setActive('');
        return;
      }

      const isAtBottom =
        window.innerHeight + window.scrollY >=
        document.documentElement.scrollHeight - 120;
      if (isAtBottom) {
        setActive(ids[ids.length - 1] ?? '');
        return;
      }

      const mostVisible = Array.from(visible.entries()).sort(
        (a, b) => b[1] - a[1],
      )[0];

      if (mostVisible) {
        setActive(mostVisible[0]);
      } else {
        const lastEl = elements[elements.length - 1];
        if (
          lastEl &&
          lastEl.getBoundingClientRect().bottom < window.innerHeight / 2
        ) {
          setActive(ids[ids.length - 1] ?? '');
        }
      }
    };

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            visible.set(entry.target.id, entry.intersectionRatio);
          } else {
            visible.delete(entry.target.id);
          }
        }
        updateActive();
      },
      { rootMargin: '-20% 0px -40% 0px', threshold: [0, 0.15, 0.5, 1] },
    );

    elements.forEach((element) => observer.observe(element));

    const onScroll = () => {
      if (
        window.scrollY < 120 ||
        window.innerHeight + window.scrollY >=
          document.documentElement.scrollHeight - 120
      ) {
        updateActive();
      }
    };

    window.addEventListener('scroll', onScroll, { passive: true });

    return () => {
      observer.disconnect();
      window.removeEventListener('scroll', onScroll);
    };
  }, [ids]);

  return active;
}
