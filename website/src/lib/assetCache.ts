const cache = new Map<string, string>();

export function setAssetBlob(url: string, blobUrl: string): void {
  cache.set(url, blobUrl);
}

export function getAssetBlob(url: string): string {
  return cache.get(url) || url;
}

