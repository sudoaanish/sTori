import { Check, ChevronDown, Download, LoaderCircle, Pause, Play, Search, Trash2, X } from 'lucide-react';
import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api } from '../lib/api';
import type { CatalogBook, DownloadJob, Library } from '../types';

const activeStatuses = ['queued', 'downloading', 'verifying', 'importing', 'indexing'];

export function DownloadsPage() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<CatalogBook[]>([]);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [libraries, setLibraries] = useState<Library[]>([]);
  const [jobs, setJobs] = useState<DownloadJob[]>([]);
  const [searching, setSearching] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [nextPage, setNextPage] = useState<number>();
  const [searchError, setSearchError] = useState('');
  const searchRequest = useRef(0);
  const [busy, setBusy] = useState('');
  const [message, setMessage] = useState('');
  const [toast, setToast] = useState<string>();
  const [showHistory, setShowHistory] = useState(false);
  const managedLibrary = libraries.find((library) => library.managed);

  const refreshJobs = useCallback(() => api.downloadJobs().then(setJobs).catch(() => undefined), []);
  useEffect(() => {
    api.libraries().then(setLibraries).catch((error) => setMessage(error.message));
    refreshJobs();
  }, [refreshJobs]);
  useEffect(() => {
    const active = jobs.some((job) => activeStatuses.includes(job.status));
    const timer = window.setTimeout(refreshJobs, active ? 700 : 3500);
    return () => clearTimeout(timer);
  }, [jobs, refreshJobs]);

  const jobsByEdition = useMemo(() => new Map(jobs.map((job) => [`${job.provider}:${job.provider_id}`, job])), [jobs]);
  const currentJobs = jobs.filter((job) => !['completed', 'cancelled'].includes(job.status));
  const historyJobs = jobs.filter((job) => ['completed', 'cancelled'].includes(job.status));
  const visibleJobs = showHistory ? [...currentJobs, ...historyJobs] : currentJobs;

  const search = (event: FormEvent) => {
    event.preventDefault();
    runSearch(1, true);
  };
  const runSearch = (page: number, replace: boolean) => {
    if (query.trim().length < 2) return;
    const request = ++searchRequest.current;
    if (replace) { setSearching(true); setSearchError(''); setWarnings([]); }
    else setLoadingMore(true);
    setMessage('');
    api.catalogSearch(query, page)
      .then((response) => {
        if (request !== searchRequest.current) return;
        setResults((current) => replace ? uniqueBooks(response.results) : uniqueBooks([...current, ...response.results]));
        setWarnings(response.warnings);
        setNextPage(response.next_page);
        if (replace && !response.results.length) setMessage('No downloadable Project Gutenberg EPUBs found.');
      })
      .catch((error) => { if (request === searchRequest.current) setSearchError(error.message || 'Project Gutenberg could not be reached.'); })
      .finally(() => { if (request === searchRequest.current) { setSearching(false); setLoadingMore(false); } });
  };

  const queue = (book: CatalogBook) => {
    if (!managedLibrary) {
      setMessage('The managed sTori Books library is not ready yet.');
      return;
    }
    const key = `${book.provider}:${book.provider_id}`;
    setBusy(key);
    setMessage('');
    api.queueDownload(managedLibrary.id, book)
      .then((job) => {
        setJobs((current) => [job, ...current.filter((item) => item.id !== job.id)]);
        setToast(`“${book.title}” was added to the download queue.`);
      })
      .catch((error) => setMessage(error.message))
      .finally(() => setBusy(''));
  };

  const act = (job: DownloadJob, action: 'pause' | 'resume' | 'cancel' | 'delete') => {
    setBusy(job.id);
    const call = action === 'pause' ? api.pauseDownload(job.id)
      : action === 'resume' ? api.resumeDownload(job.id)
      : action === 'cancel' ? api.cancelDownload(job.id)
      : api.deleteDownload(job.id);
    call.then(refreshJobs).catch((error) => setMessage(error.message)).finally(() => setBusy(''));
  };

  const viewQueue = () => document.getElementById('download-queue')?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  const clearHistory = () => {
    setBusy('clear-history');
    api.clearFinishedDownloads().then(refreshJobs).catch((error) => setMessage(error.message)).finally(() => setBusy(''));
  };

  return <div className="page downloads-page">
    <header className="page-header"><div><span className="eyebrow">Desktop library tools</span><h1>Download EPUBs</h1><p>Find public-domain books and add them to your personal reading room.</p></div></header>
    <div className="download-notice"><strong>Project Gutenberg</strong><span>sTori validates every EPUB and imports its embedded metadata and cover before adding it to your library.</span></div>
    {managedLibrary && <div className="managed-library-destination"><Download/><span><strong>Downloads save to sTori Books</strong><code>{managedLibrary.path}</code></span></div>}
    <form className="catalog-search" onSubmit={search}>
      <div><Search size={20}/><input aria-label="Search downloadable EPUBs" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search Project Gutenberg by title or author…"/></div>
      <button className="primary-button" disabled={searching || query.trim().length < 2}>{searching ? <LoaderCircle className="spin"/> : <Search/>}Search</button>
    </form>
    {message && !searchError && <p className="download-message">{message}</p>}
    {searchError && <p className="download-message" role="alert">{searchError} <button onClick={() => runSearch(results.length ? nextPage || 1 : 1, !results.length)}>Retry</button></p>}
    {warnings.map((warning) => <p className="download-warning" key={warning}>{warning}</p>)}

    {jobs.length > 0 && <section className="download-section" id="download-queue"><div className="section-heading"><h2>Download queue</h2>{historyJobs.length > 0 && <div className="download-history-actions"><button onClick={() => setShowHistory((value) => !value)}><ChevronDown className={showHistory ? 'history-open' : ''}/>{showHistory ? 'Hide' : 'Show'} completed ({historyJobs.length})</button>{showHistory && <button disabled={busy === 'clear-history'} onClick={clearHistory}><Trash2/>Clear completed</button>}</div>}</div>{visibleJobs.length > 0 ? <div className="job-list">{visibleJobs.map((job) => <article className="download-job" key={job.id}><div className="job-heading"><div><strong>{job.title}</strong><span>{job.authors.join(', ') || 'Unknown author'} · {job.provider}</span></div><span className={`job-status ${job.status}`}>{job.status}</span></div><div className="job-progress"><i style={{ width: `${Math.round(job.progress * 100)}%` }}/></div><div className="job-footer"><span>{job.error || job.message} · {formatBytes(job.bytes_downloaded)}{job.total_bytes ? ` of ${formatBytes(job.total_bytes)}` : ''} · {Math.round(job.progress * 100)}%</span><div>{['queued', 'downloading'].includes(job.status) && <button title="Pause" onClick={() => act(job, 'pause')}><Pause/></button>}{['paused', 'failed'].includes(job.status) && <button title="Resume" onClick={() => act(job, 'resume')}><Play/></button>}{!['completed', 'failed', 'cancelled'].includes(job.status) && <button title="Cancel" onClick={() => act(job, 'cancel')}><X/></button>}{['completed', 'failed', 'cancelled'].includes(job.status) && <button title="Remove from queue" onClick={() => act(job, 'delete')}><Trash2/></button>}</div></div></article>)}</div> : <p className="download-history-empty">No active downloads. Completed downloads are safely tucked into history.</p>}</section>}

    <section className="download-section"><div className="section-heading"><h2>{results.length ? `${results.length} results` : searching ? 'Searching Project Gutenberg…' : 'Search Project Gutenberg'}</h2></div>{results.length > 0 && <div className="catalog-grid">{results.map((book) => {
      const key = `${book.provider}:${book.provider_id}`;
      const job = jobsByEdition.get(key);
      const isBusy = busy === key || busy === job?.id;
      const retryable = job?.status === 'failed' || job?.status === 'paused';
      const completed = job?.status === 'completed';
      const active = job && activeStatuses.includes(job.status);
      return <article className="catalog-card" key={key}>{book.cover_url ? <img src={book.cover_url} alt="" loading="lazy" referrerPolicy="no-referrer"/> : <div className="catalog-cover-placeholder"><Download/></div>}<div className="catalog-card-body"><span className="catalog-provider">{book.provider}</span><h3>{book.title}</h3><p className="catalog-author">{book.authors.join(', ') || 'Unknown author'}</p>{book.description && <p className="catalog-description">{book.description}</p>}<span className="catalog-license">{book.license_label}</span>{retryable ? <button className="primary-button" disabled={isBusy} onClick={() => act(job, 'resume')}>{isBusy ? <LoaderCircle className="spin"/> : <Play/>}Resume download</button> : <button className={`primary-button ${completed ? 'download-complete-button' : ''}`} disabled={isBusy || Boolean(active) || completed || !managedLibrary} onClick={() => queue(book)}>{isBusy ? <LoaderCircle className="spin"/> : completed ? <Check/> : active ? <LoaderCircle className="spin"/> : <Download/>}{completed ? 'Added to library' : active ? `${job.message} · ${Math.round(job.progress * 100)}%` : 'Add to library'}</button>}</div></article>;
    })}</div>}{results.length > 0 && nextPage && <button className="secondary-button" disabled={loadingMore} onClick={() => runSearch(nextPage, false)}>{loadingMore ? <LoaderCircle className="spin"/> : 'Load more'}</button>}</section>

    {toast && <aside className="download-toast" role="status"><Check/><span>{toast}</span><button onClick={viewQueue}>View queue</button><button aria-label="Dismiss notification" onClick={() => setToast(undefined)}><X/></button></aside>}
  </div>;
}

export function uniqueBooks(books: CatalogBook[]) {
  return [...new Map(books.map((book) => [`${book.provider}:${book.provider_id}`, book])).values()];
}

function formatBytes(bytes: number) {
  if (!bytes) return '0 KB';
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
