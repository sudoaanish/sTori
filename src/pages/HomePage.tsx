import { ArrowRight, BookOpenText, Download, LoaderCircle } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { BookCard } from '../components/BookCard';
import { api } from '../lib/api';
import { bookMatchesGenre, HOME_GENRES } from '../lib/genres';
import type { Book, Collection, DownloadJob, Series } from '../types';

export function HomePage() {
  const [books, setBooks] = useState<Book[]>([]);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [series, setSeries] = useState<Series[]>([]);
  const [starterJobs, setStarterJobs] = useState<DownloadJob[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([api.books(), api.collections(), api.series()]).then(([bookRows, collectionRows, seriesRows]) => {
      setBooks(bookRows);
      setCollections(collectionRows);
      setSeries(seriesRows);
      setLoading(false);
    }).catch(() => setLoading(false));
  }, []);

  useEffect(() => {
    const desktop = '__TAURI_INTERNALS__' in window || ['localhost', '127.0.0.1'].includes(window.location.hostname);
    if (!desktop) return;
    const loadStarter = () => Promise.all([api.downloadJobs(), api.books()]).then(([jobs, rows]) => {
      setStarterJobs(jobs.filter((job) => job.provider === 'Standard Ebooks'));
      setBooks(rows);
    }).catch(() => undefined);
    loadStarter();
    const timer = window.setInterval(loadStarter, 1400);
    return () => clearInterval(timer);
  }, []);

  const continuing = books.filter((book) => book.progress > 0 && book.progress < .995).slice(0, 6);
  const recent = [...books].sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0, 10);
  const genreRows = HOME_GENRES.map((genre) => ({ genre, books: books.filter((book) => bookMatchesGenre(book, genre)) })).filter((row) => row.books.length > 0);
  const starterActive = starterJobs.some((job) => !['completed', 'cancelled'].includes(job.status));

  return <div className="page home-page">
    <header className="page-header hero"><div><span className="eyebrow">Your personal reading room</span><h1>Good evening.</h1><p>{books.length ? `${books.length} books are waiting on your shelves.` : 'Your reading room is getting ready.'}</p></div><Link className="search-pill" to="/search">Search title, author, series…</Link></header>
    {starterActive && <StarterShelf jobs={starterJobs}/>}
    {loading ? <div className="empty-state">Opening your shelves…</div> : !books.length ? <div className="empty-state"><BookOpenText size={42}/><h2>Your starter shelf is on its way</h2><p>sTori is preparing two beautifully produced classics in the background.</p><Link className="primary-button desktop-only-inline" to="/downloads">View downloads</Link></div> : <>
      {continuing.length > 0 && <Row title="Continue reading" link="/library?status=reading"><div className="book-grid row-grid">{continuing.map((book) => <BookCard key={book.id} book={book}/>)}</div></Row>}
      {(collections.length > 0 || series.length > 0) && <Row title="Collections & Series" link="/collections"><div className="collection-grid row-grid">{series.slice(0, 4).map((item) => <FeatureCard key={`s${item.id}`} title={item.name} count={item.books.length} type="Series" books={item.books} to={`/collections/series/${item.id}`}/>)}{collections.slice(0, 4).map((item) => <FeatureCard key={`c${item.id}`} title={item.name} count={item.books.length} type="Collection" books={item.books} to={`/collections/collection/${item.id}`}/>)}</div></Row>}
      {genreRows.map(({ genre, books: genreBooks }) => <Row key={genre.slug} title={genre.name} link={`/library?genre=${genre.slug}`}><div className="book-grid row-grid">{genreBooks.slice(0, 10).map((book) => <BookCard key={book.id} book={book}/>)}</div></Row>)}
      <Row title="Recently added" link="/library"><div className="book-grid row-grid">{recent.map((book) => <BookCard key={book.id} book={book}/>)}</div></Row>
    </>}
  </div>;
}

function StarterShelf({ jobs }: { jobs: DownloadJob[] }) {
  return <section className="starter-shelf"><div><Download/><span><strong>Preparing your starter shelf</strong><small>Books appear here as soon as they are ready.</small></span></div><div>{jobs.map((job) => <span key={job.id}><i>{['queued', 'downloading', 'verifying', 'importing', 'indexing'].includes(job.status) ? <LoaderCircle className="spin"/> : job.status === 'failed' ? '!' : '✓'}</i><span>{job.title}<small>{job.error || `${job.message} · ${Math.round(job.progress * 100)}%`}</small></span></span>)}</div><Link to="/downloads">View downloads <ArrowRight/></Link></section>;
}

function Row({ title, link, children }: { title: string; link: string; children: React.ReactNode }) {
  return <section className="content-row"><div className="section-heading"><h2>{title}</h2><Link to={link}>See all <ArrowRight size={16}/></Link></div>{children}</section>;
}

export function FeatureCard({ title, count, type, books, to }: { title: string; count: number; type: string; books: Book[]; to: string }) {
  return <Link to={to} className="feature-card"><div className="cover-montage">{books.slice(0, 3).map((book) => book.has_cover ? <img key={book.id} src={api.coverUrl(book.id)} alt=""/> : <div key={book.id}/>)}</div><span className="eyebrow">{type}</span><strong>{title}</strong><small>{count} {count === 1 ? 'book' : 'books'}</small></Link>;
}
