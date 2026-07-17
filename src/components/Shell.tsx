import { BookCopy, Check, Download, DownloadIcon, Home, Library, LoaderCircle, RefreshCw, Search, Settings, Shapes, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { NavLink, Outlet } from 'react-router-dom';
import { api } from '../lib/api';
import {
  APP_FONT_STORAGE_KEY,
  AppFontId,
  fontOptions,
  isAppFontId,
  isReaderFontId,
  READER_FONT_STORAGE_KEY,
  ReaderFontId,
} from '../lib/typography';

const nav = [
  { to: '/', label: 'Home', icon: Home },
  { to: '/library', label: 'Library', icon: Library },
  { to: '/search', label: 'Search', icon: Search },
  { to: '/collections', label: 'Collections', icon: Shapes },
  { to: '/downloads', label: 'Download EPUBs', icon: Download, desktopOnly: true },
  { to: '/settings', label: 'Server', icon: Settings, desktopOnly: true },
];

export function Shell() {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [theme, setTheme] = useState<'light' | 'dark'>(() => localStorage.getItem('stori_shell_theme') === 'dark' ? 'dark' : 'light');
  const [appFont, setAppFont] = useState<AppFontId>(() => {
    const stored = localStorage.getItem(APP_FONT_STORAGE_KEY);
    return isAppFontId(stored) ? stored : 'merriweather';
  });
  const [readerFont, setReaderFont] = useState<ReaderFontId>(() => {
    const stored = localStorage.getItem(READER_FONT_STORAGE_KEY);
    return isReaderFontId(stored) ? stored : 'merriweather';
  });
  const [version, setVersion] = useState('');
  const [scanning, setScanning] = useState(false);
  const [scanMessage, setScanMessage] = useState('');
  const [activeDownloads, setActiveDownloads] = useState(0);
  const [updateStatus, setUpdateStatus] = useState<'idle' | 'checking' | 'available' | 'installing' | 'current' | 'failed'>('idle');
  const [updateMessage, setUpdateMessage] = useState('');

  const isDesktop = '__TAURI_INTERNALS__' in window;

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem('stori_shell_theme', theme);
  }, [theme]);

  useEffect(() => {
    document.documentElement.dataset.appFont = appFont;
    localStorage.setItem(APP_FONT_STORAGE_KEY, appFont);
  }, [appFont]);

  useEffect(() => {
    localStorage.setItem(READER_FONT_STORAGE_KEY, readerFont);
  }, [readerFont]);

  useEffect(() => {
    api.health().then((health) => setVersion(health.version)).catch(() => setVersion('unavailable'));
  }, []);

  const checkForUpdates = async () => {
    if (!isDesktop) return;
    setUpdateStatus('checking');
    setUpdateMessage('Checking for an update…');
    try {
      const { check } = await import('@tauri-apps/plugin-updater');
      const update = await check();
      if (!update) {
        setUpdateStatus('current');
        setUpdateMessage('sTori is up to date.');
        return;
      }
      setUpdateStatus('available');
      setUpdateMessage(`sTori v${update.version} is ready to install.`);
    } catch {
      setUpdateStatus('failed');
      setUpdateMessage('Could not check for updates. Try again later.');
    }
  };

  const installUpdate = async () => {
    setUpdateStatus('installing');
    setUpdateMessage('Downloading and installing the update…');
    try {
      const { check } = await import('@tauri-apps/plugin-updater');
      const update = await check();
      if (!update) {
        setUpdateStatus('current');
        setUpdateMessage('sTori is already up to date.');
        return;
      }
      await update.downloadAndInstall();
      const { relaunch } = await import('@tauri-apps/plugin-process');
      await relaunch();
    } catch {
      setUpdateStatus('failed');
      setUpdateMessage('The update could not be installed. Your current sTori is unchanged.');
    }
  };

  useEffect(() => {
    if (!isDesktop) return;
    const timer = window.setTimeout(() => { void checkForUpdates(); }, 1800);
    return () => window.clearTimeout(timer);
  }, [isDesktop]);

  useEffect(() => {
    const desktop = '__TAURI_INTERNALS__' in window || ['localhost', '127.0.0.1'].includes(window.location.hostname);
    if (!desktop) return;
    const load = () => api.downloadJobs().then((jobs) => setActiveDownloads(jobs.filter((job) => ['queued', 'downloading', 'verifying', 'importing', 'indexing'].includes(job.status)).length)).catch(() => undefined);
    load();
    const timer = window.setInterval(load, 2500);
    return () => clearInterval(timer);
  }, []);

  const rescan = () => {
    setScanning(true);
    setScanMessage('');
    api.rescanLibraries().then((result) => {
      setScanMessage(`Indexed ${result.indexed} books${result.warnings.length ? ` with ${result.warnings.length} warnings` : ''}.`);
      setScanning(false);
    }).catch((error) => {
      setScanMessage(error instanceof Error ? error.message : 'Library scan failed.');
      setScanning(false);
    });
  };

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <NavLink to="/" className="brand"><ThemedLogo /></NavLink>
        <nav>{nav.map(({ to, label, icon: Icon, desktopOnly }) => <NavLink key={to} to={to} className={desktopOnly ? 'desktop-only' : ''}><Icon size={19} /><span>{label}</span>{to === '/downloads' && activeDownloads > 0 && <em className="nav-download-badge">{activeDownloads}</em>}</NavLink>)}</nav>
        <div className="sidebar-foot"><BookCopy size={17} /><span>Your personal reading room</span></div>
      </aside>
      <header className="mobile-header">
        <NavLink to="/" className="mobile-brand" aria-label="sTori home"><ThemedLogo /></NavLink>
        <button className="mobile-settings-button" aria-label="Open settings" onClick={() => setSettingsOpen(true)}><Settings size={23}/></button>
      </header>
      <main className="main-content"><Outlet /></main>
      <nav className="bottom-nav">{nav.filter((item) => !item.desktopOnly).map(({ to, label, icon: Icon }) => <NavLink key={to} to={to}><Icon size={21} /><span>{label}</span></NavLink>)}</nav>

      {settingsOpen && (
        <div className="shell-settings-backdrop" onClick={() => setSettingsOpen(false)}>
          <section className="shell-settings-panel" role="dialog" aria-modal="true" aria-labelledby="shell-settings-title" onClick={(event) => event.stopPropagation()}>
            <header>
              <div><span className="eyebrow">sTori preferences</span><h2 id="shell-settings-title">Settings</h2></div>
              <button aria-label="Close settings" onClick={() => setSettingsOpen(false)}><X/></button>
            </header>
            <div className="shell-settings-content">
              <fieldset>
                <legend>Appearance</legend>
                <button className={theme === 'light' ? 'theme-choice active' : 'theme-choice'} onClick={() => setTheme('light')}><span><i className="theme-swatch light"/><span><strong>Light</strong><small>Current default</small></span></span>{theme === 'light' && <Check/>}</button>
                <button className={theme === 'dark' ? 'theme-choice active' : 'theme-choice'} onClick={() => setTheme('dark')}><span><i className="theme-swatch dark"/><span><strong>Dark</strong><small>Charcoal and dark grey</small></span></span>{theme === 'dark' && <Check/>}</button>
              </fieldset>

              <div className="settings-divider"/>
              <FontPicker title="App font" value={appFont} onChange={setAppFont} />

              <div className="settings-divider"/>
              <FontPicker title="Default reading font" value={readerFont} onChange={setReaderFont} includePublisher />

              <div className="settings-divider"/>
              <section>
                <h3>Library</h3>
                <p>Ask the PC server to check every configured library for changes.</p>
                <button className="primary-button rescan-button" disabled={scanning} onClick={rescan}>{scanning ? <LoaderCircle className="spin"/> : <RefreshCw/>}{scanning ? 'Scanning library…' : 'Refresh / rescan library'}</button>
                {scanMessage && <p className="settings-status">{scanMessage}</p>}
              </section>

              {isDesktop && <><div className="settings-divider"/><section>
                <h3>Updates</h3>
                <p>Secure updates are downloaded from sTori’s GitHub releases and verified before installation.</p>
                {updateStatus === 'available' ? <button className="primary-button rescan-button" onClick={installUpdate}><DownloadIcon/>Install update</button> : <button className="secondary-button rescan-button" disabled={updateStatus === 'checking' || updateStatus === 'installing'} onClick={checkForUpdates}>{updateStatus === 'checking' || updateStatus === 'installing' ? <LoaderCircle className="spin"/> : <RefreshCw/>}{updateStatus === 'checking' ? 'Checking…' : updateStatus === 'installing' ? 'Installing…' : 'Check for updates'}</button>}
                {updateMessage && <p className="settings-status" role="status">{updateMessage}</p>}
              </section></>}
            </div>
            <footer><span>sTori {version ? `v${version}` : 'version…'}</span><span>Developed by Aanish Farrukh (sudoaanish)</span></footer>
          </section>
        </div>
      )}
    </div>
  );
}

function ThemedLogo() {
  return <span className="themed-logo"><img className="logo-light" src="/stori-logo-transparent.png" alt="sTori" /><img className="logo-dark" src="/stori-logo-dark-transparent.png" alt="" /></span>;
}

function FontPicker<T extends AppFontId | ReaderFontId>({ title, value, onChange, includePublisher = false }: { title: string; value: T; onChange: (font: T) => void; includePublisher?: boolean }) {
  return (
    <fieldset className="font-picker">
      <legend>{title}</legend>
      {includePublisher && <button className={value === 'publisher' ? 'font-choice active' : 'font-choice'} onClick={() => onChange('publisher' as T)}><span><strong>Publisher font</strong><small>Use the typography embedded in the EPUB</small></span>{value === 'publisher' && <Check/>}</button>}
      {fontOptions.map((font) => <button key={font.id} className={value === font.id ? 'font-choice active' : 'font-choice'} style={{ fontFamily: font.family }} onClick={() => onChange(font.id as T)}><span><strong>{font.name}</strong><small>{font.description}</small></span>{value === font.id && <Check/>}</button>)}
    </fieldset>
  );
}
