export function heatmapLevels(values: number[]): number[] {
  const normalized = values.map((value) => Math.max(0, Number(value) || 0));
  const nonzero = normalized.filter((value) => value > 0).sort((a, b) => a - b);
  if (!nonzero.length) {
    return normalized.map(() => 0);
  }
  if (nonzero[0] === nonzero[nonzero.length - 1]) {
    return normalized.map((value) => (value > 0 ? 4 : 0));
  }
  const quantile = (fraction: number) =>
    nonzero[Math.max(0, Math.ceil(nonzero.length * fraction) - 1)] ?? 0;
  const q25 = quantile(0.25);
  const q50 = quantile(0.5);
  const q75 = quantile(0.75);
  return normalized.map((value) => {
    if (value <= 0) {
      return 0;
    }
    if (value <= q25) {
      return 1;
    }
    if (value <= q50) {
      return 2;
    }
    if (value <= q75) {
      return 3;
    }
    return 4;
  });
}
