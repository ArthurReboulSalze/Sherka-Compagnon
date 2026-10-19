import { html, t, feedback } from './i18n';
import { icon } from './icons';
import { editProfileVoice, paintProfileVoice } from './profile-voice';
import type { Emotion, Mutation, Profile, Snapshot } from './types';

const escape = (text: string) => text.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!));
const states = (): {id: Emotion; emoji: string; name: string; description: string}[] => [
  {id:'neutral',emoji:'😐',name:t('Neutre'),description:t('Réponse normale, information, discussion courante.')},
  {id:'joy',emoji:'😊',name:t('Joie'),description:t('Bonheur, enthousiasme, amusement, satisfaction.')},
  {id:'sadness',emoji:'😢',name:t('Tristesse'),description:t('Peine, mélancolie, empathie.')},
  {id:'anger',emoji:'😠',name:t('Colère'),description:t('Agacement, indignation, frustration.')},
  {id:'surprise',emoji:'😮',name:t('Surprise'),description:t('Étonnement, découverte inattendue.')},
  {id:'reflection',emoji:'🤔',name:t('Réflexion'),description:t('Doute, curiosité, hésitation, perplexité.')},
  {id:'tenderness',emoji:'🥰',name:t('Tendresse'),description:t('Amour, affection.')},
];

export function portrait(profile: Profile, emotion: Emotion = 'neutral'): string {
  if (profile.connections.llm_provider !== 'hermes' && emotion !== 'neutral' && profile.emotion_avatars?.[emotion]) return profile.emotion_avatars[emotion]!;
  const folder = profile.avatar_preset === 'candis' ? 'alba' : profile.avatar_preset === 'minerva' ? 'minerva-v2' : profile.avatar_preset;
  return profile.avatar ?? (folder && folder !== 'sherka' ? `/assets/${folder}/neutral.png` : '/assets/sherka.png');
}

const fades = new WeakMap<HTMLImageElement, { requested: string; running: boolean }>();

function fadePortrait(image: HTMLImageElement, src: string): void {
  let fade = fades.get(image);
  if (!fade) { fade = { requested: src, running: false }; fades.set(image, fade); }
  fade.requested = src;
  if (fade.running || image.getAttribute('src') === src) return;
  fade.running = true;
  const pending = fade;
  void (async () => {
    try {
      while (image.isConnected && image.getAttribute('src') !== pending.requested) {
        const next = pending.requested;
        const loaded = new Image();
        loaded.src = next;
        try { await loaded.decode(); }
        catch { if (next !== pending.requested) continue; return; }
        // A cancelled turn or newer expression wins over a slow image load.
        if (next !== pending.requested) continue;
        if (!image.isConnected) return;
        const outgoing = image.cloneNode(false) as HTMLImageElement;
        outgoing.className = 'avatar-outgoing';
        outgoing.alt = '';
        outgoing.setAttribute('aria-hidden', 'true');
        image.before(outgoing);
        image.src = next;
        const timing = {duration: matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 500, easing: 'ease-in-out'};
        const arriving = image.animate([{opacity: 0}, {opacity: 1}], timing);
        const leaving = outgoing.animate([{opacity: 1}, {opacity: 0}], {...timing, fill: 'forwards'});
        try { await Promise.allSettled([arriving.finished, leaving.finished]); }
        finally { outgoing.remove(); arriving.cancel(); leaving.cancel(); }
        // Keep at most two layers; a rapid change queues only the latest state.
      }
    } finally { pending.running = false; }
  })();
}

export function paintAvatar(state: Snapshot): void {
  const profile = state.profiles.find(p => p.id === state.session.profile_id);
  const image = document.querySelector<HTMLImageElement>('.widget-portrait > img:not(.avatar-outgoing)');
  if (!profile || !image) return;
  const emotion = state.companion_capabilities.local_emotions ? profile.mood_persistence ? state.live.mood : state.live.tag ? state.live.emotion : 'neutral' : 'neutral';
  const src = portrait(profile, emotion);
  fadePortrait(image, src);
  image.dataset.emotion = emotion;
}

export function editAvatar(initial: Profile, current: () => Profile | undefined, save: (mutation: Mutation, message: string) => Promise<boolean>): void {
  let profile = initial;
  let selected: Emotion = 'neutral';
  let saving = false;
  const dialog = document.createElement('dialog');
  dialog.className = 'modal avatar-modal';
  dialog.setAttribute('aria-labelledby', 'avatar-title');
  dialog.innerHTML = html`<div class="modal-header"><div><p class="eyebrow">${escape(profile.name)}</p><h2 id="avatar-title">Avatar et voix</h2></div><button class="icon-button" data-close aria-label="Fermer">${icon('close')}</button></div>
    <p>Une image par émotion, une voix pour cet avatar.</p>
    <div class="avatar-layout"><div class="avatar-preview"><img alt="Aperçu de l’avatar" /><span id="avatar-image-status"></span></div><div class="avatar-editor"><div class="emotion-grid" role="group" aria-label="Émotions de l’avatar"></div><h3 id="emotion-title"></h3><p id="emotion-description"></p><div class="avatar-file-actions"><button class="button primary" id="avatar-pick">${icon('image')} Choisir une image</button><button class="button secondary" id="avatar-reset">Réinitialiser</button></div><input id="avatar-file" type="file" accept="image/png,image/webp" hidden /><p class="connection-note">PNG ou WebP, 10 Mio maximum. Sans image dédiée, l’avatar utilise son image neutre.</p></div></div>
    <p class="connection-note" id="avatar-hermes" hidden>Hermes utilise uniquement l’image neutre. Les autres images restent enregistrées.</p>
    <section class="avatar-voice"><div><h3>Voix VoiceStudio</h3><p id="avatar-voice-label"></p></div><button class="button secondary" id="avatar-voice-edit">Choisir sa voix →</button></section>
    <p class="form-error" role="alert" hidden></p><p id="avatar-save-status" role="status"></p><div class="modal-actions"><button class="button secondary" data-close>Fermer</button></div>`;
  const file = dialog.querySelector<HTMLInputElement>('#avatar-file')!;
  const grid = dialog.querySelector<HTMLElement>('.emotion-grid')!;
  const error = dialog.querySelector<HTMLElement>('[role="alert"]')!;
  const status = dialog.querySelector<HTMLElement>('#avatar-save-status')!;
  const paint = () => {
    const agent = profile.connections.llm_provider === 'hermes';
    if (agent) selected = 'neutral';
    const state = states().find(s => s.id === selected)!;
    const assigned = selected === 'neutral' ? profile.avatar : profile.emotion_avatars?.[selected];
    dialog.querySelector<HTMLImageElement>('.avatar-preview img')!.src = portrait(profile, selected);
    dialog.querySelector('#avatar-image-status')!.textContent = assigned ? t('Image personnalisée') : selected === 'neutral' ? t('Image de départ') : t('Image neutre utilisée');
    dialog.querySelector('#emotion-title')!.textContent = `${state.emoji} ${state.name}`;
    dialog.querySelector('#emotion-description')!.textContent = state.description;
    dialog.querySelector<HTMLElement>('#avatar-hermes')!.hidden = !agent;
    grid.innerHTML = states().map(s => html`<button type="button" class="emotion-slot" data-emotion="${s.id}" aria-pressed="${s.id === selected}" ${saving || agent && s.id !== 'neutral' ? 'disabled' : ''}><img src="${portrait(profile, s.id)}" alt="" /><span>${s.emoji} ${s.name}</span><small>${s.id === 'neutral' ? t('Image de base') : profile.emotion_avatars?.[s.id] ? t('Image ajoutée') : t('À personnaliser')}</small></button>`).join('');
    grid.querySelectorAll<HTMLButtonElement>('button').forEach(button => button.onclick = () => { selected = button.dataset.emotion as Emotion; paint(); grid.querySelector<HTMLButtonElement>(`[data-emotion="${selected}"]`)!.focus(); });
    dialog.querySelector<HTMLButtonElement>('#avatar-pick')!.disabled = saving;
    dialog.querySelector<HTMLButtonElement>('#avatar-reset')!.disabled = saving || !assigned;
    dialog.querySelector<HTMLButtonElement>('#avatar-voice-edit')!.disabled = saving;
  };
  const voiceLabel = () => void paintProfileVoice(dialog.querySelector('#avatar-voice-label')!, profile);
  const write = async (image: File | null) => {
    if (saving) return;
    saving = true; error.hidden = true; status.textContent = t('Enregistrement…'); paint();
    const revision = profile.revision;
    const emotion = selected;
    try {
      let base64: string | null = null;
      if (image) {
        if (image.size > 10 * 1024 * 1024) throw new Error(t('Choisis une image de moins de 10 Mio.'));
        const bytes = new Uint8Array(await image.arrayBuffer());
        let binary = '';
        for (let i = 0; i < bytes.length; i += 8192) binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
        base64 = btoa(binary);
      }
      const ok = await save({kind:'set_avatar',id:profile.id,revision,emotion,base64}, t('Nouvelle apparence enregistrée.'));
      status.textContent = ok ? t('Nouvelle apparence enregistrée.') : '';
      profile = current() ?? profile;
    } catch (e) { error.textContent = feedback(String(e)); error.hidden = false; status.textContent = ''; }
    finally { saving = false; file.value = ''; if (dialog.isConnected) paint(); }
  };
  file.addEventListener('change', () => { if (file.files?.[0]) void write(file.files[0]); });
  dialog.querySelector('#avatar-pick')!.addEventListener('click', () => file.click());
  dialog.querySelector('#avatar-reset')!.addEventListener('click', () => void write(null));
  dialog.querySelector('#avatar-voice-edit')!.addEventListener('click', () => {
    const editing = profile;
    void editProfileVoice(editing, async connections => {
      const ok = await save({kind:'connections',id:editing.id,revision:editing.revision,connections}, t`Voix de ${editing.name} enregistrée.`);
      profile = current() ?? profile;
      if (dialog.isConnected) { paint(); voiceLabel(); }
      return ok;
    });
  });
  dialog.querySelectorAll('[data-close]').forEach(button => button.addEventListener('click', () => dialog.close()));
  dialog.addEventListener('close', () => dialog.remove());
  document.body.append(dialog); paint(); voiceLabel(); dialog.showModal();
}
