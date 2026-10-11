import { t, html, formatLocale, languages, feedback } from './i18n';
import * as api from './api';
import { icon } from './icons';
import { bindRangeControls } from './settings-controls';
import type { Connections, LanguageCatalog, Profile, VoiceCatalog } from './types';

const escape = (text: string) => text.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!));
type Service = 'lm' | 'voice';
interface Draft {
  revision: number;
  values: Connections;
  dirty: boolean;
  saving: boolean;
  lm?: LanguageCatalog;
  voice?: VoiceCatalog;
  errors: Partial<Record<Service, string>>;
  requests: Record<Service, number>;
  loading: Record<Service, boolean>;
  repaint?: () => void;
}
const drafts = new Map<string, Draft>();
const defaultConnections: Connections = { lm_studio_url: 'http://127.0.0.1:1234', voice_studio_url: 'http://127.0.0.1:3900', llm_model: null, llm_model_key: null, tts_engine: null, tts_voice: null, stt_model: null, language: 'fr', speed: 1, volume: 0.8, context_limit: null, response_tokens: 1024, reasoning: null };
export const freshConnections = (): Connections => ({ ...defaultConnections });

function options(items: { id: string; label: string; disabled?: boolean }[], value: string | null): string {
  const unknown = value && !items.some((i) => i.id === value);
  return html`<option value="">À choisir</option>${unknown ? html`<option value="${escape(value)}" selected>${escape(value)} · choix enregistré, non vérifié</option>` : ''}${items.map((i) => html`<option value="${escape(i.id)}" ${value === i.id ? 'selected' : ''} ${i.disabled ? 'disabled' : ''}>${escape(i.label)}</option>`).join('')}`;
}

export function mountConnections(root: HTMLElement, profile: Profile, profiles: Profile[], select: (id: string) => void, save: (revision: number, value: Connections) => Promise<boolean>): void {
  let draft = drafts.get(profile.id);
  if (!draft) {
    draft = { revision: profile.revision, values: { ...profile.connections }, dirty: false, saving: false, errors: {}, requests: { lm: 0, voice: 0 }, loading: { lm: false, voice: false } };
    drafts.set(profile.id, draft);
  } else if (!draft.dirty && draft.revision !== profile.revision) {
    if (draft.values.lm_studio_url !== profile.connections.lm_studio_url) { draft.lm = undefined; delete draft.errors.lm; ++draft.requests.lm; draft.loading.lm = false; }
    if (draft.values.voice_studio_url !== profile.connections.voice_studio_url) { draft.voice = undefined; delete draft.errors.voice; ++draft.requests.voice; draft.loading.voice = false; }
    draft.values = { ...profile.connections };
    draft.revision = profile.revision;
  }
  // Only the built-in preset can change language. Keep unsaved model/voice
  // choices when its locale changes, without hiding unrelated edit conflicts.
  if (draft.values.language !== profile.connections.language) {
    if (draft.revision + 1 === profile.revision) draft.revision = profile.revision;
    draft.values.language = profile.connections.language;
  }
  const d = draft;
  root.dataset.profile = profile.id;

  const status = (service: Service) => d.loading[service] ? html`<p class="connection-status" role="status">Lecture du catalogue…</p>` : d.errors[service] ? html`<p class="connection-error" role="alert">${escape(feedback(d.errors[service]!))}</p>` : '';
  const refreshButton = (service: Service) => html`<button class="button secondary small" type="button" data-refresh="${service}" ${d.loading[service] ? 'disabled' : ''}>${d.loading[service] ? t('Connexion…') : t('Tester et actualiser')}</button>`;

  function paint(): void {
    if (!root.isConnected || root.dataset.profile !== profile.id) return;
    const readiness = { ready: t('Prêt'), missing_engine: t('Moteur indisponible'), missing_model: t('Modèle à installer dans VoiceStudio'), unknown: t('Installation du modèle non confirmée') };
    const engine = d.voice?.tts_engines.find((e) => e.id === d.values.tts_engine);
    const model = d.lm?.models.find(m => m.id === d.values.llm_model || m.key === d.values.llm_model);
    const thoughts: Record<string,string> = {off:t('Désactivée'),on:t('Activée'),low:t('Faible'),medium:t('Moyenne'),high:t('Forte'),xhigh:t('Très forte')};
    const reasoning = model?.reasoning?.allowed_options.filter(choice => choice in thoughts) ?? [];
    const reasoningOptions = reasoning.map(choice => html`<option value="${choice}" ${d.values.reasoning === choice ? 'selected' : ''}>${thoughts[choice]}</option>`).join('');
    const number = (n: number) => n.toLocaleString(formatLocale());
    const lmChoices = d.lm?.models.map((m) => ({ id: m.id, label: `${m.label}${m.quantization ? ` · ${m.quantization}` : ''} · ${m.loaded === true ? t('chargé') : m.loaded === false ? t('non chargé') : t('état inconnu')}` })) ?? [];
    const voices = d.voice?.voices.map((v) => ({ id: v.voice_id, label: `${v.name}${v.language ? ` · ${v.language}` : ''} · ${v.voice_id}`, disabled: v.type === 'profile' && engine !== undefined && !engine.supports_cloning })) ?? [];
    root.innerHTML = html`<section class="settings-card connections-card"><div class="section-heading"><h2>Voix & modèles</h2><span class="tag">CONNEXIONS LOCALES</span></div>
      <p>Choisis les modèles et la voix de chaque compagnon. Ces choix sont utilisés dès le prochain échange.</p><details class="language-help"><summary>Langues, modèles et voix</summary><p>Sherka d’origine suit la langue de l’interface ; les compagnons personnalisés gardent leur langue. Choisis toi-même des modèles et des voix compatibles dans LM Studio et VoiceStudio.</p><p>Vérifie que le modèle de conversation, la reconnaissance vocale et la voix choisis prennent en charge la langue du compagnon. Leurs réglages se font dans LM Studio et VoiceStudio.</p></details>
      <label class="profile-picker">Compagnon à configurer<select id="connection-profile">${profiles.map((p) => html`<option value="${p.id}" ${p.id === profile.id ? 'selected' : ''}>${escape(p.name)}</option>`).join('')}</select></label>
      <form id="connections-form">
        <fieldset class="provider"><legend>LM Studio</legend><div class="connection-address"><label>Adresse de LM Studio<input type="url" name="lm_studio_url" required maxlength="2048" value="${escape(d.values.lm_studio_url)}" /></label>${refreshButton('lm')}</div>${status('lm')}
        ${d.lm ? html`<p class="connection-status">${d.lm.models.length} modèle(s) de conversation détecté(s) · ${d.lm.models.filter((m) => m.loaded).length} chargé(s)${d.lm.excluded_models ? t` · ${d.lm.excluded_models} autre(s) modèle(s) écarté(s)` : ''}</p>` : ''}
        <label>Modèle de conversation<select name="llm_model">${options(lmChoices, d.values.llm_model)}</select></label>
        <p class="connection-note">${model ? t`Maximum du modèle : ${model.max_context_length ? number(model.max_context_length) : t('inconnu')} tokens. Contexte chargé : ${model.loaded_context_length ? number(model.loaded_context_length) + t(' tokens') : t('à vérifier au chargement')}.` : t('Le contexte réel sera vérifié avant chaque réponse.')} Le changement de compagnon décharge son ancien modèle avant de charger le nouveau ; un modèle commun reste chargé.</p>
        <div id="reasoning-controls">${reasoning.length ? html`<label>Pensée<select name="reasoning"><option value="" ${d.values.reasoning == null ? 'selected' : ''}>Réglage de LM Studio${model?.reasoning?.default ? ' · ' + (thoughts[model.reasoning.default] ?? escape(model.reasoning.default)) : ''}</option>${d.values.reasoning && !reasoning.includes(d.values.reasoning) ? html`<option value="${escape(d.values.reasoning)}" selected>Choix enregistré non disponible · à modifier</option>` : ''}${reasoningOptions}</select></label><p class="connection-note">${reasoning.some(choice => ['low','medium','high','xhigh'].includes(choice)) ? t('Choisis une intensité plus faible pour réduire le temps de réflexion.') : t('Ce modèle propose marche/arrêt, sans réglage d’intensité.')} La réflexion partage la réserve de tokens avec la réponse. Seule la réponse finale est lue à voix haute.</p>` : html`<p class="connection-note">${model ? t('LM Studio ne signale aucun réglage de pensée pour ce modèle.') : t('Les options de pensée apparaîtront après lecture du modèle.')}</p>`}</div>
        <details><summary>Budget de conversation</summary><div class="connection-selects"><label>Plafond de contexte (vide = automatique)<input name="context_limit" type="number" min="2048" max="2000000" step="1" value="${d.values.context_limit ?? ''}" placeholder="Contexte chargé dans LM Studio" /></label><label>Tokens réservés à la réponse<input name="response_tokens" type="number" min="128" max="8192" step="1" value="${d.values.response_tokens}" /></label></div><p class="connection-note">Le plafond Sherka ne modifie pas le chargement dans LM Studio. Le budget utilise une estimation prudente et une marge de sécurité ; les anciens échanges sont écartés si nécessaire, sans couper ton nouveau message.</p></details></fieldset>
        <fieldset class="provider"><legend>VoiceStudio</legend><div class="connection-address"><label>Adresse de VoiceStudio<input type="url" name="voice_studio_url" required maxlength="2048" value="${escape(d.values.voice_studio_url)}" /></label>${refreshButton('voice')}</div>${status('voice')}
        ${d.voice ? html`<p class="connection-status">VoiceStudio ${escape(d.voice.service_version)} · ${d.voice.voices.length} voix · ${d.voice.tts_engines.filter((e) => e.readiness === 'ready').length} moteur(s) TTS prêt(s)</p><p class="connection-note">Transcription : ${d.voice.streaming_transcription ? t('streaming annoncé') : t('pas de streaming annoncé')}${d.voice.batch_transcription ? t(' · HTTP disponible') : ''}.</p>` : ''}
        <label>Moteur de synthèse vocale<select name="tts_engine">${options(d.voice?.tts_engines.map((e) => ({ id: e.id, label: `${e.label} · ${readiness[e.readiness]}${e.active ? t(' · actif dans VoiceStudio') : ''}`, disabled: e.readiness !== 'ready' })) ?? [], d.values.tts_engine)}</select></label>
        <label>Voix du compagnon<select name="tts_voice">${options(d.voice ? [{ id: 'default', label: t('Voix par défaut du moteur') }, ...voices] : [], d.values.tts_voice)}</select></label>
        ${engine && !engine.supports_cloning ? html`<p class="connection-note">Ce moteur ne prend pas en charge tes profils vocaux personnalisés.</p>` : ''}
        ${d.voice?.voice_alias_count ? html`<p class="connection-note">Les alias comme « alloy » ou « nova » désignent la voix par défaut du moteur ; ils ne constituent pas des voix supplémentaires.</p>` : ''}
        <label>Modèle de transcription<select name="stt_model">${options([...(d.voice?.stt_models.map((id) => ({ id, label: t`${id} · transcription HTTP` })) ?? []), ...(d.voice?.dictation_models.map((m) => ({ id: m.id, label: t`${m.label} · dictée directe · ${m.languages}${m.installed ? '' : t(' · à installer dans VoiceStudio')}`, disabled: !m.installed || !d.voice?.dictation_engine_available })) ?? [])], d.values.stt_model)}</select></label>
        ${d.voice ? html`<details class="model-inventory"><summary>Modèles TTS présents et moteurs disponibles</summary><ul>${d.voice.tts_models.filter((m) => m.installed || m.incomplete).map((m) => html`<li><strong>${escape(m.label)}</strong><span>${escape(m.repo_id)}</span><small>${!m.supported ? t('Non pris en charge sur ce PC') : m.incomplete ? t('Téléchargement incomplet') : t('Fichiers présents')} · ${m.engines.map((id) => { const e = d.voice!.tts_engines.find((entry) => entry.id === id); return `${escape(id)} : ${e ? readiness[e.readiness] : t('état inconnu')}`; }).join(' / ')}</small></li>`).join('') || html`<li>Aucun modèle TTS installé annoncé par VoiceStudio.</li>`}</ul></details>` : ''}
        </fieldset>
        <div class="connection-selects"><label>Langue du compagnon<input name="language" readonly value="${escape(languages.find(([code])=>code===d.values.language)?.[1] ?? d.values.language)}" /></label><label>Vitesse de parole<input name="speed" type="number" min="0.5" max="2" step="0.05" value="${d.values.speed}" /></label><label>Volume de Sherka<span class="range-control"><input id="companion-volume" name="volume" type="range" min="0" max="1" step="0.05" value="${d.values.volume}" /><output id="volume-value" for="companion-volume">${Math.round(d.values.volume * 100)} %</output></span></label></div><div class="settings-actions"><button class="button primary" id="save-connections" type="submit" ${!d.dirty || d.saving ? 'disabled' : ''}>${icon('check')} ${d.saving ? t('Enregistrement…') : t('Enregistrer ces choix')}</button><span class="connection-note" id="connection-dirty">${d.dirty ? t('Modifications non enregistrées') : t('Réglages propres à ce compagnon')}</span></div>
      </form></section>`;
    bindRangeControls(root);
    root.querySelector<HTMLSelectElement>('#connection-profile')!.addEventListener('change', (event) => select((event.target as HTMLSelectElement).value));
    root.querySelector<HTMLSelectElement>('#connection-profile')!.disabled = d.saving;
    root.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[name]').forEach((input) => { input.disabled = d.saving; });
    root.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[name]').forEach((input) => input.addEventListener('input', () => {
      const key = input.name as keyof Connections;
      if (key === 'lm_studio_url' || key === 'voice_studio_url') {
        const service = key === 'lm_studio_url' ? 'lm' : 'voice';
        d.values[key] = input.value;
        if (service === 'lm') d.values.llm_model_key = null;
        ++d.requests[service]; d.loading[service] = false; delete d.errors[service]; delete d[service];
        // Selections refer to a particular service; they cannot be silently
        // reused for another URL. Keep the input mounted while typing.
        if (service === 'lm') { d.values.reasoning = null; d.values.llm_model_key = null; root.querySelector('#reasoning-controls')?.replaceChildren(); }
        const keys = service === 'lm' ? ['llm_model'] as const : ['tts_engine', 'tts_voice', 'stt_model'] as const;
        for (const name of keys) { d.values[name] = null; root.querySelector<HTMLSelectElement>(`[name="${name}"]`)!.innerHTML = options([], null); }
        const refresh = root.querySelector<HTMLButtonElement>(`[data-refresh="${service}"]`)!;
        refresh.disabled = false;
        refresh.textContent = t('Tester et actualiser');
        input.closest('fieldset')!.querySelectorAll('.connection-status,.connection-error,.connection-note,.model-inventory').forEach((el) => el.remove());
      } else if (key === 'speed' || key === 'volume' || key === 'response_tokens') {
        d.values[key] = Number(input.value);
      } else if (key === 'context_limit') {
        d.values[key] = input.value ? Number(input.value) : null;
      } else if (key === 'language') {
        return;
      } else {
        d.values[key] = input.value || null;
        if (key === 'llm_model') { d.values.llm_model_key = d.lm?.models.find(m => m.id === input.value)?.key ?? null; d.values.reasoning = null; }
        if (key === 'tts_engine') {
          const picked = d.voice?.tts_engines.find((e) => e.id === input.value);
          const voice = d.voice?.voices.find((v) => v.voice_id === d.values.tts_voice);
          if (picked && !picked.supports_cloning && voice?.type === 'profile') d.values.tts_voice = null;
        }
      }
      d.dirty = true;
      root.querySelector<HTMLButtonElement>('#save-connections')!.disabled = false;
      root.querySelector('#connection-dirty')!.textContent = t('Modifications non enregistrées');
      if (key === 'tts_engine' || key === 'llm_model') paint();
    }));
    root.querySelectorAll<HTMLButtonElement>('[data-refresh]').forEach((button) => button.addEventListener('click', () => void probe(button.dataset.refresh as Service)));
    root.querySelector<HTMLFormElement>('#connections-form')!.addEventListener('submit', async (event) => {
      event.preventDefault();
      if (!d.dirty || d.saving) return;
      d.saving = true; paint();
      const ok = await save(d.revision, { ...d.values });
      d.saving = false;
      if (ok) { d.dirty = false; d.revision += 1; }
      else { drafts.delete(profile.id); select(profile.id); return; }
      d.repaint?.();
    });
  }

  async function probe(service: Service): Promise<void> {
    const token = ++d.requests[service];
    const url = service === 'lm' ? d.values.lm_studio_url : d.values.voice_studio_url;
    d.loading[service] = true; delete d.errors[service]; delete d[service]; paint();
    try {
      if (service === 'lm') { const result = await api.discoverLanguageModels(url); if (token === d.requests[service]) d.lm = result; }
      else { const result = await api.discoverVoices(url); if (token === d.requests[service]) d.voice = result; }
    } catch (error) { if (token === d.requests[service]) d.errors[service] = String(error); }
    finally { if (token === d.requests[service]) { d.loading[service] = false; d.repaint?.(); } }
  }
  d.repaint = paint;
  paint();
  for (const service of ['lm', 'voice'] as const) if (!d[service] && !d.loading[service] && !d.errors[service]) void probe(service);
}
