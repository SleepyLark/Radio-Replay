export function tuningMilliseconds(
  settings: Record<string, unknown>,
  random = Math.random,
): number {
  if (settings.tuningTransition === false) return 0;
  const bound = (value: unknown, fallback: number) =>
    typeof value === 'number' && Number.isFinite(value)
      ? Math.round(Math.max(0, Math.min(1500, value)))
      : fallback;
  const min = bound(settings.tuningMinMs, 0),
    max = bound(settings.tuningMaxMs, 1500);
  return Math.min(min, max) + Math.floor(random() * (Math.abs(max - min) + 1));
}
