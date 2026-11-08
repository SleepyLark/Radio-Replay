export const isColor = (value: unknown): value is string =>
  typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value);
export function stationAccent(
  station: Record<string, unknown> | undefined,
  settings: Record<string, unknown>,
  sampled = '',
) {
  if (isColor(station?.accent)) return station.accent;
  const fallback = isColor(settings.accent) ? settings.accent : '#c7f464';
  if (!station || settings.matchBannerAccent === false) return fallback;
  const color = station.bannerPath ? station.bannerAccent || sampled : station.bannerColor;
  return isColor(color) ? color : fallback;
}

// Matches Electron's small, saturation-weighted banner sample. Never decodes audio.
export async function sampleBanner(url: string): Promise<string> {
  const image = new Image();
  image.crossOrigin = 'anonymous';
  image.src = url;
  await image.decode();
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 16;
  const context = canvas.getContext('2d');
  if (!context) return '';
  context.drawImage(image, 0, 0, 16, 16);
  const pixels = context.getImageData(0, 0, 16, 16).data;
  let red = 0,
    green = 0,
    blue = 0,
    weight = 0;
  for (let i = 0; i < pixels.length; i += 4) {
    const alpha = pixels[i + 3] / 255;
    if (alpha < 0.25) continue;
    const [r, g, b] = pixels.slice(i, i + 3);
    const emphasis = alpha * (1 + (Math.max(r, g, b) - Math.min(r, g, b)) / 255);
    red += r * emphasis;
    green += g * emphasis;
    blue += b * emphasis;
    weight += emphasis;
  }
  if (!weight) return '#c7f464';
  const channel = (value: number) =>
    Math.max(45, Math.min(230, Math.round(value / weight)))
      .toString(16)
      .padStart(2, '0');
  return `#${channel(red)}${channel(green)}${channel(blue)}`;
}
