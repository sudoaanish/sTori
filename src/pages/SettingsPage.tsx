import { CheckCircle2, Database, FolderOpen, HardDrive, LoaderCircle, QrCode, RefreshCw, Server, ShieldCheck, Smartphone, Trash2, Wifi } from 'lucide-react';
import { useEffect, useState } from 'react';
import { QRCodeSVG } from 'qrcode.react';
import { invoke } from '@tauri-apps/api/core';
import { api } from '../lib/api';
import type { Connectivity, DatabaseBackup, Diagnostics, Library, PairedDevice } from '../types';

export function SettingsPage() {
  const [libraries, setLibraries] = useState<Library[]>([]);
  const [connectivity, setConnectivity] = useState<Connectivity>();
  const [devices, setDevices] = useState<PairedDevice[]>([]);
  const [diagnostics, setDiagnostics] = useState<Diagnostics>();
  const [backups, setBackups] = useState<DatabaseBackup[]>([]);
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState('My Books');
  const [path, setPath] = useState('');
  const [busy, setBusy] = useState('');
  const [message, setMessage] = useState('');
  const [startWithWindows, setStartWithWindows] = useState(false);
  const isDesktop = '__TAURI_INTERNALS__' in window;

  const load = () => Promise.all([api.libraries(), api.connectivity(), api.pairedDevices(), api.diagnostics(), api.backups()])
    .then(([libraryRows, connection, deviceRows, diagnosticRows, backupRows]) => {
      setLibraries(libraryRows);
      setConnectivity(connection);
      setDevices(deviceRows);
      setDiagnostics(diagnosticRows);
      setBackups(backupRows);
    });

  useEffect(() => {
    load().catch((error) => setMessage(error.message));
    if (isDesktop) invoke<boolean>('get_autostart').then(setStartWithWindows).catch(() => undefined);
  }, []);

  const setAutostart = async (enabled: boolean) => {
    setBusy('autostart');
    try {
      await invoke('set_autostart', { enabled });
      setStartWithWindows(enabled);
      setMessage(enabled ? 'sTori will start minimized to the system tray when you sign in to Windows.' : 'sTori will no longer start with Windows.');
    } catch (error) {
      setMessage(error instanceof Error ? error.message : 'Could not change the Windows startup setting.');
    } finally { setBusy(''); }
  };

  const browse = async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({ directory: true, multiple: false, title: 'Select an sTori library folder' });
      if (selected) setPath(selected as string);
    } catch {
      setMessage('Native folder selection is available inside the sTori desktop app.');
    }
  };

  const add = () => {
    setBusy('add-library'); setMessage('');
    api.addLibrary(name, path)
      .then((library) => api.scanLibrary(library.id))
      .then((result) => { setAdding(false); setMessage(`Indexed ${result.indexed} books${result.warnings.length ? ` with ${result.warnings.length} warnings` : ''}.`); return load(); })
      .catch((error) => setMessage(error.message))
      .finally(() => setBusy(''));
  };

  const pair = () => {
    setBusy('pair'); setMessage('');
    api.createPairing().then(setConnectivity).catch((error) => setMessage(error.message)).finally(() => setBusy(''));
  };

  const revoke = (device: PairedDevice) => {
    if (!window.confirm(`Revoke access for ${device.name}? That device will need to pair again.`)) return;
    setBusy(device.id);
    api.revokeDevice(device.id).then(load).catch((error) => setMessage(error.message)).finally(() => setBusy(''));
  };

  const revokeAll = () => {
    if (!window.confirm('Revoke every paired device? All iPhones will need to pair again.')) return;
    setBusy('revoke-all');
    api.revokeAllDevices().then(load).catch((error) => setMessage(error.message)).finally(() => setBusy(''));
  };

  const backup = () => {
    setBusy('backup'); setMessage('');
    api.createBackup().then((created) => { setMessage(`Backup created: ${created.file_name}`); return load(); }).catch((error) => setMessage(error.message)).finally(() => setBusy(''));
  };

  return <div className="page settings-page">
    <header className="page-header"><div><span className="eyebrow">Local sTori server</span><h1>Server & Libraries</h1><p>Private, local, and under your control.</p></div></header>

    <section className="server-card">
      <div className="server-status"><CheckCircle2/><div><strong>sTori server is running</strong><span>Port {connectivity?.port || 1822} · version {connectivity?.version || '0.1.0'}</span></div></div>
      <div className="connection-grid">
        <div><h3><QrCode/> Connect an iPhone</h3><p>Create a one-use code that expires after ten minutes.</p><button className="primary-button" disabled={busy === 'pair'} onClick={pair}>{busy === 'pair' ? <LoaderCircle className="spin"/> : <QrCode/>}Create pairing code</button>{connectivity?.pairing_code && connectivity.urls.filter((url) => url.recommended).slice(0, 1).map((url) => { const link = `${url.url}/?pair=${connectivity.pairing_code}`; return <div className="qr-card" key={link}><QRCodeSVG value={link} size={168} bgColor="#f7f3eb" fgColor="#10111d"/><span className="pairing-code">{connectivity.pairing_code}</span><strong>{url.label}</strong><code>{url.url}</code><small>One use · expires in 10 minutes</small></div>; })}</div>
        <div><h3><Server/> Addresses</h3>{connectivity?.urls.map((url) => <div className="address-row" key={url.url}><span>{url.label}{url.recommended && <em>Recommended</em>}</span><code>{url.url}</code></div>)}</div>
      </div>
    </section>

    {isDesktop && <section className="settings-section"><div className="section-heading"><div><span className="eyebrow">Desktop behavior</span><h2>Windows startup</h2></div></div><div className="startup-setting"><div><strong>Start sTori with Windows</strong><span>Launch the server minimized to the system tray after you sign in. Closing the window also keeps the server available in the tray.</span></div><button className={startWithWindows ? 'primary-button' : 'secondary-button'} disabled={busy === 'autostart'} onClick={() => setAutostart(!startWithWindows)}>{busy === 'autostart' && <LoaderCircle className="spin"/>}{startWithWindows ? 'On' : 'Off'}</button></div></section>}

    <section className="settings-section"><div className="section-heading"><div><span className="eyebrow">Access control</span><h2>Paired devices</h2></div>{devices.length > 1 && <button className="secondary-button danger-button" disabled={busy === 'revoke-all'} onClick={revokeAll}><Trash2/>Revoke all</button>}</div>{devices.length ? <div className="device-list">{devices.map((device) => <article className="device-card" key={device.id}><Smartphone/><div><strong>{device.name}</strong><span>Last seen {new Date(device.last_seen_at).toLocaleString()}{device.last_ip ? ` · ${device.last_ip}` : ''}</span><small>Paired {new Date(device.created_at).toLocaleDateString()}</small></div><button className="secondary-button danger-button" disabled={busy === device.id} onClick={() => revoke(device)}>{busy === device.id ? <LoaderCircle className="spin"/> : <Trash2/>}Revoke</button></article>)}</div> : <div className="empty-state compact">No iPhones are currently paired.</div>}</section>

    <section className="settings-section"><div className="section-heading"><div><span className="eyebrow">Recovery</span><h2>Database & diagnostics</h2></div><button className="primary-button" disabled={busy === 'backup'} onClick={backup}>{busy === 'backup' ? <LoaderCircle className="spin"/> : <Database/>}Back up now</button></div><div className="diagnostic-grid"><article><ShieldCheck/><div><strong>Database {diagnostics?.database_status === 'ok' ? 'healthy' : 'needs attention'}</strong><span>Schema version {diagnostics?.schema_version ?? '—'}</span><code>{diagnostics?.database_path}</code></div></article><article><HardDrive/><div><strong>{formatBytes(diagnostics?.managed_library_free_bytes)} free</strong><span>Managed book storage</span><code>{diagnostics?.backup_directory}</code></div></article><article><Wifi/><div><strong>{diagnostics?.firewall_rule_detected === true ? 'Firewall permission detected' : diagnostics?.firewall_rule_detected === false ? 'Firewall permission not detected' : 'Check firewall permission'}</strong><span>{diagnostics?.firewall_guidance || 'Checking Windows Firewall…'}</span><code>TCP port 1822</code></div></article></div>{backups.length > 0 && <div className="backup-list"><strong>Recent backups</strong>{backups.slice(0, 5).map((entry) => <div key={entry.path}><span>{entry.file_name}</span><small>{formatBytes(entry.size_bytes)} · {new Date(entry.created_at).toLocaleString()}</small></div>)}</div>}</section>

    <section className="library-settings"><div className="section-heading"><div><span className="eyebrow">Book storage</span><h2>Libraries</h2></div><button className="primary-button" onClick={() => setAdding(true)}>+ Add library</button></div>{libraries.map((library) => { const available = diagnostics?.libraries.find((item) => item.id === library.id)?.available ?? true; return <article className="library-card" key={library.id}><FolderOpen/><div><strong>{library.name}</strong><code>{library.path}</code><span>{library.book_count} books · {available ? (library.last_scanned_at ? `Last scan ${new Date(library.last_scanned_at).toLocaleString()}` : 'Not scanned') : 'Folder unavailable'}</span></div><button className="secondary-button" disabled={busy === `scan-${library.id}` || !available} onClick={() => { setBusy(`scan-${library.id}`); api.scanLibrary(library.id).then((result) => { setMessage(`Indexed ${result.indexed} books.`); return load(); }).catch((error) => setMessage(error.message)).finally(() => setBusy('')); }}>{busy === `scan-${library.id}` ? <LoaderCircle className="spin"/> : <RefreshCw/>}Scan</button></article>; })}{!libraries.length && <div className="empty-state compact">No libraries configured.</div>}</section>
    {message && <p className="status-message settings-global-message">{message}</p>}

    {adding && <div className="modal-backdrop"><div className="modal"><h2>Add library</h2><label>Name<input value={name} onChange={(event) => setName(event.target.value)}/></label><label>Folder<div className="path-input"><input value={path} placeholder="Choose a folder…" onChange={(event) => setPath(event.target.value)}/><button onClick={browse}>Browse…</button></div></label><div className="option-list"><label><input type="checkbox" checked readOnly/>Read metadata.opf and EPUB metadata</label><label><input type="checkbox" checked readOnly/>Use cover.jpg or folder.jpg</label></div><div className="modal-actions"><button className="secondary-button" onClick={() => setAdding(false)}>Cancel</button><button className="primary-button" disabled={busy === 'add-library' || !path.trim()} onClick={add}>{busy === 'add-library' && <LoaderCircle className="spin"/>}Add & scan</button></div></div></div>}
  </div>;
}

function formatBytes(bytes?: number) {
  if (bytes === undefined) return 'Checking…';
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}
