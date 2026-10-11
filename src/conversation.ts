import { t, html, feedback, formatLocale } from './i18n';
import * as api from './api';
import { icon } from './icons';
import type { Profile, Snapshot, LiveState } from './types';
import { webReport } from './web';

const escape = (s: string) => s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!));
const drafts = new Map<string, string>();
let spoken = true;
let historyRequest = 0;

export function status(session: Snapshot['session']): string {
  if (session.transcription === 'capturing') return t('Je t’écoute…');
  if (session.transcription === 'transcribing') return t('Je transcris ta phrase…');
  if (session.response === 'speaking') return t('Je te réponds…');
  if (session.response === 'generating') return t('Je prépare ma réponse…');
  if (session.microphone === 'unavailable') return t('Microphone indisponible');
  return session.microphone === 'listening' ? t('À ton écoute') : t('Microphone coupé');
}

export function conversationView(profile: Profile, mode: Snapshot['settings']['web_search']): string {
  return html`<div class="page-heading"><div><p class="eyebrow">UNE CONVERSATION, TOUT SIMPLEMENT</p><h1>Avec ${escape(profile.name)}<span>.</span></h1><p>Active le micro, parle naturellement. Tu peux l’interrompre à tout moment.</p></div></div>
    <section class="conversation-controls"><button class="button primary" data-action="toggle_microphone" data-mic-toggle>${icon('mic')} <span>Activer le micro</span></button><button class="button secondary" data-action="stop_response">${icon('stop')} Arrêter la réponse</button><div class="voice-indicator"><span data-session-status></span><meter data-meter min="0" max="1" value="0" aria-label="Niveau du microphone"></meter><small data-speech-indicator>Microphone coupé</small></div></section>
    <p class="conversation-error" id="conversation-error" role="alert" hidden></p><p class="muted" id="conversation-notice" hidden></p>
    <section class="settings-card" id="hermes-approval" hidden></section>
    <p class="memory-receipt" id="memory-receipt" role="status" hidden></p>
    <details class="context-info" id="context-info" hidden><summary>Contexte & mémoire</summary><p id="context-budget"></p></details>
    <section class="conversation-log" id="conversation-log" aria-label="Conversation avec ${escape(profile.name)}"><div id="conversation-history" data-profile="${profile.id}"></div><article class="message user live-message" id="live-transcript" hidden><strong>Toi · transcription en cours</strong><p></p></article><article class="message assistant live-message" id="live-reply" hidden><strong>${escape(profile.name)}</strong><p></p></article><div id="web-report"></div></section>
    <form id="message-form" class="message-form"><label for="chat-text">Ou écris-lui ici</label><textarea id="chat-text" name="text" rows="2" maxlength="128000" placeholder="Qu’est-ce que tu as en tête ?">${escape(drafts.get(profile.id) ?? '')}</textarea><div class="message-actions"><label class="inline-check"><input id="speak-reply" type="checkbox" ${spoken ? 'checked' : ''} /> Lire la réponse à voix haute</label><label class="inline-check"><input id="search-message" type="checkbox" ${mode==='off'||profile.connections.llm_provider==='hermes'?'disabled':''} /> Chercher sur Internet pour ce message</label><button type="submit" class="button primary">Envoyer ${icon('arrow')}</button></div></form>`;
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
      if (profile.connections.llm_provider === 'hermes' && text === '/new') { await api.newHermesConversation(profile.id); input.value = ''; drafts.delete(profile.id); return; }
      await api.sendMessage(profile.id, form.querySelector<HTMLInputElement>('#search-message')!.checked ? '/web '+text : text, spoken);
      form.querySelector<HTMLInputElement>('#search-message')!.checked = false;
      if (input.value.trim() === text) { input.value = ''; drafts.delete(profile.id); }
      await loadConversationHistory(profile.id);
    } catch (error) { report(error); }
    finally { button.disabled = false; input.focus(); }
  });
  input.addEventListener('keydown', (e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); form.requestSubmit(); } });
  document.querySelector('#conversation-log')?.addEventListener('click', event => {
    const target=(event.target as HTMLElement).closest<HTMLButtonElement>('[data-web-source]');
    if (target) void api.openWebSource(target.dataset.sourceProfile!, target.dataset.sourceMessage ?? null, Number(target.dataset.webSource)).catch(report);
  });
  void loadConversationHistory(profile.id).catch(report);
}

export async function loadConversationHistory(profileId: string): Promise<void> {
  const root = document.querySelector<HTMLElement>('#conversation-history');
  if (!root || root.dataset.profile !== profileId) return;
  const request = ++historyRequest;
  const messages = await api.history(profileId);
  if (request !== historyRequest || !root.isConnected) return;
  const follow = nearConversationBottom();
  root.innerHTML = messages.length ? messages.map((m) => html`<article class="message ${m.role}"><div><strong>${m.role === 'user' ? t('Toi') : t('Réponse')}</strong><small>${new Intl.DateTimeFormat(formatLocale(), { hour: '2-digit', minute: '2-digit' }).format(m.created_at)}${m.delivery === 'interrupted' ? t(' · interrompue') : m.delivery === 'uncertain' ? t(' · lecture incomplète') : m.delivery === 'text_only' ? t(' · texte') : t(' · diffusée')}</small></div><p>${escape(m.content)}</p>${webReport(m.web,profileId,m.id)}${['interrupted','uncertain'].includes(m.delivery) ? html`<details><summary>Partie diffusée en entier</summary><p>${escape(m.heard_content || t('Aucune phrase complète confirmée. Le dernier fragment a pu être entendu en partie.'))}</p></details>` : ''}</article>`).join('') : html`<div class="empty-state"><h3>Fais-lui signe.</h3><p>Le micro s’active seulement quand tu le demandes.<br>Tu peux aussi commencer par un message écrit.</p></div>`;
  scrollConversation(follow);
}

function nearConversationBottom(): boolean {
  const log = document.querySelector('#conversation-log');
  return Boolean(log && log.scrollHeight - log.scrollTop - log.clientHeight < 240);
}

function scrollConversation(follow = false): void {
  const log = document.querySelector('#conversation-log');
  if (log && (follow || nearConversationBottom())) log.scrollTop = log.scrollHeight;
}

export function paintLive(value: LiveState): void {
  const follow = nearConversationBottom();
  const on = value.session.microphone === 'listening';
  document.querySelectorAll('[data-session-status]').forEach((el) => { el.textContent = value.live.web?.searching ? t('Recherche sur Internet…') : status(value.session); });
  document.querySelectorAll<HTMLElement>('[data-mic-toggle]').forEach((el) => {
    const label = on ? t('Couper le micro') : t('Activer le micro');
    el.setAttribute('aria-label', label); el.title = label;
    el.classList.toggle('microphone-on', on);
    el.innerHTML = `${icon(on ? 'mic' : 'micOff')}${el.classList.contains('button') ? html`<span>${label}</span>` : ''}`;
  });
  document.querySelectorAll('.mic-status').forEach((el) => { el.classList.toggle('listening', on); const span = el.querySelector('span'); if (span) span.textContent = on ? t('Microphone actif') : t('Microphone coupé'); });
  document.querySelector('.widget')?.classList.toggle('speaking', value.session.response === 'speaking');
  document.querySelector('.widget')?.classList.toggle('listening', on);
  const web = document.querySelector<HTMLElement>('#web-report');
  if (web) { const content=webReport(value.live.tag ? value.live.web : null,value.session.profile_id,null); if(web.innerHTML!==content)web.innerHTML=content; }
  const approvalRoot = document.querySelector<HTMLElement>('#hermes-approval');
  const approval = value.live.hermes_approval;
  if (approvalRoot) {
    approvalRoot.hidden = !approval;
    if (approval) {
      const identity = JSON.stringify([approval.run_id, approval.request_id]);
      if (approvalRoot.dataset.identity !== identity) {
        approvalRoot.dataset.identity = identity;
        approvalRoot.innerHTML = html`<h3>Hermes demande ton accord</h3><pre></pre><div class="settings-actions"><button type="button" class="button primary" data-allow ${approval.allow_once ? '' : 'disabled'}>Autoriser une fois</button><button type="button" class="button secondary" data-deny>Refuser</button></div><p role="alert"></p>`;
        approvalRoot.querySelector('pre')!.textContent = approval.description;
        for (const [selector, allow] of [['[data-allow]',true],['[data-deny]',false]] as const) approvalRoot.querySelector<HTMLButtonElement>(selector)!.onclick = async () => {
          approvalRoot.querySelectorAll<HTMLButtonElement>('button').forEach(b => b.disabled = true);
          try { await api.answerHermesApproval(approval.run_id, approval.request_id, allow); }
          catch (error) {
            if (approvalRoot.dataset.identity === identity) {
              approvalRoot.querySelector('[role="alert"]')!.textContent = feedback(String(error));
              approvalRoot.querySelector<HTMLButtonElement>('[data-allow]')!.disabled = !approval.allow_once;
              approvalRoot.querySelector<HTMLButtonElement>('[data-deny]')!.disabled = false;
            }
          }
        };
      }
    } else { approvalRoot.dataset.identity = ''; approvalRoot.replaceChildren(); }
  }
  const error = document.querySelector<HTMLElement>('#conversation-error');
  if (error) { error.textContent = feedback(value.live.error); error.hidden = !value.live.error; }
  const notice = document.querySelector<HTMLElement>('#conversation-notice');
  if (notice) { notice.textContent = feedback(value.live.notice); notice.hidden = !value.live.notice; }
  const receipt = document.querySelector<HTMLElement>('#memory-receipt');
  if (receipt) { receipt.textContent = feedback(value.live.memory_receipt); receipt.hidden = !value.live.memory_receipt; }
  const context = document.querySelector<HTMLElement>('#context-info');
  if (context) {
    const b = value.live.budget;
    context.hidden = !b;
    if (b) {
      const n = (v: number) => v.toLocaleString(formatLocale());
      context.querySelector('p')!.textContent = t`Budget : ${n(b.limit)} tokens (contexte disponible : ${n(b.loaded_context)}). Entrée : estimation prudente ${n(b.input_estimate)} ; réponse réservée : ${n(b.response_tokens)} ; marge : ${n(b.safety_tokens)}. ${b.memories_used.length} souvenir(s), ${b.history_kept} échange(s) transmis, ${b.history_omitted} écarté(s)${b.memories_omitted ? t` ; ${b.memories_omitted} souvenir(s) hors budget` : ''}.${value.live.usage?.prompt_tokens != null ? t` Usage renvoyé par le fournisseur : ${n(value.live.usage.prompt_tokens)} en entrée, ${n(value.live.usage.completion_tokens ?? 0)} en sortie${value.live.usage.reasoning_tokens != null ? t`, dont ${n(value.live.usage.reasoning_tokens)} de réflexion` : ''}.` : ''}`;
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
  scrollConversation(follow);
}

export function paintMeter(value: { level: number; speech: boolean }, on = true): void {
  document.querySelectorAll<HTMLMeterElement>('[data-meter]').forEach((el) => { el.value = value.level; });
  document.querySelectorAll('[data-speech-indicator]').forEach((el) => { el.textContent = on ? value.speech ? t('Parole détectée') : t('Écoute continue') : t('Microphone coupé'); });
}
