import {html,t} from './i18n';
import type {TrioSettings,Mutation} from './types';
const delays=[0,5,8,10,15];
const escape=(s:string)=>s.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
export function trioSettings(c:TrioSettings):string {
 return html`<section class="settings-card" id="trio-settings"><h2>Conversations à trois</h2><label>Ton prénom<input type="text" name="user_name" maxlength="40" autocomplete="off" value="${escape(c.user_name)}" /></label><p>Tes compagnons l’utilisent pour savoir que c’est toi qui parles en conversation à trois. Laisse vide pour « l’utilisateur ».</p><label>Relance de l’autre compagnon<select name="relay_seconds"><option value="0" ${c.relay_seconds===0?'selected':''}>Désactivée</option><option value="5" ${c.relay_seconds===5?'selected':''}>Après 5 secondes</option><option value="8" ${c.relay_seconds===8?'selected':''}>Après 8 secondes</option><option value="10" ${c.relay_seconds===10?'selected':''}>Après 10 secondes</option><option value="15" ${c.relay_seconds===15?'selected':''}>Après 15 secondes</option></select></label><p>Quand tu parles, un seul compagnon répond ; si tu restes silencieux pendant ce délai, l’autre réagit, puis ils discutent entre eux jusqu’à ce que tu parles ou écrives (20 répliques au plus).</p><label class="setting-row"><span><strong>Favoriser des réponses rapides</strong><small>En conversation à trois, les compagnons répondent en une ou deux phrases courtes : la voix démarre plus vite et les échanges s’enchaînent.</small></span><input class="switch" type="checkbox" name="quick_replies" ${c.quick_replies?'checked':''} /></label></section>`;
}
export function mountTrioSettings(c:TrioSettings,save:(m:Mutation,message:string)=>Promise<boolean>):void {
 const root=document.querySelector<HTMLElement>('#trio-settings');if(!root)return;
 const select=root.querySelector<HTMLSelectElement>('select[name="relay_seconds"]')!,quick=root.querySelector<HTMLInputElement>('input[name="quick_replies"]')!,name=root.querySelector<HTMLInputElement>('input[name="user_name"]')!;
 // A delay saved outside the offered steps (3..30) stays selectable until changed.
 if(!delays.includes(c.relay_seconds)){const o=document.createElement('option');o.value=String(c.relay_seconds);o.textContent=t`Après ${c.relay_seconds} secondes`;o.selected=true;select.append(o);}
 const submit=()=>void save({kind:'trio_settings',config:{relay_seconds:Number(select.value),quick_replies:quick.checked,user_name:name.value.trim()}},t('Réglages des conversations à trois enregistrés.'));
 select.onchange=submit;quick.onchange=submit;name.onchange=submit;
}
