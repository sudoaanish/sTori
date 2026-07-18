import { ArrowLeft, Bookmark, ChevronLeft, ChevronRight, Moon, Settings2, Sun, Trash2 } from 'lucide-react';
import * as pdfjs from 'pdfjs-dist';
import { CSSProperties, useCallback, useEffect, useRef, useState } from 'react';
import { useLocation, useNavigate, useParams } from 'react-router-dom';
import ePub, { Book as EpubBook, Rendition } from 'epubjs';
import { api } from '../lib/api';
import {
  fontFamily,
  fontOptions,
  isReaderFontId,
  readerFontFaceCss,
  READER_FONT_STORAGE_KEY,
  ReaderFontId,
} from '../lib/typography';
import type { Annotation, Book } from '../types';

pdfjs.GlobalWorkerOptions.workerSrc = new URL('pdfjs-dist/build/pdf.worker.min.mjs', import.meta.url).toString();

type Theme = 'paper' | 'night' | 'sepia' | 'white';
type Alignment = 'left' | 'justify';
type ParagraphStyle = 'indent' | 'spacing';

interface ReaderAppearance {
  theme: Theme;
  font: ReaderFontId;
  fontSize: number;
  lineHeight: number;
  pageMargin: number;
  alignment: Alignment;
  paragraphs: ParagraphStyle;
  hyphenation: boolean;
}

interface ReaderNavigationState {
  fromBook?: boolean;
  bookOrigin?: string;
}

function storedReaderFont(): ReaderFontId {
  const stored = localStorage.getItem(READER_FONT_STORAGE_KEY);
  return isReaderFontId(stored) ? stored : 'merriweather';
}

export function ReaderPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const location = useLocation();
  const navigationState = location.state as ReaderNavigationState | null;
  const [book, setBook] = useState<Book>();
  const [controls, setControls] = useState(true);
  const [settings, setSettings] = useState(false);
  const [progress, setProgress] = useState(0);
  const [locator, setLocator] = useState('');
  const [bookmarks, setBookmarks] = useState<Annotation[]>([]);
  const [bookmarkPanel, setBookmarkPanel] = useState(false);
  const [bookmarkError, setBookmarkError] = useState('');
  const [bookmarkBusy, setBookmarkBusy] = useState(false);
  const [jumpToBookmark, setJumpToBookmark] = useState<string>();
  const [appearance, setAppearance] = useState<ReaderAppearance>(() => ({
    theme: (localStorage.getItem('stori_theme') as Theme) || 'paper',
    font: storedReaderFont(),
    fontSize: Number(localStorage.getItem('stori_font_size') || 100),
    lineHeight: Number(localStorage.getItem('stori_line_height') || 1.5),
    pageMargin: Number(localStorage.getItem('stori_page_margin') || 28),
    alignment: localStorage.getItem('stori_text_alignment') === 'justify' ? 'justify' : 'left',
    paragraphs: localStorage.getItem('stori_paragraph_style') === 'spacing' ? 'spacing' : 'indent',
    hyphenation: localStorage.getItem('stori_hyphenation') !== 'false',
  }));

  useEffect(() => { if (id) api.book(Number(id)).then(setBook); }, [id]);
  useEffect(() => { if (id) api.bookmarks(Number(id)).then(setBookmarks).catch((error) => setBookmarkError(error.message)); }, [id]);

  useEffect(() => {
    localStorage.setItem('stori_theme', appearance.theme);
    localStorage.setItem(READER_FONT_STORAGE_KEY, appearance.font);
    localStorage.setItem('stori_font_size', String(appearance.fontSize));
    localStorage.setItem('stori_line_height', String(appearance.lineHeight));
    localStorage.setItem('stori_page_margin', String(appearance.pageMargin));
    localStorage.setItem('stori_text_alignment', appearance.alignment);
    localStorage.setItem('stori_paragraph_style', appearance.paragraphs);
    localStorage.setItem('stori_hyphenation', String(appearance.hyphenation));
  }, [appearance]);

  const changeAppearance = <K extends keyof ReaderAppearance>(key: K, value: ReaderAppearance[K]) => {
    setAppearance((current) => ({ ...current, [key]: value }));
  };

  const leaveReader = () => {
    if (navigationState?.fromBook) {
      navigate(-1);
      return;
    }
    if (book) navigate(`/books/${book.id}`, { replace: true, state: { from: navigationState?.bookOrigin || '/' } });
  };
  const selectedBookmark = bookmarks.find((bookmark) => bookmark.locator === locator);
  const toggleBookmark = async () => {
    if (!book || !locator || bookmarkBusy) { if (!locator) setBookmarkError('The reader is still finding your current location.'); return; }
    setBookmarkBusy(true); setBookmarkError('');
    try {
      if (selectedBookmark) {
        await api.deleteBookmark(book.id, selectedBookmark.id);
        setBookmarks((current) => current.filter((bookmark) => bookmark.id !== selectedBookmark.id));
      } else {
        const saved = await api.addBookmark(book.id, { locator, text: `${Math.round(progress * 100)}%` });
        setBookmarks((current) => [saved, ...current.filter((bookmark) => bookmark.id !== saved.id)]);
      }
    } catch (error) { setBookmarkError(error instanceof Error ? error.message : 'Could not update bookmark.'); }
    finally { setBookmarkBusy(false); }
  };

  if (!book) return <div className="reader loading">Opening book…</div>;

  return (
    <div className={`reader theme-${appearance.theme}`} onClick={() => setControls((visible) => !visible)}>
      {controls && <header className="reader-top" onClick={(event) => event.stopPropagation()}><button aria-label="Back to book" onClick={leaveReader}><ArrowLeft/></button><div><strong>{book.title}</strong><span>{book.authors.join(', ')}</span></div><button aria-label={selectedBookmark ? 'Remove bookmark' : 'Add bookmark'} aria-pressed={Boolean(selectedBookmark)} className={selectedBookmark ? 'bookmark-selected' : ''} disabled={bookmarkBusy} onClick={toggleBookmark}><Bookmark fill={selectedBookmark ? 'currentColor' : 'none'}/></button><button aria-label="Show bookmarks" onClick={() => setBookmarkPanel((visible) => !visible)}>Bookmarks ({bookmarks.length})</button><button aria-label="Reading appearance" onClick={() => setSettings((visible) => !visible)}><Settings2/></button></header>}
      {bookmarkError && <p className="reader-bookmark-error" role="alert">{bookmarkError}</p>}
      {bookmarkPanel && <aside className="reader-bookmarks" onClick={(event) => event.stopPropagation()}><h3>Bookmarks</h3>{bookmarks.length ? <ul>{bookmarks.map((bookmark) => <li key={bookmark.id}><button onClick={() => { setJumpToBookmark(bookmark.locator); setBookmarkPanel(false); }}>Go to {bookmark.text || 'saved location'}</button><button aria-label="Delete bookmark" onClick={() => api.deleteBookmark(book.id, bookmark.id).then(() => setBookmarks((current) => current.filter((item) => item.id !== bookmark.id))).catch((error) => setBookmarkError(error.message))}><Trash2/></button></li>)}</ul> : <p>No bookmarks in this book.</p>}</aside>}

      {settings && (
        <div className="reader-settings" onClick={(event) => event.stopPropagation()}>
          <h3>Reading appearance</h3>
          <div className="theme-options">{(['paper', 'night', 'sepia', 'white'] as Theme[]).map((name) => <button className={appearance.theme === name ? 'active' : ''} onClick={() => changeAppearance('theme', name)} key={name}>{name === 'night' ? <Moon/> : <Sun/>}{name}</button>)}</div>
          {book.format === 'epub' && <>
            <label className="reader-select-label">Typeface<select value={appearance.font} onChange={(event) => changeAppearance('font', event.target.value as ReaderFontId)}><option value="publisher">Publisher font</option>{fontOptions.map((font) => <option key={font.id} value={font.id}>{font.name}</option>)}</select></label>
            <label>Text size<input type="range" min="75" max="160" value={appearance.fontSize} onChange={(event) => changeAppearance('fontSize', Number(event.target.value))}/><span>{appearance.fontSize}%</span></label>
            <label>Line height<input type="range" min="1.25" max="1.9" step="0.05" value={appearance.lineHeight} onChange={(event) => changeAppearance('lineHeight', Number(event.target.value))}/><span>{appearance.lineHeight.toFixed(2)}</span></label>
            <label>Page margins<input type="range" min="16" max="64" step="4" value={appearance.pageMargin} onChange={(event) => changeAppearance('pageMargin', Number(event.target.value))}/><span>{appearance.pageMargin}px</span></label>
            <div className="reader-segmented" role="group" aria-label="Text alignment"><button className={appearance.alignment === 'left' ? 'active' : ''} onClick={() => changeAppearance('alignment', 'left')}>Left aligned</button><button className={appearance.alignment === 'justify' ? 'active' : ''} onClick={() => changeAppearance('alignment', 'justify')}>Justified</button></div>
            <div className="reader-segmented" role="group" aria-label="Paragraph layout"><button className={appearance.paragraphs === 'indent' ? 'active' : ''} onClick={() => changeAppearance('paragraphs', 'indent')}>Indented</button><button className={appearance.paragraphs === 'spacing' ? 'active' : ''} onClick={() => changeAppearance('paragraphs', 'spacing')}>Spaced</button></div>
            <label className="reader-toggle"><span>Hyphenation</span><input type="checkbox" checked={appearance.hyphenation} onChange={(event) => changeAppearance('hyphenation', event.target.checked)}/></label>
          </>}
        </div>
      )}

      <div className="reader-stage" onClick={(event) => event.stopPropagation()}>{book.format === 'epub' ? <EpubReader book={book} appearance={appearance} onProgress={setProgress} onLocation={setLocator} jumpTo={jumpToBookmark}/> : book.format === 'pdf' ? <PdfReader book={book} onProgress={setProgress} onLocation={setLocator} jumpTo={jumpToBookmark}/> : <div className="empty-state"><h2>MOBI reading is not available yet</h2><p>This book is indexed and ready for a future converter/reader.</p></div>}</div>
      {controls && <footer className="reader-bottom"><span>{Math.round(progress * 100)}%</span><div><span style={{width: `${progress * 100}%`}}/></div></footer>}
    </div>
  );
}

function EpubReader({ book, appearance, onProgress, onLocation, jumpTo }: { book: Book; appearance: ReaderAppearance; onProgress: (progress: number) => void; onLocation: (locator: string) => void; jumpTo?: string }) {
  const host = useRef<HTMLDivElement>(null);
  const epubRef = useRef<EpubBook | undefined>(undefined);
  const renditionRef = useRef<Rendition | undefined>(undefined);
  const appearanceRef = useRef(appearance);
  const [ready, setReady] = useState(false);

  const applyAppearance = useCallback((document: Document, selected: ReaderAppearance) => {
    let style = document.getElementById('stori-reader-appearance') as HTMLStyleElement | null;
    if (!style) {
      style = document.createElement('style');
      style.id = 'stori-reader-appearance';
      document.head.appendChild(style);
    }
    const colors = selected.theme === 'night' ? { bg: '#10111d', fg: '#e9e7e1' } : selected.theme === 'sepia' ? { bg: '#e9ddc4', fg: '#352f26' } : selected.theme === 'white' ? { bg: '#fff', fg: '#18181d' } : { bg: '#f7f3eb', fg: '#22212a' };
    const selectedFamily = fontFamily(selected.font);
    const paragraphRules = selected.paragraphs === 'indent' ? 'margin-top: 0 !important; margin-bottom: 0 !important; text-indent: 1.25em !important;' : 'margin-top: 0 !important; margin-bottom: .85em !important; text-indent: 0 !important;';
    style.textContent = `${readerFontFaceCss()}
      html, body { background: ${colors.bg} !important; color: ${colors.fg} !important; }
      body { ${selectedFamily ? `font-family: ${selectedFamily} !important;` : ''} font-size: ${selected.fontSize}% !important; line-height: ${selected.lineHeight} !important; text-align: ${selected.alignment} !important; -webkit-hyphens: ${selected.hyphenation ? 'auto' : 'none'} !important; hyphens: ${selected.hyphenation ? 'auto' : 'none'} !important; font-kerning: normal; font-variant-ligatures: common-ligatures; }
      p { ${paragraphRules} }
      a { color: #49307b !important; }
    `;
  }, []);

  useEffect(() => { appearanceRef.current = appearance; }, [appearance]);

  useEffect(() => {
    let disposed = false;
    fetch(api.bookFileUrl(book.id), { headers: localStorage.getItem('stori_reader_token') ? { Authorization: `Bearer ${localStorage.getItem('stori_reader_token')}` } : {} }).then((response) => response.arrayBuffer()).then(async (data) => {
      if (disposed || !host.current) return;
      const epub = ePub(data);
      epubRef.current = epub;
      const rendition = epub.renderTo(host.current, { width: '100%', height: '100%', flow: 'paginated', spread: 'auto' });
      renditionRef.current = rendition;
      rendition.hooks.content.register((contents: { document: Document }) => applyAppearance(contents.document, appearanceRef.current));
      await epub.ready;
      await epub.locations.generate(1200);
      const saved = await api.readingState(book.id).catch(() => null);
      await rendition.display(saved?.locator || undefined);
      rendition.on('relocated', (relocation: { start: { cfi: string; percentage?: number } }) => {
        const percentage = relocation.start.percentage ?? epub.locations.percentageFromCfi(relocation.start.cfi);
        onProgress(percentage);
        onLocation(relocation.start.cfi);
        api.saveProgress(book.id, relocation.start.cfi, percentage).catch(() => undefined);
      });
      setReady(true);
    });
    return () => { disposed = true; renditionRef.current?.destroy(); epubRef.current?.destroy(); };
  }, [applyAppearance, book.id, onProgress, onLocation]);
  useEffect(() => { if (jumpTo) renditionRef.current?.display(jumpTo); }, [jumpTo]);

  useEffect(() => {
    const current = renditionRef.current?.getContents() as unknown;
    const contents = (Array.isArray(current) ? current : current ? [current] : []) as Array<{ document: Document }>;
    contents.forEach((content) => applyAppearance(content.document, appearance));
  }, [appearance, applyAppearance, ready]);

  const hostStyle = { '--reader-page-gutter': `${appearance.pageMargin * 2}px` } as CSSProperties;
  return <div className="epub-reader"><button className="page-turn prev" aria-label="Previous page" onClick={() => renditionRef.current?.prev()}><ChevronLeft/></button><div ref={host} className="epub-host" style={hostStyle}/><button className="page-turn next" aria-label="Next page" onClick={() => renditionRef.current?.next()}><ChevronRight/></button>{!ready && <div className="reader-loading">Preparing pages…</div>}</div>;
}

function PdfReader({ book, onProgress, onLocation, jumpTo }: { book: Book; onProgress: (progress: number) => void; onLocation: (locator: string) => void; jumpTo?: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [doc, setDoc] = useState<pdfjs.PDFDocumentProxy>();
  const [page, setPage] = useState(1);
  useEffect(() => { api.readingState(book.id).then((saved) => { if (saved?.locator) setPage(Number(saved.locator) || 1); }); pdfjs.getDocument(api.bookFileUrl(book.id)).promise.then(setDoc); }, [book.id]);
  useEffect(() => {
    if (!doc || !canvas.current) return;
    let cancelled = false;
    doc.getPage(page).then((pdfPage) => {
      if (cancelled || !canvas.current) return;
      const base = pdfPage.getViewport({ scale: 1 });
      const maxWidth = Math.min(window.innerWidth - 32, 900);
      const viewport = pdfPage.getViewport({ scale: maxWidth / base.width });
      const target = canvas.current;
      const context = target.getContext('2d')!;
      target.width = viewport.width;
      target.height = viewport.height;
      pdfPage.render({ canvas: target, canvasContext: context, viewport }).promise;
      const percentage = (page - 1) / Math.max(doc.numPages - 1, 1);
      onProgress(percentage);
      onLocation(String(page));
      api.saveProgress(book.id, String(page), percentage).catch(() => undefined);
    });
    return () => { cancelled = true; };
  }, [book.id, doc, page, onProgress, onLocation]);
  useEffect(() => { if (jumpTo) setPage(Number(jumpTo) || 1); }, [jumpTo]);
  return <div className="pdf-reader"><button className="page-turn prev" disabled={page <= 1} onClick={() => setPage((current) => Math.max(1, current - 1))}><ChevronLeft/></button><canvas ref={canvas}/><button className="page-turn next" disabled={!doc || page >= doc.numPages} onClick={() => setPage((current) => Math.min(doc?.numPages || current, current + 1))}><ChevronRight/></button><span className="pdf-page">Page {page} of {doc?.numPages || '…'}</span></div>;
}
