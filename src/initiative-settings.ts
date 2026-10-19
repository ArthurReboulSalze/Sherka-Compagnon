import {html,t} from './i18n';
import type {Profile,Mutation} from './types';
export function initiativeSettings(p:Profile):string {
 if(p.connections.llm_provider==='hermes')return '';
 const c=p.initiative??{enabled:false,min_minutes:5,max_minutes:15};
 return html`<section class="settings-card"><h2>Prise de parole spontanée</h2><form id="initiative-form"><label class="setting-row"><span><strong>Engager la conversation après un silence</strong><small>Le compagnon choisit un délai au hasard dans cette fourchette. L’activité dans Sherka et la conversation repoussent son intervention.</small></span><input class="switch" type="checkbox" name="initiative_enabled" ${c.enabled?'checked':''} /></label><div class="connection-selects"><label>Délai minimum (minutes)<input type="number" name="initiative_min" min="1" max="1440" required value="${c.min_minutes}" /></label><label>Délai maximum (minutes)<input type="number" name="initiative_max" min="1" max="1440" required value="${c.max_minutes}" /></label></div><p>Une courte invitation, puis il attend ta réponse. Aucun déclenchement pendant la dictée, une réponse, une pause vocale ou lorsque le widget est masqué. Les fournisseurs distants peuvent facturer cette réponse.</p><button type="submit" class="button secondary">Enregistrer la prise de parole</button></form></section>`;
}
export function mountInitiativeSettings(p:Profile,current:()=>Profile|undefined,save:(m:Mutation,message:string)=>Promise<boolean>):void {
 const form=document.querySelector<HTMLFormElement>('#initiative-form');if(!form)return;
 form.onsubmit=e=>{e.preventDefault();const profile=current();if(!profile)return;const data=new FormData(form);void save({kind:'initiative',id:p.id,revision:profile.revision,config:{enabled:data.has('initiative_enabled'),min_minutes:Number(data.get('initiative_min')),max_minutes:Number(data.get('initiative_max'))}},t('Prise de parole spontanée enregistrée.'));};
}
