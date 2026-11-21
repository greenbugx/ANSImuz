import { useEffect, useRef, useState } from 'react';

type CopyState = 'idle' | 'copied' | 'error';

export default function CopyButton({ value }: { value: string }) {
  const [state, setState] = useState<CopyState>('idle');
  const timeout = useRef<number | undefined>(undefined);

  useEffect(() => {
    return () => {
      if (timeout.current) window.clearTimeout(timeout.current);
    };
  }, []);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setState('copied');
    } catch {
      setState('error');
    }

    if (timeout.current) window.clearTimeout(timeout.current);
    timeout.current = window.setTimeout(() => setState('idle'), 1800);
  };

  const label = state === 'copied' ? 'copied' : state === 'error' ? 'failed' : 'copy';
  const accessibleLabel =
    state === 'copied'
      ? `Copied command: ${value}`
      : state === 'error'
        ? `Copy failed for command: ${value}`
        : `Copy command: ${value}`;

  return (
    <button
      type="button"
      className={`copy-button${state !== 'idle' ? ` is-${state}` : ''}`}
      onClick={copy}
      aria-label={accessibleLabel}
    >
      <span aria-live="polite">{label}</span>
    </button>
  );
}
