/** Split a command line into arguments, honoring single/double quotes. */
export function shellSplit(input: string): string[] {
  const out: string[] = [];
  let cur = '';
  let quote: string | null = null;
  let has = false;
  for (const ch of input.trim()) {
    if (quote) {
      if (ch === quote) quote = null;
      else cur += ch;
    } else if (ch === '"' || ch === "'") {
      quote = ch;
      has = true;
    } else if (/\s/.test(ch)) {
      if (cur || has) {
        out.push(cur);
        cur = '';
        has = false;
      }
    } else {
      cur += ch;
    }
  }
  if (cur || has) out.push(cur);
  return out;
}

export function formatUptime(startedAt: number | null): string {
  if (!startedAt) return '-';
  let s = Math.max(0, Math.floor((Date.now() - startedAt) / 1000));
  const d = Math.floor(s / 86400);
  s %= 86400;
  const h = Math.floor(s / 3600);
  s %= 3600;
  const m = Math.floor(s / 60);
  s %= 60;
  if (d > 0) return `${d}天${h}时${m}分`;
  if (h > 0) return `${h}时${m}分${s}秒`;
  if (m > 0) return `${m}分${s}秒`;
  return `${s}秒`;
}

export function formatTime(ts: number): string {
  const d = new Date(ts);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

export function displayCommand(command: string, args: string[]): string {
  return [command, ...args].filter(Boolean).join(' ');
}
