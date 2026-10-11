import { html, t, feedback } from './i18n';
import type { SearchMode, WebReport } from './types';

const escape = (text: string) => text.replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));

export function webSettings(mode: SearchMode): string {
  return html`<section class="settings-card"><h2>Recherche Internet</h2><label>Accès Internet<select id="web-search-mode"><option value="off" ${mode==='off'?'selected':''}>Désactivé</option><option value="on_request" ${mode==='on_request'?'selected':''}>Sur demande</option><option value="auto" ${mode==='auto'?'selected':''}>Automatique</option></select></label><p>DuckDuckGo, sans compte ni clé API. La requête de recherche est envoyée sur Internet ; les souvenirs et l’historique ne sont pas joints.</p><small>Sur demande : utilise le bouton dans Conversation ou demande « cherche sur Internet ». Automatique : le modèle décide si une recherche est utile. Sa fiabilité dépend du modèle et ce mode ajoute un peu d’attente.</small><p>Les réponses utilisent des extraits de résultats, avec des sources à ouvrir. DuckDuckGo peut demander une vérification anti-robot.</p></section>`;
}

export function webReport(report: WebReport | null, profileId: string, messageId: string | null): string {
  if (!report) return '';
  if (report.searching) return html`<p class="web-search-status" role="status">Recherche sur Internet… <span>${escape(report.query)}</span></p>`;
  const message = report.error ? feedback(report.error) : report.sources.length ? '' : t('Aucun résultat trouvé sur Internet.');
  return html`<section class="web-report"><strong>Résultats Internet</strong><small>${escape(report.query)}</small>${message?html`<p class="web-search-status">${escape(message)}</p>`:''}${report.sources.length ? html`<details class="web-sources" open><summary>Sources de la recherche</summary><p>Extraits du moteur de recherche ; les pages complètes n’ont pas été lues. Clique sur une source pour l’ouvrir dans ton navigateur.</p><ol>${report.sources.map((source,index)=>html`<li><button type="button" class="web-source" data-web-source="${index}" data-source-profile="${profileId}" ${messageId?html`data-source-message="${messageId}"`:''}>${escape(source.title)}</button><small>${escape(new URL(source.url).hostname)}</small><p>${escape(source.snippet)}</p></li>`).join('')}</ol></details>` : ''}</section>`;
}
