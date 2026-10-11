import * as api from './api';
import { icon } from './icons';
import type { Profile, Snapshot, LiveState } from './types';

const escape = (s: string) => s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!));
const drafts = new Map<string, string>();
let spoken = true;
let historyRequest = 0;

export function status(session: Snapshot['session']): string {
  if (session.transcription === 'capturing') return 'Je t’écoute…';
  if (session.transcription === 'transcribing') return 'Je transcris ta phrase…';
  if (session.response === 'speaking') return 'Je te réponds…';
  if (session.response === 'generating') return 'Je prépare ma réponse…';
  if (session.microphone === 'unavailable') return 'Microphone indisponible';
  return session.microphone === 'listening' ? 'À ton écoute' : 'Microphone coupé';
}

export function conversationView(profile: Profile): string {
  return `<div class="page-heading"><div><p class="eyebrow">UNE CONVERSATION, TOUT SIMPLEMENT</p><h1>Avec ${escape(profile.name)}<span>.</span></h1><p>Active le micro, parle naturellement. Tu peux l’interrompre à tout moment.</p></div></div>
    <section class="conversation-controls"><button class="button primary" data-action="toggle_microphone" data-mic-toggle>${icon('mic')} <span>Activer le micro</span></button><button class="button secondary" data-action="stop_response">${icon('stop')} Arrêter la réponse</button><div class="voice-indicator"><span data-session-status></span><meter data-meter min="0" max="1" value="0" aria-label="Niveau du microphone"></meter><small data-speech-indicator>Microphone coupé</small></div></section>
    <p class="conversation-error" id="conversation-error" role="alert" hidden></p><p class="muted" id="conversation-notice" hidden></p>
    <p class="memory-receipt" id="memory-receipt" role="status" hidden></p>
    <details class="context-info" id="context-info" hidden><summary>Contexte & mémoire</summary><p id="context-budget"></p></details>
    <section class="conversation-log" id="conversation-log" aria-label="Conversation avec ${escape(profile.name)}"><div id="conversation-history" data-profile="${profile.id}"></div><article class="message user live-message" id="live-transcript" hidden><strong>Toi · transcription en cours</strong><p></p></article><article class="message assistant live-message" id="live-reply" hidden><strong>${escape(profile.name)}</strong><p></p></article></section>
    <form id="message-form" class="message-form"><label for="chat-text">Ou écris-lui ici</label><textarea id="chat-text" name="text" rows="2" maxlength="128000" placeholder="Qu’est-ce que tu as en tête ?">${escape(drafts.get(profile.id) ?? '')}</textarea><div class="message-actions"><label class="inline-check"><input id="speak-reply" type="checkbox" ${spoken ? 'checked' : ''} /> Lire la réponse à voix haute</label><button type="submit" class="button primary">Envoyer ${icon('arrow')}</button></div></form>`;
}

export function mountConversation(profile: Profile, report: (e: unknown) => void): void {
  const form = document.querySelector<HTMLFormElement>('#message-form');
  if (!form) return;
  const input = form.querySelector<HTMLTextAreaElement>('#chat-text')!;
  input.addEventListener('input', () => drafts.set(profile.id, input.value));
  form.querySelector<HTMLInputElement>('#speak-reply')!.addEventListener('change', (e) => { spoken = (e.target as HTMLInputElement).checked; });
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    const text = input.value.trim();
    if (!text) return;
    const button = form.querySelector<HTMLButtonElement>('[type="submit"]')!;
    if (button.disabled) return;
    button.disabled = true;
    try {
      await api.sendMessage(profile.id, text, spoken);
      if (input.value.trim() === text) { input.value = ''; drafts.delete(profile.id); }
      await loadConversationHistory(profile.id);
    } catch (error) { report(error); }
    finally { button.disabled = false; input.focus(); }
  });
  input.addEventListener('keydown', (e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); form.requestSubmit(); } });
  void loadConversationHistory(profile.id).catch(report);
}

export async function loadConversationHistory(profileId: string): Promise<void> {
  const root = document.querySelector<HTMLElement>('#conversation-history');
  if (!root || root.dataset.profile !== profileId) return;
  const request = ++historyRequest;
  const messages = await api.history(profileId);
  if (request !== historyRequest || !root.isConnected) return;
  root.innerHTML = messages.length ? messages.map((m) => `<article class="message ${m.role}"><div><strong>${m.role === 'user' ? 'Toi' : 'Réponse'}</strong><small>${new Intl.DateTimeFormat('fr-FR', { hour: '2-digit', minute: '2-digit' }).format(m.created_at)}${m.delivery === 'interrupted' ? ' · interrompue' : m.delivery === 'uncertain' ? ' · lecture incomplète' : m.delivery === 'text_only' ? ' · texte' : ' · diffusée'}</small></div><p>${escape(m.content)}</p>${['interrupted','uncertain'].includes(m.delivery) ? `<details><summary>Partie diffusée en entier</summary><p>${escape(m.heard_content || 'Aucune phrase complète confirmée. Le dernier fragment a pu être entendu en partie.')}</p></details>` : ''}</article>`).join('') : '<div class="empty-state"><h3>Fais-lui signe.</h3><p>Le micro s’active seulement quand tu le demandes.<br>Tu peux aussi commencer par un message écrit.</p></div>';
  scrollConversation();
}

function scrollConversation(): void {
  const log = document.querySelector('#conversation-log');
  if (log && log.scrollHeight - log.scrollTop - log.clientHeight < 240) log.scrollTop = log.scrollHeight;
}

export function paintLive(value: LiveState): void {
  const on = value.session.microphone === 'listening';
  document.querySelectorAll('[data-session-status]').forEach((el) => { el.textContent = status(value.session); });
  document.querySelectorAll<HTMLElement>('[data-mic-toggle]').forEach((el) => {
    const label = on ? 'Couper le micro' : 'Activer le micro';
    el.setAttribute('aria-label', label); el.title = label;
    el.classList.toggle('microphone-on', on);
    el.innerHTML = `${icon(on ? 'mic' : 'micOff')}${el.classList.contains('button') ? `<span>${label}</span>` : ''}`;
  });
  document.querySelectorAll('.mic-status').forEach((el) => { el.classList.toggle('listening', on); const span = el.querySelector('span'); if (span) span.textContent = on ? 'Microphone actif' : 'Microphone coupé'; });
  document.querySelector('.widget')?.classList.toggle('speaking', value.session.response === 'speaking');
  document.querySelector('.widget')?.classList.toggle('listening', on);
  const error = document.querySelector<HTMLElement>('#conversation-error');
  if (error) { error.textContent = value.live.error; error.hidden = !value.live.error; }
  const notice = document.querySelector<HTMLElement>('#conversation-notice');
  if (notice) { notice.textContent = value.live.notice; notice.hidden = !value.live.notice; }
  const receipt = document.querySelector<HTMLElement>('#memory-receipt');
  if (receipt) { receipt.textContent = value.live.memory_receipt; receipt.hidden = !value.live.memory_receipt; }
  const context = document.querySelector<HTMLElement>('#context-info');
  if (context) {
    const b = value.live.budget;
    context.hidden = !b;
    if (b) {
      const n = (v: number) => v.toLocaleString('fr-FR');
      context.querySelector('p')!.textContent = `Budget : ${n(b.limit)} tokens (${n(b.loaded_context)} chargés). Entrée : estimation prudente ${n(b.input_estimate)} ; réponse réservée : ${n(b.response_tokens)} ; marge : ${n(b.safety_tokens)}. ${b.memories_used.length} souvenir(s), ${b.history_kept} échange(s) transmis, ${b.history_omitted} écarté(s)${b.memories_omitted ? ` ; ${b.memories_omitted} souvenir(s) hors budget` : ''}.${value.live.usage?.prompt_tokens != null ? ` Usage renvoyé par LM Studio : ${n(value.live.usage.prompt_tokens)} en entrée, ${n(value.live.usage.completion_tokens ?? 0)} en sortie${value.live.usage.reasoning_tokens != null ? `, dont ${n(value.live.usage.reasoning_tokens)} de réflexion` : ''}.` : ''}`;
    }
  }
  for (const [selector, text, visible] of [
    ['#live-transcript', value.live.transcript, value.session.transcription !== 'idle'],
    ['#live-reply', value.live.reply, Boolean(value.live.tag)]
  ] as const) {
    const element = document.querySelector<HTMLElement>(selector);
    if (element) { element.hidden = !visible || !text; element.querySelector('p')!.textContent = text; }
  }
  if (!on) paintMeter({ level: 0, speech: false }, false);
  scrollConversation();
}

export function paintMeter(value: { level: number; speech: boolean }, on = true): void {
  document.querySelectorAll<HTMLMeterElement>('[data-meter]').forEach((el) => { el.value = value.level; });
  document.querySelectorAll('[data-speech-indicator]').forEach((el) => { el.textContent = on ? value.speech ? 'Parole détectée' : 'Écoute continue' : 'Microphone coupé'; });
}
