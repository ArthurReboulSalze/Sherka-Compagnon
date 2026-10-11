import { t, feedback } from './i18n';
import { getVoiceService, onVoiceService, retryVoiceService, type VoiceService } from './api';

let status: VoiceService | undefined;

export function paintVoiceService(): void {
  const waiting = status && ['checking', 'starting', 'error'].includes(status.phase);
  if (document.body.classList.contains('widget-body')) {
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
  const paint = (value: VoiceService) => { status = value; restoreLive(); paintVoiceService(); };
  await onVoiceService(value => { ++events; paint(value); });
  const before = events;
  const value = await getVoiceService();
  if (events === before) paint(value);
}
