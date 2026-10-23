import * as api from './api';
import {html,t,feedback,language,languageOptions,type Language} from './i18n';
import type {Profile,CharacterDraft,CharacterStudioStatus,CharacterStudioConfig,CharacterStudioCatalog,CharacterStage,Emotion} from './types';

const escape=(s:string)=>s.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
const emotions:Emotion[]=['neutral','joy','sadness','anger','surprise','reflection','tenderness'];
const labels=()=>[t('Neutre'),t('Joie'),t('Tristesse'),t('Colère'),t('Surprise'),t('Réflexion'),t('Tendresse')];
const storageKey='sherka:studio-models:v1';
let status:CharacterStudioStatus={sequence:0,revision:0,busy:false,phase:'',source_profile:null,draft:null,config:null,images:{},transparent_images:{},neutral_validated:false,neutral_imported:false,note:null,target_profile:null,error:null,created_profile:null};
let draft:CharacterDraft|null=null,config:CharacterStudioConfig|undefined,concept='',sourceId='',chosenLanguage:Language|undefined;
// Optional picture for a new sheet: it becomes the neutral portrait.
let seed:{name:string;data:string}|null=null;
let pending=false,initialized=false,subscription:Promise<unknown>|undefined;
let localError:string|null=null;
let context:{root:HTMLElement;profiles:Profile[];created:(id:string)=>void}|undefined;
const catalogs=new Map<string,CharacterStudioCatalog>();
const same=(a:unknown,b:unknown):boolean=>{
 const ordered=(v:unknown):unknown=>Array.isArray(v)?v.map(ordered):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>[k,ordered(v)])):v;
 return JSON.stringify(ordered(a))===JSON.stringify(ordered(b));
};
const server=()=>config!.server_url.trim().replace(/\/+$/,'');
const edited=(d:CharacterDraft|null,e:Emotion)=>d?.prompt_overrides?.[e];
// An imported neutral portrait does not depend on the sheet or the checkpoint.
const portraitMatches=()=>!!draft&&!!status.draft&&!!config&&!!status.config&&(status.neutral_imported||
 ['physical_description','appearance_negative_prompt','portrait_prompt'].every(k=>draft![k as keyof CharacterDraft]===status.draft![k as keyof CharacterDraft])&&
 same(edited(draft,'neutral'),edited(status.draft,'neutral'))&&
 ['server_url','checkpoint','model_family'].every(k=>config![k as keyof CharacterStudioConfig]===status.config![k as keyof CharacterStudioConfig]));
const moodMatches=(e:Emotion)=>portraitMatches()&&
 ['server_url','mood_model','text_encoder','vae'].every(k=>config![k as keyof CharacterStudioConfig]===status.config![k as keyof CharacterStudioConfig])&&
 draft!.appearance_negative_prompt===status.draft!.appearance_negative_prompt&&same(edited(draft,e),edited(status.draft,e))&&
 draft!.emotion_prompts[e as Exclude<Emotion,'neutral'>]===status.draft!.emotion_prompts[e as Exclude<Emotion,'neutral'>];
const imageTypes=['image/png','image/jpeg','image/webp'];
function readImage(file:File):Promise<string> {
 if(!imageTypes.includes(file.type)||file.size>10*1024*1024)return Promise.reject(new Error(t('Choisis une image PNG, JPEG ou WebP de moins de 10 Mio.')));
 return new Promise((done,fail)=>{const reader=new FileReader();reader.onload=()=>done(String(reader.result));reader.onerror=()=>fail(new Error(t('Impossible de lire ce fichier.')));reader.readAsDataURL(file);});
}
function saveConfig():void {try{localStorage.setItem(storageKey,JSON.stringify(config));}catch{/* Storage may be full. */}}
function cachedCatalog():CharacterStudioCatalog|undefined {
 const key=server();if(catalogs.has(key))return catalogs.get(key);
 try{const value=JSON.parse(localStorage.getItem(storageKey+':catalog:'+key)??'null') as CharacterStudioCatalog|null;
  if(value&&['checkpoints','mood_models','text_encoders','vaes'].every(k=>Array.isArray(value[k as keyof CharacterStudioCatalog]))){
   catalogs.set(key,value);return value;
  }
 }catch{/* An unavailable cache never blocks refresh. */}
 return undefined;
}
export function characterStudioView():string {
 return html`<div class="page-heading"><div><p class="eyebrow">DE L’IDÉE AU COMPAGNON</p><h1>Création de personnage<span>.</span></h1><p>Prépare sa fiche, valide son portrait et ses émotions, puis applique la transparence.</p></div></div><div id="character-studio"><p>Chargement…</p></div>`;
}
export function mountCharacterStudio(profiles:Profile[],selected:Profile,created:(id:string)=>void):void {
 const root=document.querySelector<HTMLElement>('#character-studio');if(!root)return;
 context={root,profiles,created};
 if(!profiles.some(p=>p.connections.llm_provider!=='hermes')){
  context=undefined;root.innerHTML=html`<p class="connection-note">Choisis un compagnon classique avec un modèle connecté pour créer un personnage.</p>`;return;
 }
 if(!sourceId)sourceId=(selected.connections.llm_provider!=='hermes'?selected:profiles.find(p=>p.connections.llm_provider!=='hermes'))?.id??'';
 ensureConfig(profiles);
 chosenLanguage??=language();
 if(!subscription)subscription=api.onCharacterStudio(receive);
 if(!initialized)void api.characterStudio().then(s=>{initialized=true;receive(s,true);}).catch(showError);
 render();
}
function ensureConfig(profiles:Profile[]):void {
 if(config)return;
 const image=profiles.find(p=>p.id===sourceId)?.illustration;
 config={server_url:image?.server_url??'http://127.0.0.1:8188',checkpoint:image?.checkpoint??null,model_family:image?.model_family??'sdxl',mood_model:null,text_encoder:null,vae:null};
 try{const saved=JSON.parse(localStorage.getItem(storageKey)??'null');
  if(saved&&typeof saved.server_url==='string'&&['sdxl','illustrious'].includes(saved.model_family)&&['checkpoint','mood_model','text_encoder','vae'].every(k=>saved[k]===null||typeof saved[k]==='string'))config=saved;
 }catch{/* Use the connected companion's server on first use. */}
}
// Load an existing companion's images into the studio to regenerate some of them.
export async function editCompanionImages(profiles:Profile[],profile:Profile):Promise<void> {
 sourceId=profile.id;ensureConfig(profiles);seed=null;
 await api.editCharacterImages(profile.id,config!);
 receive(await api.characterStudio(),true);
}
function receive(next:CharacterStudioStatus,initial=false):void {
 if(next.sequence<status.sequence)return;
 const newDraft=next.draft&&(initial||next.source_profile!==status.source_profile||!same(next.draft,status.draft));
 status=next;
 if(newDraft){draft=structuredClone(next.draft);config=structuredClone(next.config!);sourceId=next.source_profile!;chosenLanguage=draft!.language;saveConfig();render();}
 else paint();
}
function showError(error:unknown):void {
 localError=feedback(String(error));const node=context?.root.querySelector('#studio-error');if(node)node.textContent=localError;
}
async function run(action:()=>Promise<unknown>):Promise<void> {
 if(pending||status.busy)return;
 localError=null;pending=true;paint();
 try{await action();receive(await api.characterStudio());}
 catch(error){showError(error);}
 finally{pending=false;paint();}
}
function modelOptions(names:string[],chosen:string|null):string {
 const values=[...new Set([...(chosen?[chosen]:[]),...names])];
 return `<option value="">${escape(t('Choisir…'))}</option>`+values.map(name=>`<option value="${escape(name)}" ${name===chosen?'selected':''}>${escape(name)}</option>`).join('');
}
function gallery(list:Emotion[],transparent=false):string {
 return `<div class="studio-gallery ${list.length===1?'studio-neutral-gallery':''}">`+list.map(e=>{
  const label=labels()[emotions.indexOf(e)];
  const actions=transparent?'':(e==='neutral'?'':html`<button class="button secondary small" data-regenerate="${e}">Générer</button>`)+html`<button class="button quiet small" data-prompt="${e}">Prompt</button>`;
  return html`<article data-emotion="${e}" data-transparent="${transparent}"><h3>${label}</h3><button type="button" class="studio-preview" aria-label="${label}" disabled><img alt="${label}" hidden/><span>À générer</span></button>${actions?`<div class="studio-card-actions">${actions}</div>`:''}</article>`;
 }).join('')+'</div>';
}
function render():void {
 const ctx=context;if(!ctx?.root.isConnected||!config)return;
 const source=ctx.profiles.find(p=>p.id===sourceId&&p.connections.llm_provider!=='hermes')??ctx.profiles.find(p=>p.connections.llm_provider!=='hermes');if(!source)return;
 sourceId=source.id;
 const c=config,d=draft,cat=cachedCatalog();
 ctx.root.innerHTML=html`
 <section class="settings-card"><h2>Imagine ton personnage</h2><p>Le modèle du compagnon choisi prépare une fiche modifiable, sans utiliser sa conversation ni ses souvenirs.</p>
 <form id="studio-concept"><div class="connection-selects"><label>Modèle connecté via<select name="source">${ctx.profiles.filter(p=>p.connections.llm_provider!=='hermes').map(p=>`<option value="${p.id}" ${p.id===sourceId?'selected':''}>${escape(p.name)} · ${escape(p.connections.llm_model??'')}</option>`).join('')}</select></label>
 <label>Langue du personnage<select name="language">${languageOptions(chosenLanguage!)}</select></label></div>
 <label>Ton idée<textarea name="concept" rows="3" maxlength="6000" placeholder="Une exploratrice robotique au visage de métal, curieuse et sarcastique…">${escape(concept)}</textarea></label>
 <div class="studio-seed"><img id="studio-seed-preview" alt="" hidden/><div><div class="settings-actions"><button type="button" class="button secondary" id="studio-seed-pick">Partir d’une image</button><button type="button" class="button quiet small" id="studio-seed-clear" hidden>Retirer l’image</button><span id="studio-seed-name"></span></div>
 <p class="connection-note">Facultatif. Ton image devient le portrait neutre. Si le modèle connecté voit les images, il s’en sert aussi pour rédiger la description physique.</p></div><input type="file" id="studio-seed-file" accept="image/png,image/jpeg,image/webp" hidden/></div>
 <button class="button primary" type="submit" id="studio-plan">Proposer une fiche</button></form></section>
 <section class="settings-card"><h2>Modèles de création</h2><form id="studio-models">
 <label>Serveur ComfyUI<input name="server_url" type="url" value="${escape(c.server_url)}"/></label>
 <div class="settings-actions"><button class="button primary" type="button" id="studio-refresh">Actualiser les modèles</button><span id="studio-catalog-note">${cat?t('Catalogue mémorisé.'):''}</span></div>
 <div class="connection-selects"><label>Modèle du portrait<select name="checkpoint">${modelOptions(cat?.checkpoints??[],c.checkpoint)}</select></label>
 <label>Famille du modèle<select name="model_family"><option value="sdxl" ${c.model_family==='sdxl'?'selected':''}>SDXL</option><option value="illustrious" ${c.model_family==='illustrious'?'selected':''}>Illustrious</option></select></label></div>
 <p class="connection-note">Le checkpoint du portrait contient son encodeur de texte et son VAE. Les émotions utilisent les trois modèles Klein ci-dessous.</p>
 <div class="connection-selects"><label>Modèle Klein 4B<select name="mood_model">${modelOptions(cat?.mood_models??[],c.mood_model)}</select></label>
 <label>Encodeur de texte Klein<select name="text_encoder">${modelOptions(cat?.text_encoders??[],c.text_encoder)}</select></label>
 <label>VAE Klein<select name="vae">${modelOptions(cat?.vaes??[],c.vae)}</select></label></div>
 <p class="connection-note">Les réglages des trois workflows sont fixes. Le détourage utilise RMBG-2.0, sans réglage à choisir.</p>
 <p id="studio-missing" class="connection-note"></p></form></section>
 ${d?html`<section class="settings-card"><h2>Affine sa fiche</h2><form id="studio-sheet"><div class="connection-selects"><label>Nom<input name="name" maxlength="60" required value="${escape(d.name)}"/></label>
 <label>Couleur<select name="accent">${['amber','teal','violet','coral'].map((a,i)=>`<option value="${a}" ${d.accent===a?'selected':''}>${[t('Ambre'),t('Turquoise'),t('Violet'),t('Corail')][i]}</option>`).join('')}</select></label></div>
 <label>Personnalité<textarea name="personality" rows="4" maxlength="12000">${escape(d.personality)}</textarea></label>
 <label>Instructions<textarea name="instructions" rows="3" maxlength="12000">${escape(d.instructions)}</textarea></label>
 <label>Description physique du compagnon<textarea name="physical_description" rows="3" maxlength="2400">${escape(d.physical_description)}</textarea></label>
 <label>À éviter pour le compagnon (prompt négatif)<textarea name="appearance_negative_prompt" rows="2" maxlength="2400">${escape(d.appearance_negative_prompt)}</textarea></label>
 <label>Pose et cadrage du portrait<textarea name="portrait_prompt" rows="3" maxlength="2400">${escape(d.portrait_prompt)}</textarea></label>
 <p class="connection-note">De face, tête et buste sur un fond gris foncé uni. Klein conserve le personnage, le cadrage et ce fond pour chaque émotion.</p>
 <details><summary>Expressions à générer</summary>${emotions.slice(1).map((e,i)=>html`<label>${labels()[i+1]}<textarea name="${e}" rows="2" maxlength="800">${escape(d.emotion_prompts[e as Exclude<Emotion,'neutral'>])}</textarea></label>`).join('')}</details></form>
 <p class="connection-note">Descriptions en anglais : tags pour Illustrious, phrases pour SDXL et instructions de modification pour Klein.</p></section>`:''}
 <section class="settings-card studio-progress"><p id="studio-target" class="studio-target" hidden></p><div class="section-heading"><p id="studio-phase" role="status"></p><button type="button" class="button secondary" id="studio-stop" hidden>Arrêter</button></div><p id="studio-error" class="form-error" role="alert"></p><p id="studio-note" class="connection-note"></p><p id="studio-changed" class="connection-note" hidden>La fiche ou les réglages ont changé. Régénère et valide le portrait neutre avant de continuer.</p></section>
 <section class="settings-card"><div class="section-heading"><h2>1. Portrait neutre</h2><button class="button secondary small" data-workflow="neutral">Voir le workflow</button></div>
 <p>Génère le portrait HD sur fond gris, ramené à 1024 pixels. Valide son apparence avant de passer aux émotions.</p>
 <div class="settings-actions"><button class="button primary" id="studio-neutral">Générer le portrait neutre</button><button class="button secondary" id="studio-import">Utiliser mon image</button><button class="button secondary" id="studio-validate">Valider ce portrait</button><input type="file" id="studio-import-file" accept="image/png,image/jpeg,image/webp" hidden/></div>
 <p class="connection-note">Ta propre image est placée sur un fond gris foncé uni et remplace le portrait généré. Elle reste valable si tu modifies la fiche ou le modèle du portrait.</p>${gallery(['neutral'])}</section>
 <section class="settings-card"><div class="section-heading"><h2>2. Émotions avec Klein</h2><button class="button secondary small" data-workflow="mood">Voir le workflow</button></div>
 <p>Génère ou recommence chaque expression séparément. Clique sur une image pour la voir en grand.</p>
 <button class="button primary" id="studio-emotions">Générer les émotions manquantes</button>${gallery(emotions.slice(1))}</section>
 <section class="settings-card"><div class="section-heading"><h2>3. Transparence</h2><button class="button secondary small" data-workflow="transparency">Voir le workflow</button></div>
 <p>Quand les sept portraits te conviennent, applique le détourage. Les images d’origine restent disponibles pour recommencer une émotion.</p>
 <button class="button primary" id="studio-transparency">Appliquer la transparence</button>${gallery(emotions,true)}
 <div class="settings-actions"><button class="button primary" id="studio-finish">Ajouter aux compagnons</button><span id="studio-finish-note">La voix et les connexions se règlent ensuite dans Compagnons.</span></div></section>`;
 const idea=ctx.root.querySelector<HTMLFormElement>('#studio-concept')!;
 idea.oninput=()=>{concept=(idea.elements.namedItem('concept') as HTMLTextAreaElement).value;sourceId=(idea.elements.namedItem('source') as HTMLSelectElement).value;chosenLanguage=(idea.elements.namedItem('language') as HTMLSelectElement).value as Language;};
 idea.onsubmit=e=>{e.preventDefault();
  if(!concept.trim()&&!seed){showError(t('Décris ton personnage ou ajoute une image.'));return;}
  void run(()=>api.planCharacter(sourceId,concept,chosenLanguage!,config!,seed?.data??null));};
 const seedFile=ctx.root.querySelector<HTMLInputElement>('#studio-seed-file')!;
 ctx.root.querySelector('#studio-seed-pick')!.addEventListener('click',()=>seedFile.click());
 seedFile.onchange=()=>void(async()=>{const file=seedFile.files?.[0];seedFile.value='';if(!file)return;
  try{seed={name:file.name,data:await readImage(file)};localError=null;paint();}catch(error){showError(error);}})();
 ctx.root.querySelector('#studio-seed-clear')!.addEventListener('click',()=>{seed=null;paint();});
 const importFile=ctx.root.querySelector<HTMLInputElement>('#studio-import-file')!;
 ctx.root.querySelector('#studio-import')!.addEventListener('click',()=>importFile.click());
 importFile.onchange=()=>void(async()=>{const file=importFile.files?.[0];importFile.value='';if(!file)return;
  try{const data=await readImage(file);await run(()=>api.importCharacterNeutral(data));}catch(error){showError(error);}})();
 ctx.root.querySelectorAll<HTMLButtonElement>('[data-prompt]').forEach(b=>b.onclick=()=>void editPrompt(b.dataset.prompt as Emotion).catch(showError));
 const models=ctx.root.querySelector<HTMLFormElement>('#studio-models')!;
 models.onsubmit=e=>e.preventDefault();
 models.oninput=()=>{
  const f=(n:string)=>(models.elements.namedItem(n) as HTMLInputElement).value;
  config={server_url:f('server_url').trim().replace(/\/+$/,''),checkpoint:f('checkpoint')||null,model_family:f('model_family') as CharacterStudioConfig['model_family'],mood_model:f('mood_model')||null,text_encoder:f('text_encoder')||null,vae:f('vae')||null};saveConfig();paint();
 };
 ctx.root.querySelector('#studio-refresh')!.addEventListener('click',()=>void run(async()=>{
  const url=server(),catalog=await api.discoverCharacterModels(url);
  catalogs.set(url,catalog);try{localStorage.setItem(storageKey+':catalog:'+url,JSON.stringify(catalog));}catch{/* Keep in-memory cache. */}
  const defaults:[keyof Pick<CharacterStudioConfig,'checkpoint'|'mood_model'|'text_encoder'|'vae'>,string[],string][]=[
   ['checkpoint',catalog.checkpoints,'SDXL\\juggernautXL_juggXIByRundiffusion.safetensors'],
   ['mood_model',catalog.mood_models,'flux-2-klein-4b-fp8.safetensors'],
   ['text_encoder',catalog.text_encoders,'qwen3vl_4b_fp8_scaled.safetensors'],
   ['vae',catalog.vaes,'flux2-vae.safetensors']
  ];
  for(const [key,available,preferred] of defaults)if(!config![key]&&available.includes(preferred))config![key]=preferred;
  saveConfig();render();
 }));
 const sheet=ctx.root.querySelector<HTMLFormElement>('#studio-sheet');
 if(sheet){
  sheet.onsubmit=e=>e.preventDefault();
  sheet.oninput=()=>{
   if(!draft)return;
   const f=(n:string)=>(sheet.elements.namedItem(n) as HTMLInputElement).value;
   draft={...draft,name:f('name'),personality:f('personality'),instructions:f('instructions'),accent:f('accent') as CharacterDraft['accent'],physical_description:f('physical_description'),appearance_negative_prompt:f('appearance_negative_prompt'),portrait_prompt:f('portrait_prompt'),emotion_prompts:Object.fromEntries(emotions.slice(1).map(e=>[e,f(e)])) as CharacterDraft['emotion_prompts']};paint();
  };
 }
 ctx.root.querySelector('#studio-neutral')!.addEventListener('click',()=>void run(()=>api.generateCharacter(draft!,config!,'neutral')));
 ctx.root.querySelector('#studio-validate')!.addEventListener('click',()=>void run(()=>api.validateCharacterNeutral(status.revision)));
 ctx.root.querySelector('#studio-emotions')!.addEventListener('click',()=>void run(()=>api.generateCharacter(draft!,config!,null)));
 ctx.root.querySelectorAll<HTMLButtonElement>('[data-regenerate]').forEach(b=>b.onclick=()=>void run(()=>api.generateCharacter(draft!,config!,b.dataset.regenerate as Emotion)));
 ctx.root.querySelector('#studio-transparency')!.addEventListener('click',()=>void run(()=>api.processCharacterTransparency(draft!,config!)));
 ctx.root.querySelector('#studio-finish')!.addEventListener('click',()=>void run(async()=>{const id=await api.finishCharacter(status.revision,draft!,config!);ctx.created(id);}));
 ctx.root.querySelector('#studio-stop')!.addEventListener('click',()=>void api.cancelCharacterStudio().catch(showError));
 ctx.root.querySelectorAll<HTMLButtonElement>('[data-workflow]').forEach(b=>b.onclick=()=>void showWorkflow(b.dataset.workflow as CharacterStage).catch(showError));
 ctx.root.querySelectorAll<HTMLElement>('[data-emotion]').forEach(card=>card.querySelector<HTMLButtonElement>('.studio-preview')!.onclick=()=>{
  const e=card.dataset.emotion as Emotion,data=(card.dataset.transparent==='true'?status.transparent_images:status.images)[e];
  if(data)showPortrait(data,labels()[emotions.indexOf(e)]!);
 });
 paint();
}
function paint():void {
 const root=context?.root;if(!root?.isConnected||!config)return;
 const working=pending||status.busy,matching=portraitMatches(),valid=matching&&status.neutral_validated;
 const allRaw=valid&&emotions.every(e=>status.images[e]&&(e==='neutral'||moodMatches(e)));
 const allAlpha=allRaw&&emotions.every(e=>status.transparent_images[e]);
 const done=!!status.created_profile,moodReady=!!(config.mood_model&&config.text_encoder&&config.vae);
 const button=(id:string,disabled:boolean)=>{const b=root.querySelector<HTMLButtonElement>('#'+id);if(b)b.disabled=disabled;};
 button('studio-plan',working);button('studio-refresh',working);button('studio-neutral',working||!draft||!config.checkpoint||done);
 button('studio-import',working||!draft||done);
 root.querySelectorAll<HTMLButtonElement>('[data-prompt]').forEach(b=>b.disabled=working||!draft||done||(b.dataset.prompt==='neutral'&&status.neutral_imported&&!!status.images.neutral));
 const preview=root.querySelector<HTMLImageElement>('#studio-seed-preview')!;preview.hidden=!seed;if(seed&&preview.getAttribute('src')!==seed.data)preview.src=seed.data;
 root.querySelector('#studio-seed-name')!.textContent=seed?.name??'';root.querySelector<HTMLElement>('#studio-seed-clear')!.hidden=!seed;
 root.querySelector('#studio-note')!.textContent=status.note?feedback(status.note):'';
 // Retouching an existing companion replaces its images; identity stays as it is.
 const target=status.target_profile?context!.profiles.find(p=>p.id===status.target_profile):undefined,targetNote=root.querySelector<HTMLElement>('#studio-target')!;
 targetNote.hidden=!target;targetNote.textContent=target?t`Retouche des images de ${target.name}. Sa personnalité, sa voix et ses souvenirs ne changent pas.`:'';
 root.querySelector('#studio-finish')!.textContent=target?t`Appliquer à ${target.name}`:t('Ajouter aux compagnons');
 root.querySelector('#studio-finish-note')!.textContent=target?t('Ses sept images actuelles seront remplacées.'):t('La voix et les connexions se règlent ensuite dans Compagnons.');
 root.querySelectorAll<HTMLElement>('[data-prompt]').forEach(b=>b.classList.toggle('studio-prompt-edited',!!edited(draft,b.dataset.prompt as Emotion)));
 button('studio-validate',working||!matching||!status.images.neutral||status.neutral_validated||done);
 button('studio-emotions',working||!valid||!moodReady||allRaw||done);
 button('studio-transparency',working||!allRaw||allAlpha||done);
 button('studio-finish',working||!allAlpha||done);
 root.querySelectorAll<HTMLButtonElement>('[data-regenerate]').forEach(b=>{
  b.disabled=working||!valid||!moodReady||done;
  b.textContent=status.images[b.dataset.regenerate as Emotion]?t('Régénérer'):t('Générer');
 });
 root.querySelectorAll<HTMLElement>('#studio-concept,#studio-sheet,#studio-models').forEach(el=>el.inert=working);
 const stop=root.querySelector<HTMLButtonElement>('#studio-stop')!;stop.hidden=!status.busy;stop.disabled=status.phase==='publishing';
 root.querySelector<HTMLElement>('#studio-changed')!.hidden=!status.images.neutral||matching;
 const missing=cachedCatalog()?.missing_nodes;
 root.querySelector('#studio-missing')!.textContent=missing&&Object.values(missing).some(v=>v.length)?t`Nodes ComfyUI manquants : ${[...new Set(Object.values(missing).flat())].join(', ')}`:'';
 const phase=root.querySelector('#studio-phase')!;
 const current=emotions.find(e=>status.phase.endsWith('_'+e));
 const progress=current?' · '+labels()[emotions.indexOf(current)]:'';
 phase.textContent=status.busy?(status.phase==='planning'?t('Le modèle prépare la fiche…'):status.phase==='publishing'?t('Ajout du compagnon…'):status.phase.startsWith('transparency')?t('Détourage des portraits…')+progress:t('ComfyUI génère le portrait. Le premier chargement peut prendre du temps…')+progress):done?(status.target_profile?t('Images du compagnon mises à jour.'):t('Compagnon ajouté.')):status.phase==='stopped'?t('Création interrompue.'):valid?t('Portrait neutre validé.'):t('Vérifie les images avant de les valider.');
 root.querySelector('#studio-error')!.textContent=localError??(!pending&&status.error?feedback(status.error):'');
 root.querySelectorAll<HTMLElement>('[data-emotion]').forEach(card=>{
  const e=card.dataset.emotion as Emotion,alpha=card.dataset.transparent==='true';
  const usable=e==='neutral'?matching:valid&&moodMatches(e);
  const data=usable?(alpha?status.transparent_images:status.images)[e]:undefined,img=card.querySelector('img')!;
  img.hidden=!data;card.querySelector<HTMLElement>('.studio-preview span')!.hidden=!!data;
  card.querySelector<HTMLButtonElement>('.studio-preview')!.disabled=!data;
  if(data&&img.getAttribute('src')!==data)img.src=data;
 });
}
function modal(title:string,body:string):HTMLDialogElement {
 const dialog=document.createElement('dialog');dialog.className='modal studio-dialog';
 dialog.innerHTML=html`<div class="modal-header"><h2>${escape(title)}</h2><button class="button secondary small" data-close>Fermer</button></div>${body}`;
 document.body.append(dialog);dialog.querySelector('[data-close]')!.addEventListener('click',()=>dialog.close());
 dialog.addEventListener('close',()=>dialog.remove());dialog.addEventListener('click',e=>{if(e.target===dialog){const r=dialog.getBoundingClientRect();if(e.clientX<r.left||e.clientX>r.right||e.clientY<r.top||e.clientY>r.bottom)dialog.close();}});
 dialog.showModal();return dialog;
}
function showPortrait(data:string,title:string):void {
 const dialog=modal(title,'<div class="studio-large-preview"><img/></div>');
 const image=dialog.querySelector('img')!;image.src=data;image.alt=title;
}
// The exact prompts ComfyUI receives for one image. An edit is kept with the
// sheet and sent verbatim; matching the automatic prompts removes it.
async function editPrompt(e:Emotion):Promise<void> {
 if(!draft||!config)return;
 const automatic=(await api.characterPrompts(draft,config))[e],current=edited(draft,e)??automatic;
 const label=labels()[emotions.indexOf(e)]!;
 const dialog=modal(t`Prompt · ${label}`,html`<p>${e==='neutral'?t('Prompt envoyé au modèle du portrait (SDXL ou Illustrious), avant les réglages fixes du workflow.'):t('Instruction de modification envoyée à Klein pour cette émotion.')}</p>
 <label>Prompt positif<textarea name="positive" rows="8" maxlength="8000" spellcheck="false"></textarea></label>
 <label>Prompt négatif<textarea name="negative" rows="4" maxlength="8000" spellcheck="false"></textarea></label>
 <p class="connection-note">${t('Ton texte est envoyé tel quel. Une modification de la fiche ne change plus ce prompt tant que tu ne reviens pas au prompt automatique.')}</p>
 <p class="form-error" role="alert"></p><div class="modal-actions"><button class="button secondary" data-automatic>Revenir au prompt automatique</button><button class="button primary" data-apply>Utiliser ce prompt</button></div>`);
 const field=(n:string)=>dialog.querySelector<HTMLTextAreaElement>(`[name="${n}"]`)!;
 field('positive').value=current.positive;field('negative').value=current.negative;
 const save=(value:{positive:string;negative:string}|null)=>{
  if(!draft)return;const overrides={...draft.prompt_overrides};
  if(!value||same(value,automatic))delete overrides[e];else overrides[e]=value;
  draft={...draft,prompt_overrides:overrides};dialog.close();paint();
 };
 dialog.querySelector('[data-automatic]')!.addEventListener('click',()=>save(null));
 dialog.querySelector('[data-apply]')!.addEventListener('click',()=>{
  const value={positive:field('positive').value.trim(),negative:field('negative').value.trim()};
  if(!value.positive){dialog.querySelector('.form-error')!.textContent=t('Le prompt positif ne peut pas être vide.');return;}
  save(value);
 });
}
async function showWorkflow(stage:CharacterStage):Promise<void> {
 const graph=await api.characterWorkflow(stage);
 const dialog=modal(t('Workflow ComfyUI'),html`<p>Workflow intégré, en lecture seule. Les modèles et les images sont injectés à chaque étape.</p><textarea class="studio-workflow" rows="22" readonly aria-label="Workflow ComfyUI"></textarea>`);
 dialog.querySelector('textarea')!.value=JSON.stringify(graph,null,2);
}
