import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import App from './App';
import './fonts.css';
import './styles.css';
import './polish.css';

if ('serviceWorker' in navigator && location.protocol === 'https:') {
  window.addEventListener('load', async () => {
    const registration = await navigator.serviceWorker.register('/sw.js', { updateViaCache: 'none' });
    await registration.update();
    registration.addEventListener('updatefound', () => registration.installing?.addEventListener('statechange', () => {
      if (registration.waiting) registration.waiting.postMessage({ type: 'SKIP_WAITING' });
    }));
    navigator.serviceWorker.addEventListener('controllerchange', () => {
      if (!sessionStorage.getItem('stori-sw-reloaded')) {
        sessionStorage.setItem('stori-sw-reloaded', 'true');
        window.location.reload();
      }
    });
  });
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </StrictMode>
);
