import { useEffect, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { Navigate, Route, Routes, useLocation } from 'react-router-dom';
import { Shell } from './components/Shell';
import { api, ApiError } from './lib/api';
import { BookDetailPage } from './pages/BookDetailPage';
import { CollectionPage, CollectionsPage } from './pages/CollectionsPage';
import { HomePage } from './pages/HomePage';
import { LibraryPage } from './pages/LibraryPage';
import { ReaderPage } from './pages/ReaderPage';
import { SearchPage } from './pages/SearchPage';
import { SettingsPage } from './pages/SettingsPage';
import { DownloadsPage } from './pages/DownloadsPage';

function DesktopOnly({ children }: { children: React.ReactNode }) {
  const desktop = isTauri() || ['localhost', '127.0.0.1'].includes(window.location.hostname);
  return desktop ? children : <Navigate to="/" replace />;
}

function PairingGate({ children }: { children: React.ReactNode }) {
  const location = useLocation();
  const [status, setStatus] = useState<'checking' | 'ready' | 'pairing' | 'required' | 'error'>('checking');
  const [message, setMessage] = useState('');
  const [manualCode, setManualCode] = useState('');

  const pairDevice = (code: string) => {
    const normalized = code.trim();
    if (!normalized) return;
    setMessage('');
    setStatus('pairing');
    api.pair(normalized).then(({ token }) => {
      localStorage.setItem('stori_reader_token', token);
      history.replaceState({}, '', location.pathname || '/');
      setStatus('ready');
    }).catch((error) => {
      setMessage(error instanceof Error ? error.message : 'Pairing failed');
      setStatus('required');
    });
  };

  useEffect(() => {
    const pairCode = new URLSearchParams(location.search).get('pair');
    const isDesktop = isTauri() || ['localhost', '127.0.0.1'].includes(window.location.hostname);
    if (pairCode) {
      pairDevice(pairCode);
      return;
    }
    if (isDesktop) {
      setStatus('ready');
      return;
    }
    if (localStorage.getItem('stori_reader_token')) {
      api.session().then(() => setStatus('ready')).catch((error) => {
        if (error instanceof ApiError && error.status === 401) {
          localStorage.removeItem('stori_reader_token');
          setMessage('This device was signed out. Pair it again to continue.');
          setStatus('required');
        } else {
          setMessage('The sTori server is unavailable.');
          setStatus('error');
        }
      });
      return;
    }
    api.health().then(() => setStatus('required')).catch(() => {
      setMessage('The sTori server is unavailable.');
      setStatus('error');
    });
  }, [location.pathname, location.search]);

  if (status === 'ready') return children;
  return <div className="gate"><img src="/stori-logo-transparent.png" alt="sTori" /><div className="gate-card"><h1>{status === 'required' ? 'Pair with sTori' : status === 'error' ? 'Could not connect' : 'Connecting…'}</h1><p>{status === 'required' ? 'Enter the six-digit code shown on the sTori Server page.' : message || 'Preparing your reading room.'}</p>{status === 'required' && <form className="pairing-form" onSubmit={(event) => { event.preventDefault(); pairDevice(manualCode); }}><input aria-label="Pairing code" inputMode="numeric" autoComplete="one-time-code" maxLength={6} placeholder="000000" value={manualCode} onChange={(event) => setManualCode(event.target.value.replace(/\D/g, '').slice(0, 6))}/>{message && <span>{message}</span>}<button className="primary-button" disabled={manualCode.length !== 6}>Pair this device</button></form>}{status === 'error' && <button className="primary-button" onClick={() => window.location.reload()}>Try again</button>}</div></div>;
}

export default function App() {
  return (
    <PairingGate>
      <Routes>
        <Route path="/read/:id" element={<ReaderPage />} />
        <Route element={<Shell />}>
          <Route index element={<HomePage />} />
          <Route path="library" element={<LibraryPage />} />
          <Route path="search" element={<SearchPage />} />
          <Route path="books/:id" element={<BookDetailPage />} />
          <Route path="collections" element={<CollectionsPage />} />
          <Route path="collections/:kind/:id" element={<CollectionPage />} />
          <Route path="settings" element={<DesktopOnly><SettingsPage /></DesktopOnly>} />
          <Route path="downloads" element={<DesktopOnly><DownloadsPage /></DesktopOnly>} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    </PairingGate>
  );
}
