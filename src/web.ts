import { html, t, feedback } from './i18n';
import type { SearchMode, WebReport,SearchConfig,SearchProvider,Profile,Mutation } from './types';
import * as api from './api';

const escape = (text: string) => text.replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));

const defaults:SearchConfig={provider:'duck_duck_go',searxng_url:'http://127.0.0.1:8080',max_results:6};
const drafts=new Map<string,SearchConfig>();
export function globalWebSettings(saved:SearchConfig):string {
  const config=drafts.get("global")??{...defaults,...saved};
  return html`<section class="settings-card" id="web-settings"><h2>Recherche Internet</h2><p>Le moteur et les clés API sont communs à tous les compagnons. Hermès utilise ses propres outils.</p><div class="connection-selects"><label>Moteur de recherche<select id="search-provider"><option value="duck_duck_go" ${config.provider==='duck_duck_go'?'selected':''}>DuckDuckGo</option><option value="brave" ${config.provider==='brave'?'selected':''}>Brave Search API</option><option value="tavily" ${config.provider==='tavily'?'selected':''}>Tavily API</option><option value="searxng" ${config.provider==='searxng'?'selected':''}>SearXNG</option></select></label><label>Nombre maximal de résultats<input type="number" id="search-results" value="${config.max_results}" min="1" max="10" step="1" /></label></div><label id="searxng-settings" ${config.provider==='searxng'?'':'hidden'}>Adresse de SearXNG<input type="url" id="search-endpoint" value="${escape(config.searxng_url)}" /></label><div id="search-api-settings" ${['brave','tavily'].includes(config.provider)?'':'hidden'}><label>Clé API du moteur<input type="password" id="search-api-key" autocomplete="off" maxlength="2560" placeholder="Coller une nouvelle clé" /></label><div class="settings-actions"><button class="button secondary small" type="button" id="save-search-key">Enregistrer la clé</button><button class="button quiet small" type="button" id="remove-search-key">Retirer la clé</button><small id="search-key-status" role="status"></small></div></div><p>DuckDuckGo reste sans compte mais peut être bloqué. Brave et Tavily nécessitent ta propre clé ; des frais ou quotas peuvent s’appliquer. SearXNG utilise ton instance avec le format JSON activé.</p><small>La requête est transmise au moteur sélectionné. Les clés sont protégées par le coffre de ta session et exclues des exports.</small><div class="settings-actions"><button class="button primary small" type="button" id="save-search-config">Enregistrer le moteur</button><button class="button secondary small" type="button" id="test-search-config">Tester le moteur</button><span id="search-test-status" role="status"></span><span id="search-settings-error" role="alert"></span></div><small>Sur demande : utilise le bouton dans Conversation ou demande « cherche sur Internet ». Automatique : le modèle décide si une recherche est utile. Sa fiabilité dépend du modèle et ce mode ajoute un peu d’attente.</small></section>`;
}
export function mountGlobalWebSettings(language:string,save:(m:Mutation,success:string)=>Promise<boolean>):void{
 const root=document.querySelector('#web-settings');if(!root)return;
 const provider=root.querySelector<HTMLSelectElement>('#search-provider')!,results=root.querySelector<HTMLInputElement>('#search-results')!,endpoint=root.querySelector<HTMLInputElement>('#search-endpoint')!;
 const error=(e:unknown)=>{root.querySelector('#search-settings-error')!.textContent=feedback(String(e));};let request=0;
 const capture=()=>{const config={provider:provider.value as SearchProvider,max_results:Number(results.value),searxng_url:endpoint.value};drafts.set("global",config);return config;};
 const keyStatus=async()=>{const revision=++request,kind=provider.value as SearchProvider;try{const stored=await api.searchKeyStatus(kind);if(revision===request&&root.isConnected)root.querySelector('#search-key-status')!.textContent=stored?t('Clé protégée enregistrée pour cette adresse'):t('Aucune clé enregistrée pour cette adresse');}catch(e){error(e);}};
 provider.onchange=()=>{capture();root.querySelector<HTMLElement>('#searxng-settings')!.hidden=provider.value!=='searxng';root.querySelector<HTMLElement>('#search-api-settings')!.hidden=!['brave','tavily'].includes(provider.value);root.querySelector<HTMLInputElement>('#search-api-key')!.value='';void keyStatus();};results.oninput=capture;endpoint.oninput=capture;
 root.querySelector('#save-search-config')!.addEventListener('click',()=>{void save({kind:'search_config',config:capture()},t('Moteur de recherche enregistré.')).then(ok=>{if(ok)drafts.delete('global');});});
 root.querySelector('#save-search-key')!.addEventListener('click',()=>{const input=root.querySelector<HTMLInputElement>('#search-api-key')!,key=input.value;input.value='';void api.setSearchKey(provider.value as SearchProvider,key).then(keyStatus).catch(error);});
 root.querySelector<HTMLButtonElement>('#test-search-config')!.addEventListener('click',()=>void(async()=>{const button=root.querySelector<HTMLButtonElement>('#test-search-config')!,status=root.querySelector('#search-test-status')!;button.disabled=true;status.textContent=t('Test du moteur…');root.querySelector('#search-settings-error')!.textContent='';try{const count=await api.testSearchConfig(capture(),language);if(root.isConnected)status.textContent=t`Moteur accessible · ${count} résultats`;}catch(e){status.textContent='';error(e);}finally{button.disabled=false;}})());
 root.querySelector('#remove-search-key')!.addEventListener('click',()=>void api.setSearchKey(provider.value as SearchProvider,null).then(keyStatus).catch(error));
 if(['brave','tavily'].includes(provider.value))void keyStatus();
}


export function webSettings(mode:SearchMode,_profile:Profile):string {
 return html`<section class="settings-card" id="profile-web-settings"><h2>Recherche Internet</h2><label>Accès Internet<select id="web-search-mode"><option value="off" ${mode==='off'?'selected':''}>Désactivé</option><option value="on_request" ${mode==='on_request'?'selected':''}>Sur demande</option><option value="auto" ${mode==='auto'?'selected':''}>Automatique</option></select></label><p>Le moteur de recherche et les clés API se configurent dans les réglages du logiciel.</p><button class="button primary small" id="save-web-mode">Enregistrer</button></section>`;
}
export function mountWebSettings(profile:Profile,current:()=>Profile|undefined,save:(m:Mutation,success:string)=>Promise<boolean>):void {
 const root=document.querySelector('#profile-web-settings');if(!root)return;
 root.querySelector('#save-web-mode')!.addEventListener('click',()=>{const p=current();if(!p)return;void save({kind:'web_search',id:profile.id,revision:p.revision,mode:root.querySelector<HTMLSelectElement>('#web-search-mode')!.value as SearchMode},t('Accès Internet enregistré.'));});
}

export function webReport(report: WebReport | null, profileId: string, messageId: string | null): string {
  if (!report) return '';
  if (report.searching) return html`<p class="web-search-status" role="status">Recherche sur Internet… <span>${escape(report.query)}</span></p>`;
  const message = report.error ? feedback(report.error) : report.sources.length ? '' : t('Aucun résultat trouvé sur Internet.');
  return html`<section class="web-report"><strong>Résultats Internet</strong><small>${escape(report.query)}</small>${message?html`<p class="web-search-status">${escape(message)}</p>`:''}${report.sources.length ? html`<details class="web-sources" open><summary>Sources de la recherche</summary><p>Extraits du moteur de recherche ; les pages complètes n’ont pas été lues. Clique sur une source pour l’ouvrir dans ton navigateur.</p><ol>${report.sources.map((source,index)=>html`<li><button type="button" class="web-source" data-web-source="${index}" data-source-profile="${profileId}" ${messageId?html`data-source-message="${messageId}"`:''}>${escape(source.title)}</button><small>${escape(new URL(source.url).hostname)}</small><p>${escape(source.snippet)}</p></li>`).join('')}</ol></details>` : ''}</section>`;
}
