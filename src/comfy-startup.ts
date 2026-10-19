import * as api from './api';
import {html,t,feedback} from './i18n';
import type {ComfyStartupConfig,ComfyServiceStatus,Mutation} from './types';
const escape=(text:string)=>text.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
let service:ComfyServiceStatus|undefined;

export function localComfyServer(value:string):boolean {
  try{const url=new URL(value);return url.protocol==='http:'&&(url.hostname==='localhost'||url.hostname==='[::1]'||/^127\.\d+\.\d+\.\d+$/.test(url.hostname))&&!url.username&&!url.password&&url.pathname==='/'&&!url.search&&!url.hash;}catch{return false;}
}
export function comfyStartupMessage(server:string):string|undefined {
  try{if(service&&new URL(service.url).origin===new URL(server).origin&&['connecting','starting','error'].includes(service.phase))return feedback(service.message);}catch{/* No service status for incomplete addresses. */}
}
export async function mountComfyService():Promise<void> {
  await api.onComfyService(value=>{service=value;paintComfyService();document.dispatchEvent(new Event('sherka:comfy-service'));});
  service=await api.comfyService();paintComfyService();
}
export function paintComfyService():void {
  const root=document.querySelector<HTMLElement>('#illustration-settings')??document.querySelector<HTMLElement>('#comfy-startup-options');if(!root)return;
  const server=root.querySelector<HTMLInputElement>('[name="image_server"]')?.value??'';
  const local=!server||localComfyServer(server);
  const fields=root.querySelector<HTMLFieldSetElement>('#comfy-local-fields');if(fields)fields.disabled=!local;
  const remote=root.querySelector<HTMLElement>('#comfy-remote-note');if(remote)remote.hidden=local;
  const status=root.querySelector<HTMLElement>('#comfy-startup-status');
  if(status){status.textContent=comfyStartupMessage(server)??'';status.hidden=!status.textContent;status.classList.toggle('form-error',service?.phase==='error');}
}
export function comfyStartupOptions(config:ComfyStartupConfig):string {
  return html`<section class="settings-card comfy-startup-options" id="comfy-startup-options"><h2>Démarrage local de ComfyUI</h2><label class="setting-row"><span>Démarrer avec Sherka</span><input type="checkbox" class="switch" id="comfy-autostart" ${config.disabled?'':'checked'} /></label><p>Les serveurs locaux lancés par Sherka se ferment avec elle. Un serveur déjà ouvert ou distant reste indépendant.</p><p>Facultatif, sur ce PC : si le chemin est renseigné, Sherka démarre ComfyUI quand les illustrations sont activées. Ces chemins sont partagés entre tes compagnons et exclus des exports.</p><p id="comfy-remote-note" hidden>Serveur distant : le démarrage automatique ne s’applique pas.</p><fieldset id="comfy-local-fields"><label>Dossier ComfyUI<div class="comfy-path"><input id="comfy-directory" type="text" maxlength="4096" value="${escape(config.directory)}" /><button type="button" class="button secondary small" id="pick-comfy-directory">Parcourir</button></div></label><label>Python de ComfyUI<div class="comfy-path"><input id="comfy-python" type="text" maxlength="4096" value="${escape(config.python)}" /><button type="button" class="button secondary small" id="pick-comfy-python">Parcourir</button></div><small>Facultatif avec un environnement .venv ou une installation portable. Sinon, indique le Python de ton environnement ComfyUI, par exemple Conda.</small></label><p>Choisis le dossier contenant main.py, ou la racine de l’installation portable. Laisse le dossier vide pour désactiver ce lancement.</p><button type="button" class="button secondary" id="save-comfy-startup">Enregistrer le démarrage local</button></fieldset><p>Les premières générations peuvent prendre plus de temps : ComfyUI doit démarrer et charger ses modèles. </p><p id="comfy-path-error" role="alert"></p></section>`;
}
export function mountComfyOptions(root:HTMLElement,save:(mutation:Mutation,message:string)=>Promise<boolean>):void {
  const field=(id:string)=>root.querySelector<HTMLInputElement>('#'+id)!;
  const error=(e:unknown)=>{root.querySelector('#comfy-path-error')!.textContent=feedback(String(e));};
  for(const [id,python] of [['directory',false],['python',true]] as const)root.querySelector('#pick-comfy-'+id)!.addEventListener('click',()=>void api.selectComfyPath(python).then(path=>{if(path)field('comfy-'+id).value=path;}).catch(error));
  root.querySelector('#save-comfy-startup')!.addEventListener('click',()=>void save({kind:'comfyui_startup',config:{directory:field('comfy-directory').value,python:field('comfy-python').value,disabled:!field('comfy-autostart').checked}},t('Démarrage local de ComfyUI enregistré.')));
  root.querySelector('[name="image_server"]')?.addEventListener('input',paintComfyService);
  paintComfyService();
}
