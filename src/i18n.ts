import catalog from '../config/locales/ui.json';

export const languages = [
  ['fr', 'Français'], ['en', 'English'], ['es', 'Español'], ['it', 'Italiano'],
  ['de', 'Deutsch'], ['zh', '中文（简体）'], ['ja', '日本語'],
] as const;
export type Language = typeof languages[number][0];
let current: Language = 'fr';
const translations: Record<string, string[]> = catalog;
export function language(): Language { return current; }
export function setLanguage(value: string): void {
  current = languages.some(([code]) => code === value) ? value as Language : 'fr';
  document.documentElement.lang = current;
  document.title = t('Sherka — Compagnon de bureau');
}
export function formatLocale(): string { return current === 'zh' ? 'zh-CN' : current; }
export function languageOptions(value: string): string {
  return languages.map(([code, label]) => `<option value="${code}" ${code === value ? 'selected' : ''}>${label}</option>`).join('');
}
const escapeText = (value: string) => value.replace(/[&<>"']/g, character => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[character]!));

/** Only authored UI strings enter this function. Never pass a user's dialogue,
 * personality, memory, model identifier or voice name for translation. */
export function t(source: string): string;
export function t(source: TemplateStringsArray, ...values: unknown[]): string;
export function t(source: string | TemplateStringsArray, ...values: unknown[]): string {
  const key = typeof source === 'string' ? source : source.reduce((result, part, index) => result + (index ? `{${index - 1}}` : '') + part, '');
  const trimmed = key.trim();
  if (!trimmed) return key;
  const leading = key.match(/^\s*/)?.[0] ?? '';
  const trailing = key.match(/\s*$/)?.[0] ?? '';
  const index = languages.findIndex(([code]) => code === current) - 1;
  const translated = leading + (index < 0 ? trimmed : translations[trimmed]?.[index] ?? trimmed) + trailing;
  return values.length ? translated.replace(/\{(\d+)\}/g, (match, position: string) => Number(position) < values.length ? String(values[Number(position)]) : match) : translated;
}

/** Translate static template text before interpolating values. Consequently
 * user content and HTML supplied by slots are never inspected or rewritten. */
export function html(strings: TemplateStringsArray, ...values: unknown[]): string {
  let markup = strings.reduce((result, part, index) => result + (index ? `{${index - 1}}` : '') + part, '');
  const translate = (source: string) => {
    const start = source.match(/^\s*/)?.[0] ?? '';
    const end = source.match(/\s*$/)?.[0] ?? '';
    const text = source.trim();
    if (!text || !/[^\d{}\s]/.test(text)) return source;
    const positions: string[] = [];
    const key = text.replace(/\{(\d+)\}/g, (_, position: string) => { positions.push(position); return `{${positions.length - 1}}`; });
    // Incomplete template fragments also contain attribute delimiters. Only
    // registered authored text is translated and escaped; syntax stays intact.
    if (!translations[key]) return source;
    const translated = t(key).replace(/\{(\d+)\}/g, (match, position: string) => positions[Number(position)] === undefined ? match : `{${positions[Number(position)]}}`);
    return start + escapeText(translated) + end;
  };
  if (markup.includes('<')) markup = markup.replace(/(^|>)([^<>]*)(?=<|$)/g, (_, boundary: string, source: string) => boundary + translate(source));
  markup = markup.replace(/\b(aria-label|title|alt|placeholder)="([^"<>]*)"/g, (_, attribute: string, source: string) => `${attribute}="${translate(source)}"`);
  return markup.replace(/\{(\d+)\}/g, (match, position: string) => Number(position) < values.length ? String(values[Number(position)]) : match);
}

/** Backend feedback is an authored template plus opaque parameters. Pattern
 * matches are anchored; unmatched provider details remain unchanged. */
export function feedback(source: string | null | undefined): string {
  if (!source) return '';
  if (current === 'fr') return source;
  if (translations[source]) return t(source);
  for (const key of Object.keys(translations)) {
    if (!key.includes('{')) continue;
    const positions: number[] = [];
    const pattern = key.split(/(\{\d+\})/g).map(part => {
      const slot = part.match(/^\{(\d+)\}$/);
      if (slot) { positions.push(Number(slot[1])); return '([\\s\\S]*?)'; }
      return part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    }).join('');
    const match = source.match(new RegExp('^' + pattern + '$'));
    if (match) {
      const values: string[] = [];
      positions.forEach((position, index) => { values[position] = match[index + 1]!; });
      if (key === 'Souvenir #{0} {1} : {2}') values[1] = t(values[1]!);
      if (key === '{0} a refusé la demande (HTTP {1}). {2}') values[2] = t(values[2]!);
      return t(key).replace(/\{(\d+)\}/g, (original, position: string) => values[Number(position)] ?? original);
    }
  }
  return source;
}
