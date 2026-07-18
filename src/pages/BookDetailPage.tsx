import { ArrowLeft, BookOpen, Bookmark } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Link, useLocation, useNavigate, useParams } from 'react-router-dom';
import { api } from '../lib/api';
import type { Book } from '../types';

export function BookDetailPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const location = useLocation();
  const origin = (location.state as { from?: string } | null)?.from;
  const [book, setBook] = useState<Book>();

  useEffect(() => { if (id) api.book(Number(id)).then(setBook); }, [id]);
  if (!book) return <div className="page empty-state">Opening book…</div>;

  const backLabel = origin?.startsWith('/library') ? 'Library' : origin?.startsWith('/search') ? 'Search' : origin?.startsWith('/collections') ? 'Collection' : 'Home';
  const goBack = () => origin ? navigate(-1) : navigate('/', { replace: true });

  return (
    <div className="page detail-page">
      <button className="back-button" onClick={goBack} aria-label={`Back to ${backLabel}`}><ArrowLeft/><span>{backLabel}</span></button>
      <div className="detail-hero">
        <div className="detail-cover">{book.has_cover ? <img src={api.coverUrl(book.id)} alt={`Cover of ${book.title}`}/> : <div className="book-cover placeholder"><BookOpen/></div>}</div>
        <div className="detail-copy">
          <span className="eyebrow">{book.format.toUpperCase()} {book.published ? `· ${book.published.slice(0, 4)}` : ''}</span>
          <h1>{book.title}</h1>
          {book.subtitle && <h2>{book.subtitle}</h2>}
          <p className="authors">{book.authors.join(', ') || 'Unknown author'}</p>
          {book.series_id && <Link className="series-link" to={`/collections/series/${book.series_id}`}>{book.series_name}{book.series_index ? ` · Book ${book.series_index}` : ''}</Link>}
          <div className="detail-actions"><Link className="primary-button" to={`/read/${book.id}`} state={{ fromBook: true, bookOrigin: origin || '/' }}><BookOpen size={18}/>{book.progress ? 'Continue reading' : 'Start reading'}</Link><Link className="secondary-button" to={`/read/${book.id}`} state={{ fromBook: true, bookOrigin: origin || '/' }}><Bookmark size={18}/> Bookmark in reader</Link></div>
          {book.progress > 0 && <div className="detail-progress"><div><span style={{width: `${book.progress * 100}%`}}/></div><small>{Math.round(book.progress * 100)}% complete</small></div>}
          {book.description && <p className="description">{book.description}</p>}
          <div className="tag-list">{book.tags.map((tag) => <span key={tag}>{tag}</span>)}</div>
          <dl className="metadata"><div><dt>File</dt><dd>{book.file_name}</dd></div><div><dt>Identifier</dt><dd>{book.identifier || '—'}</dd></div><div><dt>Format</dt><dd>{book.format.toUpperCase()}</dd></div></dl>
        </div>
      </div>
    </div>
  );
}
