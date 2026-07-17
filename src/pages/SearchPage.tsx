import { Search } from 'lucide-react';
import { useEffect, useState } from 'react';
import { BookCard } from '../components/BookCard';
import { api } from '../lib/api';
import type { Book, Series } from '../types';

export function SearchPage() {
  const [query, setQuery] = useState('');
  const [books, setBooks] = useState<Book[]>([]);
  const [series, setSeries] = useState<Series[]>([]);
  useEffect(() => { const timer = setTimeout(() => Promise.all([api.books(query), api.series()]).then(([b, s]) => { setBooks(b); setSeries(query ? s.filter((x) => x.name.toLowerCase().includes(query.toLowerCase())) : []); }), 160); return () => clearTimeout(timer); }, [query]);
  return <div className="page search-page"><header className="page-header"><div><span className="eyebrow">Find anything</span><h1>Search</h1></div></header><label className="search-field"><Search/><input autoFocus value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Title, author, tag, series…"/></label>{series.length > 0 && <section><h2>Series</h2><div className="search-series">{series.map((s) => <a href={`/collections/series/${s.id}`} key={s.id}><strong>{s.name}</strong><span>{s.books.length} books</span></a>)}</div></section>}<section><h2>{query ? `Books matching “${query}”` : 'All books'}</h2><div className="book-grid">{books.map((book) => <BookCard key={book.id} book={book}/>)}</div></section></div>;
}
