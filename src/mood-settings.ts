import {html,t} from './i18n';
import type {Mutation,Profile} from './types';

export function moodSettings(profile:Profile):string {
  if(profile.connections.llm_provider==='hermes')return '';
  return html`<section class="settings-card mood-settings"><h2>Humeurs du compagnon</h2><label class="setting-row"><span><strong>Activer la persistance des humeurs</strong><small>Les émotions s’accumulent au fil des échanges et retombent progressivement. Une interaction positive peut apaiser une colère. L’humeur est propre à ce compagnon et conservée au redémarrage.</small></span><input type="checkbox" class="switch" id="mood-persistence" ${profile.mood_persistence?'checked':''} /></label><div class="settings-actions"><button type="button" class="button secondary small" id="reset-mood-settings">Réinitialiser l’humeur</button><small>Remet les points à zéro sans effacer les messages ni les faits sauvegardés.</small></div></section>`;
}

export function mountMoodSettings(profile:Profile,current:()=>Profile|undefined,save:(mutation:Mutation,message:string)=>Promise<boolean>):void {
  document.querySelector<HTMLInputElement>('#mood-persistence')?.addEventListener('change',event=>{
    const p=current();if(!p)return;
    void save({kind:'mood_persistence',id:profile.id,revision:p.revision,enabled:(event.target as HTMLInputElement).checked},t('Réglage des humeurs enregistré.'));
  });
  document.querySelector('#reset-mood-settings')?.addEventListener('click',()=>void save({kind:'reset_mood',id:profile.id},t('Humeur réinitialisée.')));
}
