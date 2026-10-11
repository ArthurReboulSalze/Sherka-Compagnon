import { t, html, formatLocale, languages, feedback } from './i18n';
import * as api from './api';
import { icon } from './icons';
import { bindRangeControls } from './settings-controls';
import type { Connections, LanguageCatalog, Profile, VoiceCatalog, LlmProvider } from './types';

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
  keyStored?: boolean;
  hermesProfiles?: string[];
  hermesServers?: string[];
  hermesReady?: boolean;
  keyRequest?: number;
}
const providers: [LlmProvider, string, string][] = [
  ['lm_studio', 'LM Studio', 'http://127.0.0.1:1234'],
  ['hermes', 'Hermes · agent distant', 'http://127.0.0.1:8642'],
  ['open_router', 'OpenRouter', 'https://openrouter.ai/api/v1'],
  ['zai', 'Z.ai API', 'https://api.z.ai/api/paas/v4'],
  ['zai_coding', 'Z.ai Coding Plan', 'https://api.z.ai/api/coding/paas/v4'],
  ['open_ai', 'OpenAI', 'https://api.openai.com/v1'],
  ['anthropic', 'Anthropic', 'https://api.anthropic.com/v1'],
  ['custom_open_ai', 'API compatible OpenAI', ''],
  ['custom_anthropic', 'API compatible Anthropic', ''],
];
function manualThoughts(c: Connections): string[] {
  if (c.llm_provider === 'open_ai' || c.llm_provider === 'custom_open_ai') return ['off', 'on', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max'];
  if (c.llm_provider === 'custom_anthropic') return ['off', 'on'];
  if (c.llm_provider === 'zai' || c.llm_provider === 'zai_coding') {
    const id = c.llm_model?.toLowerCase() ?? '';
    if (id.startsWith('glm-5.3')) return ['on', 'low', 'high', 'max'];
    if (id.startsWith('glm-5.2')) return ['off', 'on', 'low', 'medium', 'high', 'xhigh', 'max'];
    if (['glm-5', 'glm-4.6', 'glm-4.7'].some(s => id.startsWith(s))) return ['off', 'on'];
  }
  return [];
}
function reasoningView(values: Connections, model: LanguageCatalog['models'][number] | undefined): string {
  const local = !values.llm_provider || values.llm_provider === 'lm_studio';
  const thoughts: Record<string,string> = {off:t('Désactivée'),on:t('Activée'),minimal:t('Minimale'),low:t('Faible'),medium:t('Moyenne'),high:t('Forte'),xhigh:t('Très forte'),max:t('Maximale')};
  const choices = (model?.reasoning?.allowed_options ?? manualThoughts(values)).filter(choice => choice in thoughts);
  if (!choices.length) return html`<p class="connection-note">${model ? (local ? t('LM Studio ne signale aucun réglage de pensée pour ce modèle.') : t('Le fournisseur ne signale aucun réglage de pensée pour ce modèle.')) : t('Les options de pensée apparaîtront après lecture du modèle.')}</p>`;
  return html`<label>Pensée<select name="reasoning"><option value="" ${values.reasoning == null ? 'selected' : ''}>${local ? t('Réglage de LM Studio') : t('Réglage du fournisseur')}${model?.reasoning?.default ? ' · ' + (thoughts[model.reasoning.default] ?? escape(model.reasoning.default)) : ''}</option>${values.reasoning && !choices.includes(values.reasoning) ? html`<option value="${escape(values.reasoning)}" selected>Choix enregistré non disponible · à modifier</option>` : ''}${choices.map(choice => html`<option value="${choice}" ${values.reasoning === choice ? 'selected' : ''}>${thoughts[choice]}</option>`).join('')}</select></label><p class="connection-note">${choices.some(choice => ['minimal','low','medium','high','xhigh','max'].includes(choice)) ? t('Choisis une intensité plus faible pour réduire le temps de réflexion.') : t('Ce modèle propose marche/arrêt, sans réglage d’intensité.')} La réflexion partage la réserve de tokens avec la réponse. Seule la réponse finale est lue à voix haute.</p>`;
}
const drafts = new Map<string, Draft>();
const defaultConnections: Connections = { lm_studio_url: 'http://127.0.0.1:1234', voice_studio_url: 'http://127.0.0.1:3900', llm_model: null, llm_model_key: null, tts_engine: null, tts_voice: null, stt_model: null, language: 'fr', speed: 1, volume: 0.8, context_limit: null, response_tokens: 8192, reasoning: null };
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
    if (draft.values.lm_studio_url !== profile.connections.lm_studio_url || draft.values.llm_provider !== profile.connections.llm_provider || draft.values.llm_api_url !== profile.connections.llm_api_url || draft.values.hermes_profile !== profile.connections.hermes_profile) { draft.lm = undefined; draft.keyStored = undefined; draft.keyRequest = (draft.keyRequest ?? 0) + 1; delete draft.errors.lm; ++draft.requests.lm; draft.loading.lm = false; }
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
    // Keep an unsaved key only in its mounted input when an asynchronous
    // catalog refresh repaints the same connection. Never keep it in a draft.
    const keyScope = JSON.stringify([d.values.llm_provider, d.values.llm_api_url, d.values.hermes_profile]);
    const previousKey = root.querySelector<HTMLInputElement>('#provider-api-key');
    const enteredKey = previousKey?.dataset.scope === keyScope ? previousKey.value : '';
    const restoreKeyFocus = previousKey === document.activeElement && previousKey?.dataset.scope === keyScope;
    const readiness = { ready: t('Prêt'), missing_engine: t('Moteur indisponible'), missing_model: t('Modèle à installer dans VoiceStudio'), unknown: t('Installation du modèle non confirmée') };
    const engine = d.voice?.tts_engines.find((e) => e.id === d.values.tts_engine);
    const model = d.lm?.models.find(m => m.id === d.values.llm_model || m.key === d.values.llm_model);
    const provider = d.values.llm_provider ?? 'lm_studio';
    const local = provider === 'lm_studio';
    const agent = provider === 'hermes';
    const providerInfo = providers.find(([id]) => id === provider)!;
    const number = (n: number) => n.toLocaleString(formatLocale());
    const lmChoices = d.lm?.models.map((m) => ({ id: m.id, label: `${m.label}${m.quantization ? ` · ${m.quantization}` : ''} · ${m.loaded === true ? t('chargé') : m.loaded === false ? t('non chargé') : t('état inconnu')}` })) ?? [];
    const voices = d.voice?.voices.map((v) => ({ id: v.voice_id, label: `${v.name}${v.language ? ` · ${v.language}` : ''} · ${v.voice_id}`, disabled: v.type === 'profile' && engine !== undefined && !engine.supports_cloning })) ?? [];
    root.innerHTML = html`<section class="settings-card connections-card"><div class="section-heading"><h2>Voix & modèles</h2><span class="tag">FOURNISSEURS</span></div>
      <p>Choisis les modèles et la voix de chaque compagnon. Ces choix sont utilisés dès le prochain échange.</p><details class="language-help"><summary>Langues, modèles et voix</summary><p>Sherka d’origine suit la langue de l’interface ; les compagnons personnalisés gardent leur langue. Choisis toi-même des modèles et des voix compatibles chez ton fournisseur et dans VoiceStudio.</p><p>Vérifie que le modèle de conversation, la reconnaissance vocale et la voix choisis prennent en charge la langue du compagnon. Leurs réglages se font chez le fournisseur et dans VoiceStudio.</p></details>
      <label class="profile-picker">Compagnon à configurer<select id="connection-profile">${profiles.map((p) => html`<option value="${p.id}" ${p.id === profile.id ? 'selected' : ''}>${escape(p.name)}</option>`).join('')}</select></label>
      <form id="connections-form">
        <fieldset class="provider"><legend>Conversation</legend><label>Fournisseur<select name="llm_provider">${providers.map(([id,label]) => html`<option value="${id}" ${provider === id ? 'selected' : ''}>${escape(id.startsWith('custom_') || id === 'hermes' ? t(label) : label)}</option>`).join('')}</select></label>
        ${local ? html`<div class="connection-address"><label>Adresse de LM Studio<input type="url" name="lm_studio_url" required maxlength="2048" value="${escape(d.values.lm_studio_url)}" /></label>${refreshButton('lm')}</div>` : html`${agent ? html`<p class="connection-note">Hermes gère la conversation, sa mémoire et ses outils. Sherka transmet tes messages et lit ses réponses avec VoiceStudio.</p>` : html`<p class="connection-note provider-privacy">Le fournisseur reçoit le prompt du compagnon, les échanges retenus et les souvenirs utilisés. La voix et les fichiers restent sur ton PC. Des frais d’API peuvent s’appliquer.</p>`}<div class="connection-address"><label>Adresse de base de l’API<input type="url" name="llm_api_url" list="hermes-servers" required maxlength="2048" value="${escape(d.values.llm_api_url ?? providerInfo[2])}" placeholder="https://example.com/v1" /></label>${refreshButton('lm')}</div><div class="provider-key"><label>Clé API<input id="provider-api-key" type="password" autocomplete="off" spellcheck="false" maxlength="2560" placeholder="Coller une nouvelle clé" /></label><div class="settings-actions"><button type="button" class="button secondary small" id="save-provider-key">Enregistrer la clé</button><button type="button" class="button secondary small" id="remove-provider-key" ${d.keyStored ? '' : 'disabled'}>Retirer la clé</button><span id="provider-key-status" class="connection-note">${d.keyStored === undefined ? t('Vérification du coffre…') : d.keyStored ? t('Clé protégée enregistrée pour cette adresse') : t('Aucune clé enregistrée pour cette adresse')}</span></div><p class="connection-note">La clé est protégée par Windows, partagée pour cette adresse et exclue des profils exportés.</p></div>`}
        ${agent ? html`<datalist id="hermes-servers">${(d.hermesServers ?? []).map(server => html`<option value="${escape(server)}"></option>`).join('')}</datalist>` : ''}${status('lm')}
        ${agent ? html`<label>Profil Hermes<input name="hermes_profile" list="hermes-profiles" required maxlength="64" pattern="(?:[A-Za-z0-9_]|-)+" value="${escape(d.values.hermes_profile ?? 'default')}" /><datalist id="hermes-profiles">${(d.hermesProfiles ?? ['default']).map(name => html`<option value="${escape(name)}"></option>`).join('')}</datalist></label><p class="connection-note">Choisis un profil enregistré ou saisis son nom exact. Chaque profil utilise sa propre clé API et sa propre conversation.</p>${d.hermesReady ? html`<p class="connection-status">Profil Hermes connecté · sessions et streaming disponibles</p>` : ''}
        <details class="language-help"><summary>Connecter ton serveur Hermes</summary><ol><li>Sur le serveur, crée une clé aléatoire avec cette commande :</li></ol><pre><code>python -c "import secrets; print(secrets.token_urlsafe(32))"</code></pre><p>Dans le fichier .env du profil Hermes, ajoute API_SERVER_ENABLED=true et API_SERVER_KEY=ta_nouvelle_cle. Le profil principal utilise ~/.hermes/.env ; un autre profil utilise ~/.hermes/profiles/nom/.env.</p><p>Pour un serveur sur ton réseau, règle API_SERVER_HOST sur son adresse IP privée. Le port habituel est 8642. Redémarre la passerelle Hermes pour appliquer ces réglages.</p><p>Pour plusieurs profils sur le même port, active gateway.multiplex_profiles: true dans la configuration Hermes et crée une clé propre à chaque profil. Un profil sur un port séparé utilise sa propre adresse avec le profil default ici.</p><p>Dans Sherka, renseigne l’adresse du serveur, choisis le profil, enregistre sa clé puis clique sur Tester et actualiser. Cette clé donne accès à Hermes : ce n’est pas une clé OpenRouter ou Z.ai.</p><p>La liste contient les profils déjà configurés dans Sherka. L’API de conversation Hermes ne publie pas d’annuaire des profils. La conversation est conservée sur le serveur ; Sherka crée un fil distinct de Telegram.</p></details>` : html`
        ${d.lm ? html`<p class="connection-status">${t`${d.lm.models.length} modèle(s) de conversation détecté(s)`}${local ? t` · ${d.lm.models.filter((m) => m.loaded).length} chargé(s)` : ''}${d.lm.excluded_models ? t` · ${d.lm.excluded_models} autre(s) modèle(s) écarté(s)` : ''}</p>` : ''}
        ${local ? html`<label>Modèle de conversation<select name="llm_model">${options(lmChoices, d.values.llm_model)}</select></label>` : html`<label>Modèle de conversation<input name="llm_model" list="provider-models" maxlength="512" value="${escape(d.values.llm_model ?? '')}" placeholder="Identifiant du modèle" /><datalist id="provider-models">${d.lm?.models.map(m => html`<option value="${escape(m.id)}">${escape(m.label)}</option>`).join('') ?? ''}</datalist></label><p class="connection-note">Choisis un modèle du catalogue ou saisis son identifiant. Si la limite de contexte est inconnue, Sherka utilise 64 000 tokens par défaut. Tu peux ajuster ce plafond.</p>`}
        <p class="connection-note" id="model-context">${local ? html`${model ? t`Maximum du modèle : ${model.max_context_length ? number(model.max_context_length) : t('inconnu')} tokens. Contexte chargé : ${model.loaded_context_length ? number(model.loaded_context_length) + t(' tokens') : t('à vérifier au chargement')}.` : t('Le contexte réel sera vérifié avant chaque réponse.')} Le changement de compagnon décharge son ancien modèle avant de charger le nouveau ; un modèle commun reste chargé.` : html`${t`Maximum du modèle : ${model?.max_context_length ? number(model.max_context_length) : t('inconnu')} tokens.`} Les modèles distants ne sont pas chargés sur ton GPU. Passer à un fournisseur distant décharge l’ancien modèle LM Studio.`}</p>
        <div id="reasoning-controls">${reasoningView(d.values, model)}</div>
        <details><summary>Budget de conversation</summary><div class="connection-selects"><label>Plafond de contexte (vide = automatique)<input name="context_limit" type="number" min="2048" max="2000000" step="1" value="${d.values.context_limit ?? ''}" placeholder="${local ? t('Contexte chargé dans LM Studio') : t('64 000 tokens par défaut')}" /></label><label>Tokens réservés à la réponse<input name="response_tokens" type="number" min="128" max="8192" step="1" value="${d.values.response_tokens}" /></label></div><p class="connection-note">${local ? t('Le plafond Sherka ne modifie pas le chargement dans LM Studio.') : t('Le plafond Sherka ne peut pas augmenter le contexte annoncé par le fournisseur.')} Le budget utilise une estimation prudente et une marge de sécurité ; les anciens échanges sont écartés si nécessaire, sans couper ton nouveau message.</p></details>`}</fieldset>
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
    const keyInput = root.querySelector<HTMLInputElement>('#provider-api-key');
    if (keyInput) { keyInput.dataset.scope = keyScope; keyInput.value = enteredKey; if (restoreKeyFocus) keyInput.focus(); }
    root.querySelector<HTMLSelectElement>('#connection-profile')!.addEventListener('change', (event) => select((event.target as HTMLSelectElement).value));
    root.querySelector<HTMLSelectElement>('#connection-profile')!.disabled = d.saving;
    root.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[name]').forEach((input) => { input.disabled = d.saving; });
    root.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[name]:not([name="reasoning"])').forEach((input) => input.addEventListener('input', () => {
      const key = input.name as keyof Connections;
      if (key === 'llm_provider') {
        d.values.llm_provider = input.value as LlmProvider;
        d.values.llm_api_url = d.values.llm_provider === 'hermes' ? 'http://127.0.0.1:8642' : null;
        d.values.hermes_profile ??= 'default'; d.hermesReady = false; d.hermesProfiles = undefined;
        d.values.llm_model = null; d.values.llm_model_key = null;
        d.values.context_limit = null; d.values.reasoning = null;
        ++d.requests.lm; d.loading.lm = false; delete d.lm; delete d.errors.lm;
        d.keyStored = undefined; d.keyRequest = (d.keyRequest ?? 0) + 1;
      } else if (key === 'hermes_profile') {
        d.values.hermes_profile = input.value; d.hermesReady = false; d.keyStored = undefined; d.keyRequest = (d.keyRequest ?? 0) + 1;
        ++d.requests.lm; d.loading.lm = false; delete d.errors.lm;
        root.querySelector<HTMLInputElement>('#provider-api-key')!.value = '';
        root.querySelector('#provider-key-status')!.textContent = t('Vérification du coffre…');
        root.querySelector<HTMLButtonElement>('#remove-provider-key')!.disabled = true;
      } else if (key === 'lm_studio_url' || key === 'voice_studio_url' || key === 'llm_api_url') {
        const service = key === 'voice_studio_url' ? 'voice' : 'lm';
        d.values[key] = input.value;
        if (service === 'lm') d.values.llm_model_key = null;
        ++d.requests[service]; d.loading[service] = false; delete d.errors[service]; delete d[service];
        // Selections refer to a particular service; they cannot be silently
        // reused for another URL. Keep the input mounted while typing.
        if (service === 'lm') { d.values.reasoning = null; d.values.llm_model_key = null; root.querySelector('#reasoning-controls')?.replaceChildren(); }
        const keys = service === 'lm' ? ['llm_model'] as const : ['tts_engine', 'tts_voice', 'stt_model'] as const;
        for (const name of keys) { d.values[name] = null; const control = root.querySelector<HTMLInputElement | HTMLSelectElement>(`[name="${name}"]`)!; if (control) control.value = ''; if (control instanceof HTMLSelectElement) control.innerHTML = options([], null); }
        if (key === 'llm_api_url') { d.values.context_limit = null; const limit = root.querySelector<HTMLInputElement>('[name="context_limit"]'); if (limit) limit.value = ''; d.hermesReady = false; d.hermesProfiles = undefined; d.keyStored = undefined; d.keyRequest = (d.keyRequest ?? 0) + 1; root.querySelector('#provider-models')?.replaceChildren(); root.querySelector('#provider-key-status')!.textContent = t('Vérification du coffre…'); root.querySelector<HTMLButtonElement>('#remove-provider-key')!.disabled = true; const keyInput = root.querySelector<HTMLInputElement>('#provider-api-key')!; keyInput.value = ''; keyInput.dataset.scope = JSON.stringify([d.values.llm_provider, d.values.llm_api_url, d.values.hermes_profile]); }
        const refresh = root.querySelector<HTMLButtonElement>(`[data-refresh="${service}"]`)!;
        refresh.disabled = false;
        refresh.textContent = t('Tester et actualiser');
        input.closest('fieldset')!.querySelectorAll('.connection-status,.connection-error,.model-inventory').forEach((el) => el.remove());
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
      if (key === 'tts_engine' || key === 'llm_model' && local || key === 'llm_provider') { paint(); if (key === 'llm_provider') { void checkKey(); void loadHermesServers(); void loadHermesProfiles(); } }
    }));
    root.querySelector<HTMLInputElement>('[name="llm_model"]')?.addEventListener('change', () => {
      if (!local) { const model = d.lm?.models.find(m => m.id === d.values.llm_model); root.querySelector('#reasoning-controls')!.innerHTML = reasoningView(d.values, model); root.querySelector('#model-context')!.innerHTML = html`${t`Maximum du modèle : ${model?.max_context_length ? number(model.max_context_length) : t('inconnu')} tokens.`} Les modèles distants ne sont pas chargés sur ton GPU. Passer à un fournisseur distant décharge l’ancien modèle LM Studio.`; bindReasoning(); }
    });
    root.querySelector<HTMLInputElement>('[name="llm_api_url"]')?.addEventListener('change', () => { void checkKey(); void loadHermesProfiles(); });
    root.querySelector<HTMLInputElement>('[name="hermes_profile"]')?.addEventListener('change', () => { paint(); void checkKey(); });
    root.querySelector<HTMLInputElement>('#provider-api-key')?.addEventListener('keydown', event => { if (event.key === 'Enter') { event.preventDefault(); root.querySelector<HTMLButtonElement>('#save-provider-key')?.click(); } });
    for (const [id, remove] of [['save-provider-key', false], ['remove-provider-key', true]] as const) {
      root.querySelector<HTMLButtonElement>(`#${id}`)?.addEventListener('click', async () => {
        if (!root.querySelector<HTMLInputElement>('[name="llm_api_url"]')!.reportValidity()) return;
        const input = root.querySelector<HTMLInputElement>('#provider-api-key')!;
        const key = remove ? null : input.value.trim(); input.value = '';
        const config = { ...d.values }; const token = d.keyRequest = (d.keyRequest ?? 0) + 1;
        root.querySelectorAll<HTMLButtonElement>('.provider-key button').forEach(button => button.disabled = true);
        try { await api.setProviderKey(config, key); if (token === d.keyRequest) { d.keyStored = !remove; d.hermesReady = false; delete d.errors.lm; delete d.lm; ++d.requests.lm; d.loading.lm = false; } }
        catch (error) { if (token === d.keyRequest) d.errors.lm = String(error); }
        finally { if (token === d.keyRequest) d.repaint?.(); }
      });
    }
    bindReasoning();
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
  function bindReasoning(): void {
    const select = root.querySelector<HTMLSelectElement>('[name="reasoning"]');
    if (!select) return;
    select.disabled = d.saving;
    select.oninput = () => { d.values.reasoning = select.value || null; d.dirty = true; root.querySelector<HTMLButtonElement>('#save-connections')!.disabled = false; root.querySelector('#connection-dirty')!.textContent = t('Modifications non enregistrées'); };
  }

  async function probe(service: Service): Promise<void> {
    const token = ++d.requests[service];
    const url = service === 'lm' ? d.values.lm_studio_url : d.values.voice_studio_url;
    d.loading[service] = true; delete d.errors[service]; delete d[service]; paint();
    try {
      if (service === 'lm' && d.values.llm_provider === 'hermes') { const result = await api.testHermes({ ...d.values }); if (token === d.requests.lm) { d.hermesProfiles = result; d.hermesReady = true; } }
      else if (service === 'lm') { const result = await api.discoverLanguageModels({ ...d.values }); if (token === d.requests[service]) d.lm = result; }
      else { const result = await api.discoverVoices(url); if (token === d.requests[service]) d.voice = result; }
    } catch (error) { if (token === d.requests[service]) d.errors[service] = String(error); }
    finally { if (token === d.requests[service]) { d.loading[service] = false; d.repaint?.(); } }
  }
  async function checkKey(): Promise<void> {
    if (!d.values.llm_provider || d.values.llm_provider === 'lm_studio') return;
    const token = d.keyRequest = (d.keyRequest ?? 0) + 1;
    try {
      const stored = await api.providerKeyStatus({ ...d.values });
      if (token !== d.keyRequest) return;
      d.keyStored = stored;
      if (root.dataset.profile === profile.id) {
        const status = root.querySelector('#provider-key-status');
        if (status) status.textContent = stored ? t('Clé protégée enregistrée pour cette adresse') : t('Aucune clé enregistrée pour cette adresse');
        const remove = root.querySelector<HTMLButtonElement>('#remove-provider-key'); if (remove) remove.disabled = !stored;
      }
    } catch (error) { if (token === d.keyRequest) { d.errors.lm = String(error); d.repaint?.(); } }
  }
  async function loadHermesServers(): Promise<void> {
    if (d.values.llm_provider !== 'hermes') return;
    const before = d.values.llm_api_url;
    try {
      const servers = await api.hermesServers();
      if (d.values.llm_provider !== 'hermes' || d.values.llm_api_url !== before) return;
      d.hermesServers = servers;
      if (d.dirty && before === 'http://127.0.0.1:8642' && servers.length === 1) {
        d.values.llm_api_url = servers[0]; d.keyStored = undefined;
        void checkKey(); void loadHermesProfiles();
      }
      d.repaint?.();
    } catch { /* Saved addresses are optional; manual setup remains available. */ }
  }
  async function loadHermesProfiles(): Promise<void> {
    if (d.values.llm_provider !== 'hermes' || !d.values.llm_api_url) return;
    const server = d.values.llm_api_url;
    try { const names = await api.hermesProfiles(server); if (d.values.llm_provider === 'hermes' && d.values.llm_api_url === server) { d.hermesProfiles = names; d.repaint?.(); } } catch { /* The address may still be incomplete while typing. */ }
  }
  d.repaint = paint;
  paint();
  if (!d.hermesServers) void loadHermesServers();
  if (!d.hermesProfiles) void loadHermesProfiles();
  if (d.keyStored === undefined) void checkKey();
  for (const service of ['lm', 'voice'] as const) if ((service === 'voice' || !d.values.llm_provider || d.values.llm_provider === 'lm_studio') && !d[service] && !d.loading[service] && !d.errors[service]) void probe(service);
}
