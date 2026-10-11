import { getUpdateStatus, checkUpdate, installUpdate, onUpdate, type UpdateStatus } from './api';

let status: UpdateStatus | undefined;

export function updatesView(): string {
  return `<section class="settings-card" id="updates-card"><h2>Mises à jour</h2><p>Retrouve les nouvelles versions de Sherka sur GitHub. La vérification se lance à ta demande.</p><small id="update-version"></small><p id="update-status" role="status"></p><progress id="update-progress" hidden></progress><details id="update-notes" hidden><summary>Ce qui change</summary><p></p></details><div class="settings-actions"><button class="button secondary" id="check-update">Vérifier les mises à jour</button><button class="button primary" id="install-update" hidden>Mettre à jour et redémarrer</button></div><small>Les profils et les souvenirs sont conservés. La voix s’arrête avant le redémarrage.</small></section>`;
}

export function paintUpdate(): void {
  if (!status) return;
  const card = document.querySelector<HTMLElement>('#updates-card');
  if (!card) return;
  const busy = ['checking', 'downloading', 'installing'].includes(status.phase);
  card.querySelector('#update-version')!.textContent = `Version ${status.current_version} · ${status.mode === 'portable' ? 'Sans installation' : 'Installée'}`;
  card.querySelector('#update-status')!.textContent = status.message;
  const check = card.querySelector<HTMLButtonElement>('#check-update')!;
  const install = card.querySelector<HTMLButtonElement>('#install-update')!;
  check.disabled = busy;
  install.disabled = busy;
  install.hidden = status.phase !== 'available';
  const progress = card.querySelector<HTMLProgressElement>('progress')!;
  progress.hidden = !['downloading', 'installing'].includes(status.phase);
  if (status.total) { progress.max = status.total; progress.value = status.downloaded; }
  else progress.removeAttribute('value');
  const notes = card.querySelector<HTMLDetailsElement>('details')!;
  notes.hidden = !status.notes || !status.version;
  notes.querySelector('p')!.textContent = status.notes ?? '';
  card.classList.toggle('update-error', status.phase === 'error');
}

export function mountUpdates(): void {
  paintUpdate();
  document.querySelector('#check-update')?.addEventListener('click', async () => {
    try { status = await checkUpdate(); paintUpdate(); }
    catch { /* Native status supplies the readable error and retry action. */ }
  });
  document.querySelector('#install-update')?.addEventListener('click', async () => {
    try { status = await installUpdate(); paintUpdate(); }
    catch { /* Do not replace other settings forms or drafts on failure. */ }
  });
}

export async function initializeUpdates(): Promise<void> {
  let events = 0;
  await onUpdate(value => { ++events; status = value; paintUpdate(); });
  const before = events;
  const value = await getUpdateStatus();
  if (events === before) { status = value; paintUpdate(); }
}
