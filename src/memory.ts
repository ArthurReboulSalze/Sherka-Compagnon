import { t, html, formatLocale } from './i18n';
import * as api from './api';
import type { MemoryEdit, MemoryEntry, MemoryState, Profile } from './types';

const escape = (s: string) => s.replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
const date = (n: number) => new Intl.DateTimeFormat(formatLocale(), {dateStyle:'medium',timeStyle:'short'}).format(n);

export function memoryPanel(profile: Profile): string {
  if (profile.connections.llm_provider === 'hermes') return html`<section class="settings-card"><h2>Mémoire Hermes</h2><p>Hermes gère sa personnalité et ses souvenirs sur le serveur. Demande-lui directement de retenir, corriger ou oublier une information.</p></section>`;
  return html`<section class="settings-card"><div class="section-heading"><h2>Son caractère</h2><button class="button quiet small" id="memory-personality">Modifier sa personnalité</button></div><p>` + escape(profile.personality) + html`</p><p class="connection-note">Le caractère guide ses réponses sans être récité. Il reste propre à ce compagnon, quel que soit son modèle.</p></section>`
    + '<section class="settings-card" id="memory-root" data-profile="' + profile.id + '">'
    + html`<div class="section-heading"><h2>Ses souvenirs</h2><button class="button secondary small" id="memory-export">Exporter les souvenirs</button></div>`
    + html`<p>Dis-lui « retiens que… », « corrige ce souvenir… » ou « oublie… ». Les autres faits repérés après quelques échanges sont proposés ici pour validation.</p>`
    + html`<label>Rechercher dans les souvenirs<input type="search" id="memory-search" placeholder="Un prénom, un projet, un thème…" /></label>`
    + html`<p id="memory-feedback" role="status"></p>`
    + html`<div id="memory-editor"><button type="button" class="button secondary small" id="memory-toggle" aria-expanded="false" aria-controls="memory-form">Ajouter un souvenir</button><form id="memory-form" hidden><label>Contenu<textarea name="content" required maxlength="1800" rows="3"></textarea></label><div class="connection-selects"><label>Thème<input name="category" required maxlength="60" value="${t('Préférences')}" /></label><label>Niveau<select name="kind"><option value="essential">Essentiel · rappel régulier</option><option value="detail">Tiroir · retrouvé par sujet</option></select></label></div><div class="settings-actions"><button type="submit" class="button primary">Enregistrer le souvenir</button><button type="button" class="button quiet" id="memory-reset">Nouveau souvenir</button></div></form></div>`
    + html`<div id="memory-entries"><p class="muted">Chargement des souvenirs…</p></div><p class="connection-note">Une correction ou un oubli exclut les anciens échanges des futurs contextes et résumés, pour empêcher le retour de l’ancienne information. Les autres souvenirs enregistrés sont conservés. L’historique reste consultable ici.</p>`
    + html`<details id="memory-summary"><summary>Repères de conversation</summary><p></p></details>`
    + html`<details><summary>Rechercher dans les archives</summary><p class="connection-note">Consultation de l’historique complet, y compris les échanges exclus de la mémoire.</p><label>Recherche dans les archives<input type="search" id="archive-search" /></label><div id="archive-results"></div></details></section>`;
}

export function mountMemory(profile: Profile, report: (e: unknown) => void): void {
  const root = document.querySelector<HTMLElement>('#memory-root');
  if (!root) return;
  let state: MemoryState | undefined;
  let editId: number | null = null;
  let request = 0;
  let archiveRequest = 0;
  let saving = false;
  const form = root.querySelector<HTMLFormElement>('#memory-form')!;
  const input = (name: string) => form.elements.namedItem(name) as HTMLInputElement;
  const search = root.querySelector<HTMLInputElement>('#memory-search')!;
  const feedback = (s: string) => { root.querySelector('#memory-feedback')!.textContent = s; };
  function card(m: MemoryEntry): string {
    const button = (action: string, label: string) => '<button class="button quiet small" data-memory-action="' + action + '" data-id="' + m.id + '">' + label + '</button>';
    return '<article class="memory-entry" data-memory-id="' + m.id + html`"><div class="section-heading"><strong>` + escape(m.category) + html`</strong><small>` + date(m.updated_at) + html`</small></div><p>` + escape(m.content) + html`</p><div class="memory-actions">`
      + (!m.approved ? button('approve',t('Retenir ce souvenir')) : '') + button('edit',t('Corriger')) + button('forget',m.approved ? t('Oublier') : t('Écarter'))
      + html`</div><details><summary>Source · ` + (m.origin === 'manual' ? t('saisie dans le panneau') : m.origin === 'explicit' ? t('demande explicite') : t('proposition automatique')) + '</summary>'
      + (m.sources.map(s => html`<blockquote><small>` + date(s.created_at) + html`</small><p>` + escape(s.content) + '</p></blockquote>').join('') || html`<p>Saisi ou corrigé directement dans ce panneau.</p>`) + '</details></article>';
  }
  function paint(value: MemoryState): void {
    if (!root!.isConnected) return;
    state = value;
    const groups = [
      [t('À valider'), value.entries.filter(m => !m.approved)],
      [t('L’essentiel'), value.entries.filter(m => m.approved && m.kind === 'essential')],
      [t('Les tiroirs'), value.entries.filter(m => m.approved && m.kind === 'detail')]
    ] as const;
    root!.querySelector('#memory-entries')!.innerHTML = groups.map(([title, entries]) => html`<section class="memory-group"><h3>` + title + html` <span>` + entries.length + '</span></h3>' + (entries.map(card).join('') || html`<p class="muted">Aucun souvenir ici.</p>`) + '</section>').join('');
    root!.querySelector('#memory-summary p')!.textContent = value.summary || t('Les repères se construiront après quelques échanges, pendant une pause.');
    root!.querySelectorAll<HTMLButtonElement>('[data-memory-action]').forEach(button => button.addEventListener('click', () => {
      const id = Number(button.dataset.id);
      const memory = state?.entries.find(m => m.id === id);
      if (!memory) return;
      if (button.dataset.memoryAction === 'edit') {
        editId = id;
        for (const field of ['content','category','kind'] as const) input(field).value = memory[field];
        form.hidden = false; root!.querySelector('#memory-toggle')!.setAttribute('aria-expanded', 'true');
        root!.querySelector('#memory-toggle')!.textContent = t('Corriger ce souvenir');
        input('content').focus();
      } else void save({action: button.dataset.memoryAction as 'forget' | 'approve', id});
    }));
  }
  async function load(): Promise<void> {
    const version = ++request;
    const result = await api.memories(profile.id, search.value);
    if (version === request && !saving) paint(result);
  }
  function reset(): void { editId = null; form.reset(); root!.querySelector('#memory-toggle')!.textContent = t('Ajouter un souvenir'); }
  async function save(edit: MemoryEdit): Promise<void> {
    if (!state || saving) return;
    saving = true;
    root!.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled = true; });
    try {
      const value = await api.editMemory(profile.id, state.epoch, edit);
      search.value = ''; paint(value); reset();
      feedback(edit.action === 'forget' ? t('Souvenir écarté des prochaines conversations.') : t('Mémoire enregistrée.'));
    } catch (e) { report(e); }
    finally { saving = false; root!.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled = false; }); await load().catch(report); }
  }
  let timer: number | undefined;
  search.addEventListener('input', () => { clearTimeout(timer); timer = window.setTimeout(() => void load().catch(report), 200); });
  root.querySelector('#memory-toggle')!.addEventListener('click', () => { form.hidden = !form.hidden; root.querySelector('#memory-toggle')!.setAttribute('aria-expanded', String(!form.hidden)); if (!form.hidden) input('content').focus(); });
  root.querySelector('#memory-reset')!.addEventListener('click', reset);
  form.addEventListener('submit', e => { e.preventDefault(); void save({action:'write',id:editId,draft:{content:input('content').value,category:input('category').value,kind:input('kind').value as 'essential'|'detail'}}); });
  root.querySelector('#memory-export')!.addEventListener('click', () => void api.exportMemories(profile.id).then(ok => { if (ok) feedback(t('Souvenirs et sources exportés.')); }).catch(report));
  const archive = root.querySelector<HTMLInputElement>('#archive-search')!;
  let archiveTimer: number | undefined;
  archive.addEventListener('input', () => {
    const version = ++archiveRequest;
    clearTimeout(archiveTimer);
    archiveTimer = window.setTimeout(() => void api.searchArchive(profile.id, archive.value).then(rows => {
      if (root.isConnected && version === archiveRequest) root.querySelector('#archive-results')!.innerHTML = rows.map(s => html`<blockquote><small>` + date(s.created_at) + html`</small><p>` + escape(s.content) + '</p></blockquote>').join('') || html`<p class="muted">Aucun échange trouvé.</p>`;
    }).catch(report), 250);
  });
  void api.onMemory(id => { if (id === profile.id && root.isConnected) void load().catch(report); }).then(unlisten => {
    const observer = new MutationObserver(() => { if (!root.isConnected) { unlisten(); observer.disconnect(); } });
    observer.observe(document.body, {childList:true,subtree:true});
    if (!root.isConnected) {unlisten();observer.disconnect();}
  });
  void load().catch(report);
}
