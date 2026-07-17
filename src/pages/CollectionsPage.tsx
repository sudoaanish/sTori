import { Plus, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';
import { BookCard } from '../components/BookCard';
import { FeatureCard } from './HomePage';
import { api } from '../lib/api';
import type { Book, Collection, Series } from '../types';

export function CollectionsPage() {
  const [collections, setCollections] = useState<Collection[]>([]);
  const [series, setSeries] = useState<Series[]>([]);
  const [creating, setCreating] = useState(false);
  const load = () => Promise.all([api.collections(), api.series()]).then(([c, s]) => { setCollections(c); setSeries(s); });
  useEffect(() => { load(); }, []);
  return <div className="page"><header className="page-header"><div><span className="eyebrow">Curated shelves</span><h1>Collections & Series</h1><p>Reading orders and lists that make your library yours.</p></div><button className="primary-button desktop-only-inline" onClick={() => setCreating(true)}><Plus/> New collection</button></header>{series.length > 0 && <section className="content-row"><div className="section-heading"><h2>Series</h2></div><div className="collection-grid">{series.map((item) => <FeatureCard key={item.id} title={item.name} count={item.books.length} type="Series" books={item.books} to={`/collections/series/${item.id}`}/>)}</div></section>}<section className="content-row"><div className="section-heading"><h2>My collections</h2></div>{collections.length ? <div className="collection-grid">{collections.map((item) => <FeatureCard key={item.id} title={item.name} count={item.books.length} type="Collection" books={item.books} to={`/collections/collection/${item.id}`}/>)}</div> : <div className="empty-state compact"><p>No manual collections yet.</p></div>}</section>{creating && <CollectionEditor onClose={() => setCreating(false)} onSaved={() => { setCreating(false); load(); }}/>}</div>;
}

function CollectionEditor({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const [books, setBooks] = useState<Book[]>([]); const [selected, setSelected] = useState<number[]>([]); const [name, setName] = useState(''); const [description, setDescription] = useState('');
  useEffect(() => { api.books().then(setBooks); }, []);
  const toggle = (id: number) => setSelected((value) => value.includes(id) ? value.filter((x) => x !== id) : [...value, id]);
  return <div className="modal-backdrop"><div className="modal large"><button className="modal-close" onClick={onClose}><X/></button><span className="eyebrow">New curated shelf</span><h2>Create collection</h2><label>Name<input value={name} onChange={(e) => setName(e.target.value)} placeholder="The Chronicles of Narnia Series"/></label><label>Description<textarea value={description} onChange={(e) => setDescription(e.target.value)} placeholder="Optional description"/></label><p>Select books in reading order ({selected.length} selected).</p><div className="book-grid selection-grid">{books.map((book) => <BookCard key={book.id} book={book} selectable selected={selected.includes(book.id)} onSelect={toggle}/>)}</div><div className="modal-actions"><button className="secondary-button" onClick={onClose}>Cancel</button><button className="primary-button" disabled={!name.trim() || !selected.length} onClick={() => api.createCollection({name: name.trim(), description, book_ids: selected}).then(onSaved)}>Create collection</button></div></div></div>;
}

export function CollectionPage() {
  const { kind, id } = useParams(); const [item, setItem] = useState<Collection | Series>();
  useEffect(() => { if (id) (kind === 'series' ? api.seriesById(Number(id)) : api.collection(Number(id))).then(setItem); }, [kind, id]);
  if (!item) return <div className="page empty-state">Opening collection…</div>;
  return <div className="page collection-page"><header className="collection-banner"><div><span className="eyebrow">{kind === 'series' ? 'Series' : 'Collection'}</span><h1>{item.name}</h1><p>{item.description || `${item.books.length} books in reading order.`}</p></div><div className="cover-montage large">{item.books.slice(0,3).map((book) => book.has_cover && <img src={api.coverUrl(book.id)} alt="" key={book.id}/>)}</div></header><ol className="ordered-books">{item.books.map((book, index) => <li key={book.id}><span>{index + 1}</span><BookCard book={book}/></li>)}</ol></div>;
}
