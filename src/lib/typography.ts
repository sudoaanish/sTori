import literataItalicUrl from '@fontsource-variable/literata/files/literata-latin-wght-italic.woff2?url';
import literataRomanUrl from '@fontsource-variable/literata/files/literata-latin-wght-normal.woff2?url';
import merriweatherItalicUrl from '@fontsource-variable/merriweather/files/merriweather-latin-wght-italic.woff2?url';
import merriweatherRomanUrl from '@fontsource-variable/merriweather/files/merriweather-latin-wght-normal.woff2?url';
import newsreaderItalicUrl from '@fontsource-variable/newsreader/files/newsreader-latin-wght-italic.woff2?url';
import newsreaderRomanUrl from '@fontsource-variable/newsreader/files/newsreader-latin-wght-normal.woff2?url';
import sourceSerifItalicUrl from '@fontsource-variable/source-serif-4/files/source-serif-4-latin-wght-italic.woff2?url';
import sourceSerifRomanUrl from '@fontsource-variable/source-serif-4/files/source-serif-4-latin-wght-normal.woff2?url';

export type AppFontId = 'merriweather' | 'literata' | 'source-serif' | 'newsreader';
export type ReaderFontId = 'publisher' | AppFontId;

export const APP_FONT_STORAGE_KEY = 'stori_app_font';
export const READER_FONT_STORAGE_KEY = 'stori_reader_font';

export const fontOptions: Array<{ id: AppFontId; name: string; description: string; family: string }> = [
  { id: 'merriweather', name: 'Merriweather', description: 'Warm and sturdy', family: '"Merriweather Variable", serif' },
  { id: 'literata', name: 'Literata', description: 'Made for digital books', family: '"Literata Variable", serif' },
  { id: 'source-serif', name: 'Source Serif 4', description: 'Clean and classical', family: '"Source Serif 4 Variable", serif' },
  { id: 'newsreader', name: 'Newsreader', description: 'Editorial and refined', family: '"Newsreader Variable", serif' },
];

export function isAppFontId(value: string | null): value is AppFontId {
  return fontOptions.some((option) => option.id === value);
}

export function isReaderFontId(value: string | null): value is ReaderFontId {
  return value === 'publisher' || isAppFontId(value);
}

export function fontFamily(id: ReaderFontId): string | undefined {
  return fontOptions.find((option) => option.id === id)?.family;
}

const fontAssets: Record<AppFontId, { family: string; roman: string; italic: string; minWeight: number; maxWeight: number }> = {
  merriweather: { family: 'Merriweather Variable', roman: merriweatherRomanUrl, italic: merriweatherItalicUrl, minWeight: 300, maxWeight: 900 },
  literata: { family: 'Literata Variable', roman: literataRomanUrl, italic: literataItalicUrl, minWeight: 200, maxWeight: 900 },
  'source-serif': { family: 'Source Serif 4 Variable', roman: sourceSerifRomanUrl, italic: sourceSerifItalicUrl, minWeight: 200, maxWeight: 900 },
  newsreader: { family: 'Newsreader Variable', roman: newsreaderRomanUrl, italic: newsreaderItalicUrl, minWeight: 200, maxWeight: 800 },
};

export function readerFontFaceCss(): string {
  return Object.values(fontAssets).map((font) => {
    const roman = new URL(font.roman, document.baseURI).href;
    const italic = new URL(font.italic, document.baseURI).href;
    return `
      @font-face { font-family: "${font.family}"; src: url("${roman}") format("woff2-variations"); font-style: normal; font-weight: ${font.minWeight} ${font.maxWeight}; font-display: swap; }
      @font-face { font-family: "${font.family}"; src: url("${italic}") format("woff2-variations"); font-style: italic; font-weight: ${font.minWeight} ${font.maxWeight}; font-display: swap; }
    `;
  }).join('\n');
}
