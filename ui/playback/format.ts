export function formatTime(seconds: number): string {
  const value = Number.isFinite(seconds) ? Math.floor(Math.max(0, seconds)) : 0;
  return `${String(Math.floor(value / 60)).padStart(2, '0')}:${String(value % 60).padStart(2, '0')}`;
}
