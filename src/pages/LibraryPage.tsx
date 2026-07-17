import { SlidersHorizontal } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { BookCard } from '../components/BookCard';
import { api } from '../lib/api';
import { bookMatchesGenre, genreBySlug } from '../lib/genres';
import type { Book } from '../types';

export function LibraryPage() {
  const [params] = useSearchParams();
  const [books, setBooks] = useState<Book[]>([]);
  const [format, setFormat] = useState('');
  const [sort, setSort] = useState('title');
  const genre = genreBySlug(params.get('genre'));
  useEffect(() => { api.books('', params.get('status') ? { status: params.get('status')! } : {}).then(setBooks); }, [params]);
  const shown = useMemo(() => books.filter((book) => (!genre || bookMatchesGenre(book, genre)) && (!format || book.format === format)).sort((a, b) => sort === 'recent' ? b.updated_at.localeCompare(a.updated_at) : sort === 'author' ? (a.authors[0] || '').localeCompare(b.authors[0] || '') : a.title.localeCompare(b.title)), [books, format, genre, sort]);
  return <div className="page"><header className="page-header"><div><span className="eyebrow">{genre ? 'Genre' : 'All books'}</span><h1>{genre?.name || 'Library'}</h1><p>{shown.length} titles</p></div><div className="filter-bar"><SlidersHorizontal size={17}/><select value={format} onChange={(e) => setFormat(e.target.value)}><option value="">All readable formats</option><option value="epub">EPUB</option><option value="pdf">PDF</option></select><select value={sort} onChange={(e) => setSort(e.target.value)}><option value="title">Title</option><option value="author">Author</option><option value="recent">Recently added</option></select></div></header><div className="book-grid">{shown.map((book) => <BookCard key={book.id} book={book}/>)}</div></div>;
}
