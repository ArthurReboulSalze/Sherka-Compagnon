import { html, setLanguage } from './i18n';
import { getStartupIssue, closeStartupRecovery } from './api';
import { updatesView, mountUpdates, initializeUpdates } from './updates';

const escape = (value: string) => value.replace(/[&<>"']/g, c => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]!));

export async function mountStartupRecovery(app: HTMLElement): Promise<void> {
  const issue = await getStartupIssue();
  if (!issue) throw new Error('No startup recovery state');
  setLanguage(issue.language);
  app.innerHTML = html`<main class="startup-recovery"><h1>Sherka ne peut pas ouvrir ses données</h1><p>Une version plus récente peut être nécessaire. Vérifie les mises à jour avant de relancer Sherka.</p><p>Tes données n’ont pas été réinitialisées. La conversation et le micro sont désactivés dans ce mode.</p>${updatesView()}<details class="settings-card"><summary>Détail du démarrage</summary><p class="form-error">${escape(issue.error)}</p><p class="storage-path">${escape(issue.data_directory)}</p><small>Les versions installable et sans installation partagent ces données.</small></details><button class="button secondary" id="close-recovery">Quitter Sherka</button></main>`;
  await initializeUpdates();
  mountUpdates();
  app.querySelector('#close-recovery')!.addEventListener('click', () => void closeStartupRecovery());
}
