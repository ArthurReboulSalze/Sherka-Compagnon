import { t, feedback } from './i18n';
import { getVoiceService, onVoiceService, retryVoiceService, type VoiceService } from './api';

let status: VoiceService | undefined;
// The splash covers only the first wait. Later rechecks (every spoken turn
// briefly reports `checking`) must never hide the companion again.
let settled = false;

// Widget startup: only the logo and a loading hint until VoiceStudio answers.
function paintSplash(): void {
  const surface = document.querySelector<HTMLElement>('.widget');
  if (!surface) return;
  const splash = !settled;
  surface.classList.toggle('splashing', splash);
  let node = surface.querySelector<HTMLElement>('.widget-splash');
  if (!splash) { node?.remove(); return; }
  if (!node) {
    node = document.createElement('div');
    node.className = 'widget-splash'; node.setAttribute('aria-live', 'off');
    const logo = document.createElement('div'); logo.className = 'splash-logo';
    const image = document.createElement('img');
    image.src = '/branding/sherka-logo-v2.png'; image.alt = ''; image.draggable = false;
    logo.append(image);
    const text = document.createElement('p'); text.className = 'splash-text';
    const message = document.createElement('span'); message.className = 'splash-message';
    const hint = document.createElement('small'); hint.textContent = t('Ton compagnon apparaît dès que la voix est prête.');
    text.append(message, hint); node.append(logo, text); surface.append(node);
  }
  node.querySelector('.splash-message')!.textContent = feedback(status?.message) || t('Connexion à VoiceStudio…');
}

export function paintVoiceService(): void {
  const waiting = status && ['checking', 'starting', 'error'].includes(status.phase);
  if (document.body.classList.contains('widget-body')) {
    paintSplash();
    const label = document.querySelector<HTMLElement>('[data-session-status]');
    if (waiting && label) label.textContent = status!.phase === 'error' ? t('VoiceStudio indisponible · ouvre le poste de bord') : feedback(status!.message);
    return;
  }
  let banner = document.querySelector<HTMLElement>('#voice-service-status');
  if (!waiting) { banner?.remove(); return; }
  if (!banner) {
    const header = document.querySelector('.topbar');
    if (!header) return;
    banner = document.createElement('div');
    banner.id = 'voice-service-status'; banner.className = 'voice-service-status';
    banner.setAttribute('role', 'status');
    banner.append(document.createElement('span'));
    const retry = document.createElement('button');
    retry.className = 'button secondary small'; retry.textContent = t('Réessayer VoiceStudio');
    retry.addEventListener('click', async () => {
      retry.disabled = true;
      try { await retryVoiceService(); }
      catch (error) { banner!.querySelector('span')!.textContent = feedback(String(error)); }
      finally { retry.disabled = false; }
    });
    banner.append(retry); header.after(banner);
  }
  banner.querySelector('span')!.textContent = feedback(status!.message);
  banner.querySelector('button')!.hidden = status!.phase !== 'error';
  banner.classList.toggle('error', status!.phase === 'error');
}

export async function mountVoiceService(restoreLive: () => void): Promise<void> {
  let events = 0;
  const paint = (value: VoiceService) => {
    status = value;
    if (value.phase !== 'checking' && value.phase !== 'starting') settled = true;
    restoreLive(); paintVoiceService();
  };
  try {
    await onVoiceService(value => { ++events; paint(value); });
    const before = events;
    const value = await getVoiceService();
    if (events === before) paint(value);
  } finally {
    // Never leave the widget on its splash if the status cannot be read.
    if (!status) { settled = true; paintVoiceService(); }
  }
}
