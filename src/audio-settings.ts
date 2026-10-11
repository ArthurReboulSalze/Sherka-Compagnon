import { t, html, feedback } from './i18n';
import * as api from './api';
import type { AudioSettings, AudioDevices } from './types';
const escape = (s: string) => s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!));

export async function mountAudioSettings(root: HTMLElement, settings: AudioSettings, save: (audio: AudioSettings) => Promise<boolean>): Promise<void> {
  root.innerHTML = html`<section class="settings-card"><h2>Microphone & écoute</h2><p>Lecture des périphériques Windows…</p></section>`;
  let devices: AudioDevices;
  try { devices = await api.audioDevices(); }
  catch (error) { if (root.isConnected) root.innerHTML = html`<section class="settings-card"><h2>Microphone & écoute</h2><p class="connection-error">${escape(feedback(String(error)))}</p></section>`; return; }
  if (!root.isConnected) return;
  const options = (items: AudioDevices['inputs'], selected: string | null) => html`<option value="">Périphérique Windows par défaut</option>${selected && !items.some((d) => d.id === selected) ? html`<option selected value="${escape(selected)}">Périphérique enregistré indisponible</option>` : ''}${items.map((d) => html`<option value="${escape(d.id)}" ${d.id === selected ? 'selected' : ''}>${escape(d.name)}${d.is_default ? t(' · par défaut') : ''}</option>`).join('')}`;
  root.innerHTML = html`<section class="settings-card"><h2>Microphone & écoute</h2><p>Le micro reste coupé au lancement. Une fois activé, il continue à t’écouter pendant les réponses.</p><form id="audio-form"><div class="connection-selects audio-device-grid"><label>Microphone<select name="input_device">${options(devices.inputs, settings.input_device)}</select></label><label>Sortie sonore<select name="output_device">${options(devices.outputs, settings.output_device)}</select></label><label>Tu écoutes avec<select name="speakers"><option value="true" ${settings.speakers ? 'selected' : ''}>Des haut-parleurs · réduction d’écho</option><option value="false" ${!settings.speakers ? 'selected' : ''}>Un casque</option></select></label><label>Silence avant la réponse (ms)<input type="number" min="300" max="2000" step="50" name="silence_ms" value="${settings.silence_ms}" /></label></div><details><summary>Détection de la parole</summary><div class="connection-selects"><label>Confirmation d’une phrase (ms)<input type="number" name="onset_ms" min="50" max="300" step="10" value="${settings.onset_ms}" /></label><label>Confirmation d’une interruption (ms)<input type="number" name="interruption_ms" min="50" max="300" step="10" value="${settings.interruption_ms}" /></label></div></details><p class="muted">Changer ces réglages coupe le micro. Réactive-le ensuite pour reprendre.</p><div class="settings-actions"><button class="button primary" type="submit">Enregistrer l’audio</button><button class="button secondary" type="button" id="refresh-audio">Actualiser les périphériques</button></div></form></section>`;
  root.querySelector('#refresh-audio')!.addEventListener('click', () => { void mountAudioSettings(root, settings, save); });
  root.querySelector('form')!.addEventListener('submit', async (event) => {
    event.preventDefault(); const form = new FormData(event.currentTarget as HTMLFormElement);
    const button = root.querySelector<HTMLButtonElement>('[type="submit"]')!; button.disabled = true;
    await save({ input_device: String(form.get('input_device')) || null, output_device: String(form.get('output_device')) || null, speakers: form.get('speakers') === 'true', silence_ms: Number(form.get('silence_ms')), onset_ms: Number(form.get('onset_ms')), interruption_ms: Number(form.get('interruption_ms')) });
    button.disabled = false;
  });
}
