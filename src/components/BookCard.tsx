import { BookOpen } from 'lucide-react';
import { Link, useLocation } from 'react-router-dom';
import { api } from '../lib/api';
import type { Book } from '../types';

export function BookCard({ book, selectable, selected, onSelect }: { book: Book; selectable?: boolean; selected?: boolean; onSelect?: (id: number) => void }) {
  const location = useLocation();
  const card = (
    <>
      <div className="book-cover-wrap">
        {book.has_cover ? <img className="book-cover" src={api.coverUrl(book.id)} alt={`Cover of ${book.title}`} loading="lazy" /> : <div className="book-cover placeholder"><BookOpen /><span>{book.title}</span></div>}
        {book.progress > 0 && <div className="cover-progress"><span style={{ width: `${Math.round(book.progress * 100)}%` }} /></div>}
        {selectable && <span className={`select-mark ${selected ? 'selected' : ''}`}>{selected ? '✓' : '+'}</span>}
      </div>
      <strong>{book.title}</strong>
      <span>{book.authors.join(', ') || 'Unknown author'}</span>
    </>
  );
  if (selectable) return <button className="book-card selectable" onClick={() => onSelect?.(book.id)}>{card}</button>;
  return <Link className="book-card" to={`/books/${book.id}`} state={{ from: `${location.pathname}${location.search}` }}>{card}</Link>;
}
