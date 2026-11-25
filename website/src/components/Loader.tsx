import { useEffect, useRef, useState } from 'react';
import { ASCII_LOGO } from '../content';
import { setAssetBlob } from '../lib/assetCache';

interface LoaderProps {
  onComplete: () => void;
}

const ASSETS = [
  { url: '/loader.webp', size: 40230, isImage: true },
  { url: '/logo.webp', size: 34912, isImage: true },
  { url: '/sponsor1.webp', size: 69768, isImage: true },
  { url: '/screenshots/1.webp', size: 182640, isImage: true },
  { url: '/screenshots/2.webp', size: 229510, isImage: true },
  { url: '/screenshots/3.webp', size: 54270, isImage: true },
  { url: '/recordings/1.webm', size: 2036132, isImage: false },
];

const FONT_WEIGHT = 100000;
const TOTAL_BYTES = ASSETS.reduce((sum, a) => sum + a.size, 0) + FONT_WEIGHT;

export default function Loader({ onComplete }: LoaderProps) {
  const [progress, setProgress] = useState(0);
  const [isFading, setIsFading] = useState(false);
  const [isDestroyed, setIsDestroyed] = useState(false);

  const rawProgressRef = useRef(0);
  const displayProgressRef = useRef(0);
  const loadedBytesRef = useRef(0);
  const isLoadedRef = useRef(false);

  useEffect(() => {
    let animId: number;
    let finishTimeout: number | undefined;
    const startTime = performance.now();
    const minDuration = 1500;

    const tick = () => {
      const elapsed = performance.now() - startTime;
      const timeRatio = Math.min(1, elapsed / minDuration);

      let target: number;
      if (isLoadedRef.current) {
        target = timeRatio * 100;
      } else {
        target = Math.min(
          timeRatio * 100,
          Math.max(timeRatio * 35, rawProgressRef.current),
          95
        );
      }

      const current = displayProgressRef.current;
      const delta = target - current;
      const step = Math.max(delta * 0.24, 0.25);
      const next = Math.min(100, current + step);

      if (isLoadedRef.current && elapsed >= minDuration && next >= 99.5) {
        displayProgressRef.current = 100;
        setProgress(100);
        finishTimeout = window.setTimeout(() => {
          setIsFading(true);
          finishTimeout = window.setTimeout(() => {
            setIsDestroyed(true);
            onComplete();
          }, 500);
        }, 250);
        return;
      }

      displayProgressRef.current = next;
      setProgress(next);
      animId = requestAnimationFrame(tick);
    };

    animId = requestAnimationFrame(tick);

    const loadSingleAsset = async (item: (typeof ASSETS)[number]) => {
      try {
        const response = await fetch(item.url);
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const contentLength =
          +(response.headers.get('content-length') || 0) || item.size;

        let blob: Blob;
        if (!response.body) {
          blob = await response.blob();
          loadedBytesRef.current += contentLength;
          rawProgressRef.current = Math.min(
            100,
            (loadedBytesRef.current / TOTAL_BYTES) * 100
          );
        } else {
          const reader = response.body.getReader();
          const chunks: Uint8Array[] = [];
          while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            if (value) {
              chunks.push(value);
              loadedBytesRef.current += value.length;
              rawProgressRef.current = Math.min(
                100,
                (loadedBytesRef.current / TOTAL_BYTES) * 100
              );
            }
          }
          blob = new Blob(chunks as BlobPart[]);
        }

        const blobUrl = URL.createObjectURL(blob);
        setAssetBlob(item.url, blobUrl);

        if (item.isImage) {
          const img = new Image();
          img.src = blobUrl;
          await img.decode().catch(() => {});
        }
      } catch {
        loadedBytesRef.current += item.size;
        rawProgressRef.current = Math.min(
          100,
          (loadedBytesRef.current / TOTAL_BYTES) * 100
        );
      }
    };

    const runLoader = async () => {
      const assetPromises = ASSETS.map(loadSingleAsset);
      const fontPromise = (
        document.fonts ? document.fonts.ready : Promise.resolve()
      ).then(() => {
        loadedBytesRef.current += FONT_WEIGHT;
        rawProgressRef.current = Math.min(
          100,
          (loadedBytesRef.current / TOTAL_BYTES) * 100
        );
      });

      await Promise.all([...assetPromises, fontPromise]);
      rawProgressRef.current = 100;
      isLoadedRef.current = true;
    };

    runLoader();

    return () => {
      cancelAnimationFrame(animId);
      if (finishTimeout) clearTimeout(finishTimeout);
    };
  }, [onComplete]);

  if (isDestroyed) return null;

  const clipInset = Math.max(0, 100 - progress);

  return (
    <aside
      className={`site-loader${isFading ? ' is-hidden' : ''}`}
      role="progressbar"
      aria-label="Loading ANSImuz"
      aria-valuenow={Math.round(progress)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div className="loader-inner">
        <img
          src="/loader.webp"
          alt=""
          className="loader-spinner"
          width="84"
          height="84"
        />
        <div className="loader-ascii-wrap" aria-hidden="true">
          <pre className="loader-ascii-base">{ASCII_LOGO}</pre>
          <pre
            className="loader-ascii-fill"
            style={{ clipPath: `inset(0 ${clipInset}% 0 0)` }}
          >
            {ASCII_LOGO}
          </pre>
        </div>
      </div>
    </aside>
  );
}

