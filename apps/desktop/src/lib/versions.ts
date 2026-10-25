/** Vergleicht Versionen segmentweise numerisch: negativ, wenn `a` älter ist als `b`. */
export function version_cmp(a: string, b: string): number {
  const key = (s: string) => s.split(/[^0-9]+/).filter(Boolean).map(Number);
  const [x, y] = [key(a), key(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}
