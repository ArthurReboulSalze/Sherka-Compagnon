import { t, html, feedback } from './i18n';
import * as api from './api';
import type { Connections, Profile, VoiceCatalog } from './types';

const escape = (text: string) => text.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!));
const catalogs = new Map<string, Promise<VoiceCatalog>>();

function catalog(url: string, fresh = false): Promise<VoiceCatalog> {
  if (fresh || !catalogs.has(url)) {
    const request = api.discoverVoices(url).catch(error => { catalogs.delete(url); throw error; });
    catalogs.set(url, request);
  }
  return catalogs.get(url)!;
}

export async function paintProfileVoice(label: HTMLElement, profile: Profile): Promise<void> {
  const id = profile.connections.tts_voice;
  if (!id) { label.textContent = t('Aucune voix choisie'); return; }
  label.textContent = id === 'default' ? t('Voix par défaut du moteur') : t`Voix enregistrée · ${id}`;
  try {
    const voices = await catalog(profile.connections.voice_studio_url);
    if (!label.isConnected) return;
    const voice = voices.voices.find(v => v.voice_id === id);
    if (voice) label.textContent = voice.name;
  } catch { /* Keep the saved identifier visible when VoiceStudio is offline. */ }
}

export async function editProfileVoice(profile: Profile, save: (connections: Connections) => Promise<boolean>): Promise<void> {
  const dialog = document.createElement('dialog');
  dialog.className = 'modal voice-modal';
  dialog.setAttribute('aria-labelledby', 'voice-title');
  dialog.innerHTML = html`<h2 id="voice-title">La voix de ${escape(profile.name)}</h2><p>Cette voix sera reprise à chaque activation de ${escape(profile.name)}.</p><div id="voice-options" aria-live="polite">Lecture des voix VoiceStudio…</div><div class="modal-actions"><button class="button secondary" id="voice-cancel">Fermer</button></div>`;
  dialog.addEventListener('close', () => dialog.remove());
  dialog.querySelector('#voice-cancel')!.addEventListener('click', () => dialog.close());
  document.body.append(dialog);
  dialog.showModal();
  try {
    const voices = await catalog(profile.connections.voice_studio_url, true);
    if (!dialog.isConnected) return;
    const engines = voices.tts_engines.filter(e => e.readiness === 'ready');
    const options = dialog.querySelector('#voice-options')!;
    options.innerHTML = html`<form id="profile-voice-form"><label>Moteur de synthèse vocale<select name="engine" required><option value="">À choisir</option>${engines.map(e => html`<option value="${escape(e.id)}" ${profile.connections.tts_engine === e.id ? 'selected' : ''}>${escape(e.label)}</option>`).join('')}</select></label><label>Voix de ${escape(profile.name)}<select name="voice" required></select></label><p class="connection-note" id="voice-note"></p></form>`;
    const engine = options.querySelector<HTMLSelectElement>('[name="engine"]')!;
    const voice = options.querySelector<HTMLSelectElement>('[name="voice"]')!;
    const note = options.querySelector('#voice-note')!;
    const fillVoices = (selected: string | null) => {
      const supports = engines.find(e => e.id === engine.value)?.supports_cloning;
      const available = voices.voices.filter(v => v.type !== 'profile' || supports);
      voice.innerHTML = html`<option value="">À choisir</option>${available.map(v => html`<option value="${escape(v.voice_id)}" ${selected === v.voice_id ? 'selected' : ''}>${escape(v.name)}${v.language ? ` · ${escape(v.language)}` : ''} · ${escape(v.voice_id)}</option>`).join('')}`;
      note.textContent = !engine.value ? t('Choisis le moteur vocal de ce compagnon.') : !supports ? t('Ce moteur ne prend pas en charge les profils vocaux personnalisés de VoiceStudio.') : selected && !available.some(v => v.voice_id === selected) ? t('La voix enregistrée est absente du catalogue : choisis-en une disponible.') : t('Le choix est propre à ce compagnon. Les réglages globaux de VoiceStudio restent libres.');
    };
    fillVoices(profile.connections.tts_voice);
    engine.addEventListener('change', () => fillVoices(voice.value));
    const submit = document.createElement('button');
    submit.className = 'button primary';
    submit.type = 'submit';
    submit.setAttribute('form', 'profile-voice-form');
    submit.textContent = t('Associer cette voix');
    dialog.querySelector('.modal-actions')!.append(submit);
    let saving = false;
    dialog.querySelector('form')!.addEventListener('submit', async event => {
      event.preventDefault();
      if (saving) return;
      saving = true; submit.disabled = true; engine.disabled = true; voice.disabled = true;
      const ok = await save({ ...profile.connections, tts_engine: engine.value, tts_voice: voice.value });
      if (ok) dialog.close();
      saving = false; submit.disabled = false; engine.disabled = false; voice.disabled = false;
    });
  } catch (error) {
    if (dialog.isConnected) dialog.querySelector('#voice-options')!.textContent = feedback(String(error));
  }
}
