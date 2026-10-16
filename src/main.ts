import { version as appVersion } from '../package.json';
import { t, html, feedback, setLanguage, language, languageOptions, formatLocale, type Language } from './i18n';
import './style.css';
import { mountVoiceService, paintVoiceService } from './voice-startup';
import * as api from './api';
import { icon } from './icons';
import type { Profile, ProfileDraft, Snapshot, Mutation, WidgetAction } from './types';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { mountConnections, freshConnections } from './connections';
import { conversationView, mountConversation, loadConversationHistory, paintLive, paintMeter } from './conversation';
import { mountAudioSettings } from './audio-settings';
import { memoryPanel, mountMemory } from './memory';
import { paintProfileVoice, editProfileVoice } from './profile-voice';
import { updatesView, mountUpdates, initializeUpdates } from './updates';
import { bindRangeControls } from './settings-controls';
import { webSettings } from './web';
import { mountStartupRecovery } from './startup-recovery';

const app = document.querySelector<HTMLDivElement>('#app')!;
const widget = new URLSearchParams(location.search).get('view') === 'widget';
document.body.classList.toggle('widget-body', widget);
let state: Snapshot | undefined;
let selected: string | undefined;
let view: 'conversation' | 'profiles' | 'memory' | 'settings' = 'profiles';
let busy = false;
let requestVersion = 0;
let historyVersion = 0;
let widgetControlsPinned = false;
let widgetPointerInside = false;
let widgetKeyboardControls = false;
let dismissWidgetControls: (() => void) | undefined;
let widgetVisible = false;
const notifications = document.createElement('div');
notifications.className = 'notifications';
notifications.setAttribute('aria-live', 'polite');
document.body.append(notifications);

const escape = (text: string) => text.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!));
const portrait = (profile: Profile) => profile.avatar ?? '/assets/sherka.png';
const activeProfile = () => state?.profiles.find((p) => p.id === state?.session.profile_id);
const selectedProfile = () => state?.profiles.find((p) => p.id === selected) ?? activeProfile();
const date = (timestamp: number) => new Intl.DateTimeFormat(formatLocale(), { dateStyle: 'medium' }).format(timestamp);

function toast(message: string, error = false): void {
  const openDialog = document.querySelector<HTMLDialogElement>('dialog[open]');
  if (error && openDialog) {
    let notice = openDialog.querySelector<HTMLParagraphElement>('.form-error');
    if (!notice) {
      notice = document.createElement('p');
      notice.className = 'form-error';
      notice.setAttribute('role', 'alert');
      openDialog.querySelector('.modal-actions')?.before(notice);
    }
    notice.textContent = message;
    return;
  }
  // Keep feedback readable during rapid edits and profile changes.
  if (!error) notifications.querySelectorAll('.toast:not(.error)').forEach((item) => item.remove());
  while (notifications.childElementCount >= 2) notifications.firstElementChild?.remove();
  const element = document.createElement('div');
  element.className = `toast${error ? ' error' : ''}`;
  element.setAttribute('role', error ? 'alert' : 'status');
  element.textContent = message;
  notifications.append(element);
  setTimeout(() => element.remove(), error ? 8500 : 4200);
}

function report(error: unknown): void { toast(feedback(String(error)), true); }

async function refresh(): Promise<void> {
  const version = ++requestVersion;
  try {
    const next = await api.getSnapshot();
    if (version !== requestVersion) return;
    state = next;
    render();
  } catch (error) {
    if (state) report(error);
    else showStartupError(String(error));
  }
}

async function change(mutation: Mutation, success: string, selectNew = false): Promise<boolean> {
  if (busy) return false;
  busy = true;
  document.body.classList.add('working');
  const ids = new Set(state?.profiles.map((p) => p.id));
  try {
    const next = widget && mutation.kind === 'switch_profile' ? await api.switchCompanion(mutation.id) : await api.mutate(mutation);
    ++requestVersion;
    state = next;
    if (selectNew) selected = next.profiles.find((p) => !ids.has(p.id))?.id;
    if (mutation.kind === 'switch_profile') selected = mutation.id;
    render();
    toast(feedback(success));
    return true;
  } catch (error) {
    report(error);
    await refresh();
    return false;
  } finally {
    busy = false;
    document.body.classList.remove('working');
  }
}

async function act(action: WidgetAction): Promise<void> {
  try {
    await api.widgetAction(action);
    if (action === 'click_through') toast(t('Pour reprendre le contrôle, clique sur Sherka dans la zone de notification.'));
  } catch (error) { report(error); }
}

function showStartupError(message: string): void {
  app.setAttribute('aria-busy', 'false');
  app.innerHTML = html`<main class="startup-error"><span class="brand-symbol"><img src="/branding/sherka-logo.png" alt="" /></span><h1>Sherka attend son poste de bord.</h1><p>${escape(feedback(message))}</p><button class="button primary" id="retry">Réessayer</button></main>`;
  document.querySelector('#retry')?.addEventListener('click', () => void refresh());
}

function render(): void {
  if (!state) return;
  setLanguage(state.settings.ui_language);
  app.setAttribute('aria-busy', 'false');
  if (widget) { renderWidget(); paintLive(state); paintVoiceService(); return; }
  const profile = selectedProfile();
  if (!profile) return;
  selected = profile.id;
  app.innerHTML = html`<div class="shell">
    <aside class="sidebar">
      <a class="brand" href="#" aria-label="Sherka, accueil"><span class="brand-symbol"><img src="/branding/sherka-logo.png" alt="" /></span><span>SHERKA<small>COMPAGNON DE BUREAU</small></span></a>
      <div class="nav-caption">POSTE DE BORD</div>
      <nav aria-label="Navigation principale">
        ${navItem('conversation', 'chat', t('Conversation'))}
        ${navItem('profiles', 'profiles', t('Compagnons'))}
        ${navItem('memory', 'memory', t('Mémoire & histoire'))}
        ${navItem('settings', 'settings', t('Réglages'))}
      </nav>
      <div class="sidebar-bottom"><div class="local-note">${icon('shield')}<span>Un espace à toi.<br><strong>Enregistré sur ce PC.</strong></span></div><div class="mic-status">${icon('micOff')}<span>Microphone coupé</span><i></i></div><span class="version">SHERKA ${appVersion} <span>COMPAGNONS</span></span></div>
    </aside>
    <main class="main-panel">
      <header class="topbar"><span class="breadcrumb">TON ESPACE <span>/</span> ${view === 'conversation' ? t('CONVERSATION') : view === 'profiles' ? t('COMPAGNONS') : view === 'memory' ? t('MÉMOIRE') : t('RÉGLAGES')}</span><button class="button quiet small" id="widget-visibility" data-action="toggle_widget" aria-pressed="${widgetVisible}">${widgetVisibilityLabel(widgetVisible)}</button></header>
      <section class="content">${view === 'conversation' ? conversationView(activeProfile()!, state.settings.web_search) : view === 'profiles' ? profilesView(profile) : view === 'memory' ? memoryView(profile) : settingsView()}</section>
      <footer class="panel-footer"><span><i class="status-dot"></i> ${escape(activeProfile()?.name ?? 'Sherka')} est le compagnon actif</span><span>Chaque compagnon garde sa propre histoire.</span></footer>
    </main>
  </div>`;
  bindPanel();
  if (view === 'profiles') void paintProfileVoice(document.querySelector('#profile-voice-label')!, profile);
  if (view === 'conversation') mountConversation(activeProfile()!, report);
  paintLive(state);
  paintVoiceService();
  if (view === 'settings') mountUpdates();
  if (view === 'settings') void mountAudioSettings(document.querySelector('#audio-settings-root')!, state.settings.audio, (audio) => change({ kind: 'audio_settings', audio }, t('Réglages audio enregistrés.')));
  if (view === 'memory') { void loadHistory(profile.id); mountMemory(profile, report); }
  if (view === 'settings') mountConnections(document.querySelector('#connections-root')!, profile, state.profiles,
    (id) => { selected = id; render(); },
    (revision, connections) => change({ kind: 'connections', id: profile.id, revision, connections }, t`Connexions de ${profile.name} enregistrées.`));
}

function widgetVisibilityLabel(visible: boolean): string {
  return visible ? t`${icon('eye')} Masquer le widget` : t`${icon('eye')} Afficher le widget`;
}

function navItem(id: typeof view, symbol: string, title: string): string {
  return html`<button class="nav-item ${view === id ? 'selected' : ''}" data-nav="${id}" ${view === id ? 'aria-current="page"' : ''}>${icon(symbol)}<span>${title}</span>${view === id ? html`<span class="nav-mark"></span>` : ''}</button>`;
}

function profilesView(profile: Profile): string {
  const isActive = profile.id === state!.settings.active_profile;
  return html`<div class="page-heading"><div><p class="eyebrow">UNE PRÉSENCE, À TA FAÇON</p><h1>Tes compagnons<span>.</span></h1><p>Une personnalité. Une voix. Une histoire qui lui appartient.</p></div><button class="button primary" id="new-profile">${icon('plus')} Nouveau compagnon</button></div>
    <div class="profile-gallery" aria-label="Galerie des compagnons">${state!.profiles.map((p) => html`<button class="profile-card ${profile.id === p.id ? 'chosen' : ''}" data-profile="${p.id}" aria-pressed="${profile.id === p.id}"><span class="mini-portrait accent-${p.accent}"><img src="${portrait(p)}" alt="" /></span><span class="card-text"><strong>${escape(p.name)}</strong><small>${p.id === state!.settings.active_profile ? html`<i class="status-dot"></i> Compagnon actif` : t('Espace indépendant')}</small></span>${profile.id === p.id ? html`<span class="card-check">${icon('check')}</span>` : ''}</button>`).join('')}<button class="import-card" id="import-profile">${icon('upload')}<span>Importer<br><small>un profil Sherka</small></span></button></div>
    <article class="companion-detail accent-${profile.accent}">
      <div class="portrait-stage"><div class="stage-label"><span class="tiny-cross">+</span> IDENTITÉ DU COMPAGNON <span class="tiny-cross">+</span></div><div class="orbit-ring ring-one"></div><div class="orbit-ring ring-two"></div><img class="hero-portrait" src="${portrait(profile)}" alt="Apparence de ${escape(profile.name)}" /><div class="portrait-controls"><button class="button glass small" id="change-avatar">${icon('image')} Changer l’image</button>${profile.avatar ? html`<button class="button glass small" id="reset-avatar" title="Revenir à l’image de départ">Réinitialiser</button>` : ''}</div><span class="stage-coordinate">01 / PRÉSENCE</span></div>
      <div class="companion-copy"><div class="identity-line"><span class="tag ${isActive ? 'live' : ''}">${isActive ? html`<i class="status-dot"></i> ACTIF SUR TON BUREAU` : t('PRÊT À ÊTRE ACTIVÉ')}</span><button class="icon-button" id="edit-profile" aria-label="Modifier le compagnon" title="Modifier">${icon('edit')}</button></div><h2>${escape(profile.name)}</h2><p class="personality-preview">${escape(profile.personality || t('Sa personnalité reste à écrire. Donne-lui une façon bien à lui de t’accompagner.'))}</p><div class="profile-stats"><div><strong>${profile.message_count}</strong><span>messages</span></div><div><strong>${profile.memory_count}</strong><span>souvenirs</span></div><div><strong class="text-value">Indépendant</strong><span>son propre espace</span></div></div><div class="detail-actions">${isActive ? html`<button class="button primary" data-action="show_widget">${icon('eye')} Retrouver ${escape(profile.name)}</button>` : html`<button class="button primary" id="activate-profile">${icon('spark')} Activer ce compagnon</button>`}<button class="button secondary" id="personality-button">Personnalité ${icon('arrow')}</button></div><div class="profile-tools"><button class="text-button" id="duplicate-profile">${icon('copy')} Dupliquer</button><button class="text-button" id="export-profile">${icon('download')} Exporter</button><button class="text-button danger" id="delete-profile" ${isActive ? html`disabled title="Active un autre compagnon avant de supprimer celui-ci."` : ''}>${icon('trash')} Supprimer</button></div></div>
    </article>
    <div class="bottom-grid"><section class="info-card"><div class="card-icon">${icon('shield')}</div><div><h3>Son histoire reste la sienne.</h3><p>Changer de compagnon change de mémoire. Une copie reprend sa personnalité et son apparence, avec une histoire vierge.</p></div></section><section class="info-card voice-card"><div class="card-icon">${icon('micOff')}</div><div><h3>La voix de ${escape(profile.name)}</h3><p id="profile-voice-label">Lecture de sa voix…</p><button class="text-button" id="choose-profile-voice">Choisir sa voix →</button></div></section></div>`;
}

function memoryView(profile: Profile): string {
  return html`<div class="page-heading"><div><p class="eyebrow">UNE HISTOIRE QUI LUI APPARTIENT</p><h1>Mémoire & histoire<span>.</span></h1><p>Les échanges de chaque compagnon restent dans son espace.</p></div></div>
    <label class="profile-picker">Compagnon <select id="history-profile">${state!.profiles.map((p) => html`<option value="${p.id}" ${profile.id === p.id ? 'selected' : ''}>${escape(p.name)}</option>`).join('')}</select></label>
    ${memoryPanel(profile)}
    <section class="history-section"><div class="section-heading"><h2>Historique de ${escape(profile.name)}</h2><span>Créé le ${date(profile.created_at)}</span></div><div id="history" aria-live="polite"><p class="muted">Chargement…</p></div></section>`;
}

function interfaceLanguageView(): string {
  return html`<section class="settings-card interface-language-card"><h2>Langue de l’interface</h2><label>Langue de l’interface<select id="interface-language">${languageOptions(language())}</select></label><p>Ce choix traduit l’interface et adapte Sherka d’origine à cette langue. Les compagnons personnalisés, les conversations, les souvenirs, les modèles et les voix restent inchangés.</p></section>`;
}

function settingsView(): string {
  return html`<div class="page-heading"><div><p class="eyebrow">DISCRET, COMME TU L’AIMES</p><h1>À tes commandes<span>.</span></h1><p>La présence de Sherka sur ton bureau.</p></div></div>
    <section class="settings-card"><div class="section-heading"><h2>Le widget</h2>${icon('orbit')}</div><form id="preferences-form"><label class="setting-row"><span><strong>Toujours au premier plan</strong><small>Garder ton compagnon visible au-dessus des autres fenêtres.</small></span><input class="switch" type="checkbox" name="always_on_top" ${state!.settings.always_on_top ? 'checked' : ''} /></label><label class="setting-row"><span><strong>Taille du compagnon</strong><small>Tu peux aussi tirer le coin inférieur droit du widget.</small></span><span class="scale-control range-control"><input id="widget-scale" name="widget_scale" type="range" min="65" max="160" step="5" value="${state!.settings.widget_scale}" /><output id="scale-value" for="widget-scale" data-percent-scale="0.01">${state!.settings.widget_scale} %</output></span></label><div class="settings-actions"><button class="button primary" type="submit">${icon('check')} Enregistrer</button><button class="button secondary" type="button" data-action="restore_widget">Replacer le widget</button></div></form></section>
    <section class="settings-card"><h2>Passage des clics</h2><p>Le widget peut laisser passer les clics vers ce qui se trouve derrière lui. Pour reprendre le contrôle, clique sur l’icône Sherka dans la zone de notification Windows.</p><div class="settings-actions"><button class="button secondary" data-action="click_through">Laisser passer les clics</button><button class="text-button" data-action="restore_widget">Reprendre les clics</button></div></section>
    ${interfaceLanguageView()}${webSettings(state!.settings.web_search)}${updatesView()}<div id="connections-root"></div><div id="audio-settings-root"></div>
    <section class="settings-card"><h2>Quitter Sherka</h2><p>Arrête le micro et la réponse, puis décharge le modèle du compagnon dans LM Studio.</p><button class="button secondary" data-action="quit">Quitter Sherka</button></section>
    <section class="storage-card">${icon('shield')}<div><strong>Données conservées sur ce PC</strong><p class="storage-path">${escape(state!.data_directory)}</p><small>Un dossier et une base par compagnon. Aucun enregistrement audio.</small></div></section>`;
}

function bindPanel(): void {
  document.querySelector<HTMLSelectElement>('#web-search-mode')?.addEventListener('change', event => {
    void change({kind:'web_search', mode:(event.target as HTMLSelectElement).value as Snapshot['settings']['web_search']}, t('Réglage de recherche Internet enregistré.'));
  });
  document.querySelector<HTMLSelectElement>('#interface-language')?.addEventListener('change', event => {
    const selectedLanguage = (event.target as HTMLSelectElement).value as Language;
    void change({kind:'interface_language',language:selectedLanguage}, 'Langue de l’interface enregistrée.');
  });
  document.querySelector('.brand')?.addEventListener('click', (event) => { event.preventDefault(); view = 'profiles'; render(); });
  document.querySelectorAll<HTMLButtonElement>('[data-nav]').forEach((button) => button.addEventListener('click', () => { view = button.dataset.nav as typeof view; render(); }));
  document.querySelectorAll<HTMLButtonElement>('button[data-profile]').forEach((button) => button.addEventListener('click', () => { selected = button.dataset.profile; render(); }));
  bindWindowActions();
  const profile = selectedProfile()!;
  document.querySelector('#choose-profile-voice')?.addEventListener('click', () => void editProfileVoice(profile, connections => change({ kind: 'connections', id: profile.id, revision: profile.revision, connections }, t`Voix de ${profile.name} enregistrée.`)));
  document.querySelector('#new-profile')?.addEventListener('click', () => editProfile());
  document.querySelector('#edit-profile')?.addEventListener('click', () => editProfile(profile));
  document.querySelector('#personality-button')?.addEventListener('click', () => editProfile(profile));
  document.querySelector('#memory-personality')?.addEventListener('click', () => editProfile(profile));
  document.querySelector('#activate-profile')?.addEventListener('click', () => void change({ kind: 'switch_profile', id: profile.id }, t`${profile.name} est maintenant actif.`));
  document.querySelector('#duplicate-profile')?.addEventListener('click', () => void change({ kind: 'duplicate_profile', id: profile.id }, t('Compagnon dupliqué avec une histoire vierge.'), true));
  document.querySelector('#delete-profile')?.addEventListener('click', () => confirmDelete(profile));
  document.querySelector('#change-avatar')?.addEventListener('click', () => chooseFile('image/png,image/webp', async (file) => {
    if (file.size > 10 * 1024 * 1024) throw new Error(t('Choisis une image de moins de 10 Mio.'));
    const bytes = new Uint8Array(await file.arrayBuffer());
    let binary = '';
    for (let i = 0; i < bytes.length; i += 8192) binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
    await change({ kind: 'set_avatar', id: profile.id, revision: profile.revision, base64: btoa(binary) }, t('Nouvelle apparence enregistrée.'));
  }));
  document.querySelector('#reset-avatar')?.addEventListener('click', () => void change({ kind: 'set_avatar', id: profile.id, revision: profile.revision, base64: null }, t('Apparence de départ rétablie.')));
  document.querySelector('#import-profile')?.addEventListener('click', () => chooseFile('.json', async (file) => {
    if (file.size > 15 * 1024 * 1024) throw new Error(t('Ce fichier dépasse la limite de 15 Mio.'));
    await change({ kind: 'import_profile', json: await file.text() }, t('Compagnon importé dans un nouvel espace.'), true);
  }));
  document.querySelector('#export-profile')?.addEventListener('click', async () => {
    try { if (await api.exportProfile(profile.id)) toast(t('Profil exporté sans historique, souvenirs ni clés.')); } catch (error) { report(error); }
  });
  document.querySelector<HTMLSelectElement>('#history-profile')?.addEventListener('change', (event) => { selected = (event.target as HTMLSelectElement).value; render(); });
  document.querySelector<HTMLFormElement>('#preferences-form')?.addEventListener('submit', (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget as HTMLFormElement);
    void change({ kind: 'preferences', preferences: { always_on_top: form.has('always_on_top'), widget_scale: Number(form.get('widget_scale')) } }, t('Préférences enregistrées.'));
  });
  bindRangeControls(document);
}

function bindWindowActions(): void {
  document.querySelectorAll<HTMLButtonElement>('[data-action]').forEach((button) => button.addEventListener('click', async () => {
    if (widget && ['show_panel', 'hide_widget'].includes(button.dataset.action!)) dismissWidgetControls?.();
    button.disabled = true;
    try { await act(button.dataset.action as WidgetAction); }
    finally { button.disabled = false; }
  }));
}

function chooseFile(accept: string, callback: (file: File) => Promise<void>): void {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = accept;
  input.addEventListener('change', () => {
    const file = input.files?.[0];
    if (file) void callback(file).catch(report);
  });
  input.click();
}

function dialog(content: string): HTMLDialogElement {
  const element = document.createElement('dialog');
  element.className = 'modal';
  element.setAttribute('aria-labelledby', 'dialog-title');
  element.innerHTML = content;
  element.addEventListener('close', () => element.remove());
  element.querySelectorAll('[data-close]').forEach((button) => button.addEventListener('click', () => element.close()));
  document.body.append(element);
  element.showModal();
  return element;
}

function companionLanguageView(profile?: Profile): string {
  const value = profile?.connections.language ?? language();
  const hint = profile?.id === state!.settings.default_companion
    ? t('Sherka d’origine suit la langue de l’interface. Si tu modifies son nom ou son caractère, elle devient un compagnon personnalisé avec une langue fixe.')
    : t('La langue est choisie à la création et reste fixe. Écris sa personnalité dans cette langue. Choisis ensuite un modèle et une voix compatibles dans les réglages.');
  return html`<label>Langue du compagnon<select name="companion_language" ${profile ? 'disabled' : ''}>${languageOptions(value)}</select></label><p class="modal-hint">${hint}</p>`;
}

function editProfile(profile?: Profile): void {
  const element = dialog(html`<form id="profile-form"><div class="modal-header"><div><p class="eyebrow">UN COMPAGNON UNIQUE</p><h2 id="dialog-title">${profile ? t('Sa personnalité') : t('Fais-lui une place.')}</h2></div><button class="icon-button" type="button" data-close aria-label="Fermer">${icon('close')}</button></div>${companionLanguageView(profile)}<label>Nom du compagnon<input name="name" required maxlength="60" placeholder="Par exemple, Vega" value="${escape(profile?.name ?? '')}" autofocus /></label><label>Personnalité<textarea aria-label="Personnalité" name="personality" rows="5" maxlength="12000" placeholder="Son caractère, sa façon de parler, ce qui le rend unique…">${escape(profile?.personality ?? '')}</textarea><small>Le ton et l’attitude qu’il adoptera dans vos échanges.</small></label><details ${profile?.instructions ? 'open' : ''}><summary>Instructions complémentaires</summary><label class="sr-only" for="instructions">Instructions complémentaires</label><textarea id="instructions" name="instructions" rows="3" maxlength="12000" placeholder="Tes préférences pour ses réponses…">${escape(profile?.instructions ?? '')}</textarea></details><fieldset class="accent-picker"><legend>Couleur de l’espace</legend>${(['amber', 'teal', 'violet', 'coral'] as const).map((accent, i) => html`<label class="accent-choice accent-${accent}"><input type="radio" name="accent" value="${accent}" ${(profile?.accent ?? 'amber') === accent ? 'checked' : ''} /><span></span>${[t('Ambre'), t('Sauge'), t('Iris'), t('Corail')][i]}</label>`).join('')}</fieldset><p class="modal-hint">${icon('shield')}${profile ? t('Modifier sa personnalité conserve son histoire et ses souvenirs.') : t('Son historique et sa mémoire commencent dans un espace indépendant.')}</p><div class="modal-actions"><button class="button quiet" type="button" data-close>Annuler</button><button class="button primary" type="submit">${icon(profile ? 'check' : 'plus')}${profile ? t('Enregistrer') : t('Créer le compagnon')}</button></div></form>`);
  element.querySelector('form')!.addEventListener('submit', async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget as HTMLFormElement);
    const draft: ProfileDraft = { name: String(form.get('name')), personality: String(form.get('personality')), instructions: String(form.get('instructions')), accent: String(form.get('accent')) as ProfileDraft['accent'], connections: profile?.connections ?? {...freshConnections(), language: String(form.get('companion_language'))} };
    const button = element.querySelector<HTMLButtonElement>('[type="submit"]')!;
    button.disabled = true;
    const ok = await change(profile ? { kind: 'update_profile', id: profile.id, revision: profile.revision, draft } : { kind: 'create_profile', draft }, profile ? t('Personnalité enregistrée.') : t('Ton nouveau compagnon est prêt.'), !profile);
    if (ok) element.close();
    else button.disabled = false;
  });
}

function confirmDelete(profile: Profile): void {
  const element = dialog(html`<div class="modal-header"><h2 id="dialog-title">Supprimer ${escape(profile.name)} ?</h2><button class="icon-button" data-close aria-label="Fermer">${icon('close')}</button></div><p>Son profil, son image, son historique et ses souvenirs seront supprimés de ce PC. Cette action ne peut pas être annulée.</p><label>Écris son nom pour confirmer<input id="confirm-name" autocomplete="off" /></label><div class="modal-actions"><button class="button quiet" data-close>Garder ce compagnon</button><button class="button destructive" id="confirm-delete" disabled>Supprimer définitivement</button></div>`);
  const confirm = element.querySelector<HTMLButtonElement>('#confirm-delete')!;
  element.querySelector<HTMLInputElement>('#confirm-name')!.addEventListener('input', (event) => { confirm.disabled = (event.target as HTMLInputElement).value !== profile.name; });
  confirm.addEventListener('click', async () => {
    confirm.disabled = true;
    if (await change({ kind: 'delete_profile', id: profile.id }, t('Compagnon supprimé.'))) element.close();
    else confirm.disabled = false;
  });
}

async function loadHistory(id: string): Promise<void> {
  const version = ++historyVersion;
  try {
    const messages = await api.history(id);
    if (version !== historyVersion || view !== 'memory' || selected !== id) return;
    const container = document.querySelector('#history');
    if (!container) return;
    container.innerHTML = messages.length ? messages.map((message) => html`<article class="message"><div><strong>${message.role === 'user' ? t('Toi') : escape(selectedProfile()!.name)}</strong><small>${date(message.created_at)}${['interrupted', 'uncertain'].includes(message.delivery) ? t(' · diffusion incomplète') : ''}</small></div><p>${escape(message.content)}</p></article>`).join('') : html`<div class="empty-state">${icon('chat')}<h3>La première page reste à écrire.</h3><p>Commence un échange dans l’onglet Conversation ou active le micro du compagnon.</p></div>`;
  } catch (error) { report(error); }
}

function renderWidget(): void {
  const profile = activeProfile();
  if (!profile) return;
  app.innerHTML = html`<main class="widget accent-${profile.accent}${widgetPointerInside || widgetControlsPinned || widgetKeyboardControls ? ' controls-visible' : ''}"><div class="widget-name widget-chrome"><i class="status-dot"></i><span>${escape(profile.name)}</span></div><button class="widget-portrait" id="drag-region" aria-label="Afficher ou masquer les commandes de ${escape(profile.name)}" aria-pressed="${widgetControlsPinned}" title="Survole ou clique pour les commandes, fais glisser pour déplacer"><img draggable="false" src="${portrait(profile)}" alt="${escape(profile.name)}" /></button><div class="widget-status widget-chrome">${icon('micOff')}<span data-session-status>Microphone coupé</span></div><div class="widget-toolbar widget-chrome"><button class="icon-button" id="choose-companion" aria-label="Changer de compagnon" title="Changer de compagnon" aria-expanded="false" aria-controls="widget-companions">${icon('profiles')}</button><button class="icon-button" data-action="show_panel" aria-label="Ouvrir le poste de bord" title="Poste de bord">${icon('settings')}</button><button class="icon-button" data-mic-toggle data-action="toggle_microphone" aria-label="Activer le micro" title="Activer le micro">${icon('micOff')}</button><button class="icon-button" data-action="stop_response" aria-label="Arrêter la réponse" title="Arrêter la réponse">${icon('stop')}</button>${profile.connections.llm_provider === 'hermes' ? html`<button class="icon-button" id="hermes-new" aria-label="Nouvelle conversation Hermes" title="Nouvelle conversation Hermes">${icon('plus')}</button>` : ''}<button class="icon-button" data-action="hide_widget" aria-label="Masquer le compagnon" title="Masquer (accessible dans la zone de notification)">${icon('close')}</button></div><div class="widget-companions" id="widget-companions" hidden><span>Ton compagnon</span><div role="group" aria-label="Choisir un compagnon">${state!.profiles.map(p => html`<button type="button" data-companion="${p.id}" aria-pressed="${p.id === profile.id}"><img src="${portrait(p)}" alt="" /><span>${escape(p.name)}</span>${p.id === profile.id ? icon('check') : ''}</button>`).join('')}</div></div><button class="resize-handle widget-chrome" id="resize-widget" aria-label="Redimensionner le widget" title="Redimensionner">${icon('resize')}</button></main>`;
  bindWindowActions();
  document.querySelector<HTMLButtonElement>('#hermes-new')?.addEventListener('click', async event => {
    const button = event.currentTarget as HTMLButtonElement; button.disabled = true;
    try { await api.newHermesConversation(profile.id); await refresh(); } catch(error) { report(error); button.disabled = false; }
  });
  const surface = document.querySelector<HTMLElement>('.widget')!;
  const portraitButton = document.querySelector<HTMLButtonElement>('#drag-region')!;
  const picker = document.querySelector<HTMLElement>('#widget-companions')!;
  const pickerButton = document.querySelector<HTMLButtonElement>('#choose-companion')!;
  const closePicker = () => { picker.hidden = true; pickerButton.setAttribute('aria-expanded', 'false'); };
  const updateControls = () => {
    surface.classList.toggle('controls-visible', widgetPointerInside || widgetControlsPinned || widgetKeyboardControls);
    portraitButton.setAttribute('aria-pressed', String(widgetControlsPinned));
  };
  dismissWidgetControls = () => {
    widgetControlsPinned = widgetPointerInside = widgetKeyboardControls = false;
    closePicker();
    if (surface.contains(document.activeElement)) (document.activeElement as HTMLElement)?.blur();
    updateControls();
  };
  // Native dragging and window changes can leave CSS :hover / :focus-visible stale.
  // Show controls only while a current pointer or keyboard interaction owns them.
  for (const event of ['pointerenter', 'pointermove']) surface.addEventListener(event, () => {
    if (!widgetPointerInside) { widgetPointerInside = true; updateControls(); }
  });
  surface.addEventListener('pointerleave', () => dismissWidgetControls?.());
  surface.addEventListener('focusin', event => {
    if ((event.target as Element).matches(':focus-visible')) { widgetKeyboardControls = true; updateControls(); }
  });
  surface.addEventListener('focusout', event => {
    if (surface.contains(event.relatedTarget as Node | null)) return;
    // Let the browser finish a focus transfer before closing the picker.
    setTimeout(() => {
      if (!surface.isConnected || surface.contains(document.activeElement)) return;
      widgetControlsPinned = widgetKeyboardControls = false;
      closePicker();
      updateControls();
    }, 0);
  });
  updateControls();
  pickerButton.addEventListener('click', () => {
    picker.hidden = !picker.hidden;
    pickerButton.setAttribute('aria-expanded', String(!picker.hidden));
    if (!picker.hidden) picker.querySelector<HTMLButtonElement>('[aria-pressed="true"]')?.focus();
  });
  surface.addEventListener('pointerdown', event => {
    if (!(event.target as Element).closest('#widget-companions, #choose-companion')) closePicker();
  });
  picker.querySelectorAll<HTMLButtonElement>('[data-companion]').forEach(button => button.addEventListener('click', async () => {
    const id = button.dataset.companion!;
    if (id === profile.id) { closePicker(); pickerButton.focus(); return; }
    picker.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled = true; });
    await change({ kind: 'switch_profile', id }, t`${button.querySelector('span')!.textContent} est maintenant actif.`);
  }));
  let press: { x: number; y: number } | undefined;
  let dragged = false;
  portraitButton.addEventListener('pointerdown', (event) => {
    if (event.button !== 0) return;
    press = { x: event.screenX, y: event.screenY };
    dragged = false;
    portraitButton.setPointerCapture(event.pointerId);
  });
  portraitButton.addEventListener('pointermove', (event) => {
    if (!press || Math.hypot(event.screenX - press.x, event.screenY - press.y) < 5) return;
    press = undefined;
    dragged = true;
    event.stopPropagation();
    portraitButton.releasePointerCapture(event.pointerId);
    dismissWidgetControls?.();
    void getCurrentWindow().startDragging().catch(report);
  });
  for (const event of ['pointerup', 'pointercancel']) portraitButton.addEventListener(event, () => { press = undefined; });
  portraitButton.addEventListener('click', () => {
    if (dragged) return;
    widgetControlsPinned = !widgetControlsPinned;
    updateControls();
  });
  surface.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape') return;
    if (!picker.hidden) { closePicker(); pickerButton.focus(); return; }
    dismissWidgetControls?.();
  });
  document.querySelector('#resize-widget')?.addEventListener('pointerdown', (event) => {
    if ((event as PointerEvent).button === 0) {
      dismissWidgetControls?.();
      void getCurrentWindow().startResizeDragging('SouthEast').catch(report);
    }
  });
}

async function start(): Promise<void> {
  if (!api.desktopAvailable()) {
    showStartupError(t('Ouvre l’application de bureau avec « npm run desktop ». Cette page seule ne peut pas accéder à tes compagnons.'));
    return;
  }
  try {
    if (new URLSearchParams(location.search).get('view') === 'recovery') {
      await mountStartupRecovery(app);
      return;
    }
    await api.onChanged(() => void refresh());
    if (widget) {
      window.addEventListener('blur', () => dismissWidgetControls?.());
      document.addEventListener('visibilitychange', () => { if (document.hidden) dismissWidgetControls?.(); });
      await getCurrentWindow().onFocusChanged(({ payload }) => { if (!payload) dismissWidgetControls?.(); });
    }
    await mountVoiceService(() => { if (state) paintLive(state); });
    if (!widget) await initializeUpdates();
    if (!widget) {
      let visibilityEvents = 0;
      const paintVisibility = (visible: boolean) => {
        widgetVisible = visible;
        const button = document.querySelector<HTMLButtonElement>('#widget-visibility');
        if (button) { button.innerHTML = widgetVisibilityLabel(visible); button.setAttribute('aria-pressed', String(visible)); }
      };
      await api.onWidgetVisibility(visible => { ++visibilityEvents; paintVisibility(visible); });
      const before = visibilityEvents;
      const visible = await api.widgetVisibility();
      if (before === visibilityEvents) paintVisibility(visible);
    }
    await api.onQuit(value => {
      document.querySelector('#quit-status')?.remove();
      document.body.classList.toggle('quitting', value.stage === 'unloading');
      if (value.stage === 'unloading') {
        const status = document.createElement('div');
        status.id = 'quit-status'; status.className = 'quit-status'; status.setAttribute('role', 'status');
        status.textContent = t('Fermeture · déchargement du modèle dans LM Studio…'); document.body.append(status);
      } else if (!widget) {
        const popup = dialog(html`<h2 id="dialog-title">Le modèle n’a pas pu être déchargé</h2><p>${escape(feedback(value.error ?? t('LM Studio est indisponible.')))}</p><p>Tu peux réessayer, rester dans Sherka, ou quitter en laissant le modèle dans LM Studio.</p><div class="modal-actions"><button class="button secondary" id="quit-stay">Rester</button><button class="button secondary" id="quit-anyway">Quitter quand même</button><button class="button primary" id="quit-retry">Réessayer</button></div>`);
        popup.querySelector('#quit-stay')!.addEventListener('click', () => popup.close());
        popup.querySelector('#quit-anyway')!.addEventListener('click', () => { popup.close(); void act('quit_without_unloading'); });
        popup.querySelector('#quit-retry')!.addEventListener('click', () => { popup.close(); void act('quit'); });
      }
    });
    await api.onMemory(() => {
      // Update counters without replacing a memory form or a conversation draft.
      void api.getSnapshot().then(value => {
        if (state) state.profiles = value.profiles;
        if (view === 'profiles' && !widget && !document.querySelector('dialog[open]')) render();
      }).catch(report);
    });
    await api.onLive((value) => {
      if (!state || value.session.profile_id !== state.settings.active_profile) return;
      const newError = value.live.error && value.live.error !== state.live.error;
      state.session = value.session; state.live = value.live;
      if (!widget && value.live.hermes_approval && (view !== 'conversation' || selected !== value.session.profile_id)) {
        view = 'conversation'; selected = value.session.profile_id; render();
      }
      paintLive(value); paintVoiceService();
      if (newError && (widget || view !== 'conversation')) report(value.live.error);
    });
    await api.onHistory((id) => { void loadConversationHistory(id).catch(report); if (view === 'memory' && selected === id) void loadHistory(id); });
    await api.onMeter((value) => { if (state?.session.microphone === 'listening') paintMeter(value); });
  } catch (error) { report(error); }
  await refresh();
}

void start();
