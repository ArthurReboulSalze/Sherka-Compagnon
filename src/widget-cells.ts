import * as api from './api';
import {html,t} from './i18n';
import {icon} from './icons';
import {comfyStartupMessage} from './comfy-startup';
import type {Snapshot,IllustrationStatus,WidgetSurface,Attachment,ImagePlacement} from './types';
const esc=(s:string)=>s.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
type Cell={text:boolean;draft:string;speak:boolean;attachments:Attachment[];textHeight?:number;image?:ImagePlacement};
const cells=new Map<string,Cell>();
let current:Snapshot|undefined,root:HTMLElement|null=null,base:WidgetSurface|undefined,status:IllustrationStatus|undefined;
let report:(e:unknown)=>void=()=>{},started=false,focus=false,frame=0,inflight=false,pending:WidgetSurface|undefined,lastSurface='';
let slides:string[]=[],slide=0,first='',timer:ReturnType<typeof setInterval>|undefined,decode=0;
let textSize:ResizeObserver|undefined;
let fullscreen=false;
// Open prompt editor over the image cell, with the text being edited.
let promptEdit:{positive:string;negative:string}|null=null;
let saveTimer:ReturnType<typeof setTimeout>|undefined;
const savedPlacements=new Map<string,string>();
function persistPlacement(immediate=false):void {
 if(!current)return;const id=profile().id,c=cell(),data=JSON.stringify([c.image??null,c.textHeight??null]);
 if(savedPlacements.get(id)===data)return;
 clearTimeout(saveTimer);const save=()=>{if(current?.session.profile_id!==id)return;void api.saveWidgetCell(id,c.image??null,c.textHeight??null).then(()=>savedPlacements.set(id,data)).catch(e=>{if(current?.session.profile_id===id)report(e);});};if(immediate)save();else saveTimer=setTimeout(save,250);
}

const profile=()=>current!.profiles.find(p=>p.id===current!.session.profile_id)!;
function cell():Cell {const id=profile().id;if(!cells.has(id)){const saved=current!.settings.desktop_layouts[id];cells.set(id,{text:false,draft:'',speak:true,attachments:[],image:saved?.image??undefined,textHeight:saved?.text_height??undefined});}return cells.get(id)!;}
export function captureCellFocus():void {focus=document.activeElement?.id==='cell-text';}
function schedule():void {cancelAnimationFrame(frame);frame=requestAnimationFrame(layout);}
async function flush():Promise<void>{
 if(inflight)return;inflight=true;
 try{while(pending){const value=pending,owner=current!.session.profile_id;pending=undefined;const key=owner+JSON.stringify(value);if(key===lastSurface)continue;try{await api.widgetSurface(value,owner);if(current?.session.profile_id===owner)lastSurface=key;}catch(e){if(current?.session.profile_id===owner)throw e;}}}
 catch(e){report(e);}finally{inflight=false;}
}
function layout():void{
 if(!root?.isConnected||!base)return;
 const c=cell(),p=profile(),widget=document.querySelector<HTMLElement>('.widget')!,canvas=document.querySelector<HTMLElement>('#widget-canvas')!;
 canvas.style.setProperty('--avatar-width',base.base_width+'px');canvas.style.setProperty('--avatar-height',base.base_height+'px');
 const box=widget.getBoundingClientRect(),toolbar=widget.querySelector('.widget-toolbar')!.getBoundingClientRect();
 const y=toolbar.bottom-box.top+7,text=root.querySelector<HTMLElement>('#widget-text-cell')!,image=root.querySelector<HTMLElement>('#widget-image-cell')!;
 text.hidden=!c.text;image.hidden=fullscreen||!p.illustration?.enabled||p.connections.llm_provider==='hermes';
 const textWidth=Math.max(230,Math.min(320,base.base_width+20)),textX=(base.base_width-textWidth)/2;
 text.style.width=textWidth+'px';text.style.left=textX+'px';text.style.top=y+'px';
 if(!c.image){const size=Math.min(260,Math.max(210,base.base_width));c.image={x:(base.base_width-size)/2,y:y+(c.text?text.offsetHeight+7:0),size,moved:false};}
 if(!c.image.moved){c.image.x=(base.base_width-c.image.size)/2;c.image.y=y+(c.text?text.offsetHeight+7:0);}
 const pos=c.image;image.style.left=pos.x+'px';image.style.top=pos.y+'px';image.style.width=pos.size+'px';
 // The prompt editor is attached below the image and follows it.
 const prompt=root.querySelector<HTMLElement>('#widget-prompt-cell')!,promptTop=pos.y+image.offsetHeight+5;prompt.hidden=image.hidden||!promptEdit;
 prompt.style.left=pos.x+'px';prompt.style.top=promptTop+'px';prompt.style.width=pos.size+'px';
 const minX=Math.floor(Math.min(0,c.text?textX:0,!image.hidden?pos.x:0)),minY=Math.floor(Math.min(0,!image.hidden?pos.y:0));
 const right=Math.ceil(Math.max(base.base_width,c.text?textX+textWidth+3:0,!image.hidden?pos.x+pos.size+3:0));
 const bottom=Math.ceil(Math.max(base.base_height,c.text?y+text.offsetHeight+3:0,!image.hidden?pos.y+image.offsetHeight+3:0,!prompt.hidden?promptTop+prompt.offsetHeight+3:0));
 canvas.style.left=-minX+'px';canvas.style.top=-minY+'px';
 const notices=document.querySelector<HTMLElement>('.notifications');if(notices)notices.style.left=(-minX+base.base_width/2)+'px';
 persistPlacement();
 pending={left:minX,top:minY,width:Math.max(188,right-minX),height:Math.max(228,bottom-minY),base_width:base.base_width,base_height:base.base_height};void flush();
}
function showSlide():void{
 const layers=root?.querySelector<HTMLElement>('#cell-image-layers');if(!layers)return;const version=++decode;
 const counter=root!.querySelector<HTMLElement>('#cell-image-count')!;counter.hidden=!!profile().illustration?.live;counter.textContent=!counter.hidden&&slides.length?`${slide+1}/${slides.length}`:'';
 if(!slides.length){layers.replaceChildren();return;}
 const src=slides[slide]!;if(layers.querySelector<HTMLImageElement>('img:not(.leaving)')?.src===src)return;
 const img=new Image();img.src=src;img.alt=t('Illustration de la conversation');img.draggable=false;
 void img.decode().then(()=>{if(version!==decode||!layers.isConnected)return;const old=layers.querySelector('img:not(.leaving)');layers.querySelectorAll('.leaving').forEach(el=>el.remove());if(old)old.classList.add('leaving');layers.append(img);layers.style.aspectRatio=String(img.naturalWidth/img.naturalHeight);schedule();if(old){const duration=matchMedia('(prefers-reduced-motion: reduce)').matches?0:500;img.animate([{opacity:0},{opacity:1}],{duration});old.animate([{opacity:1},{opacity:0}],{duration}).onfinish=()=>old.remove();}}).catch(report);
}
function paintImages():void{
 if(!root||!current)return;const active=status?.profile_id===profile().id?status:undefined;
 const images=active?.images??(active?.image?[active.image]:[]);if(images[0]!==first){first=images[0]??'';slide=0;}slides=images;slide=slides.length?slide%slides.length:0;
 const message=comfyStartupMessage(profile().illustration?.server_url??'')??active?.message??(active?.phase==='paused'?t('Images en pause : priorité à la conversation.'):active?.phase==='planning'?t('Préparation de la scène…'):active?.phase==='generating'?t('ComfyUI dessine la scène…'):active?.phase==='ready'?(active.prompt?.edited?t('Image de ton prompt modifié'):t('Image du dernier échange')):t('La prochaine conversation inspirera une image.'));
 root.querySelector('#cell-image-status')!.textContent=message;
 root.querySelector<HTMLElement>('#cell-image-placeholder')!.hidden=slides.length>0;
 root.querySelector<HTMLButtonElement>('#cell-image-refresh')!.disabled=['planning','generating','paused','disabled'].includes(active?.phase??'disabled');
 const promptButton=root.querySelector<HTMLButtonElement>('#cell-image-prompt')!;
 if(!active?.prompt)promptEdit=null;
 promptButton.disabled=!active?.prompt;promptButton.setAttribute('aria-pressed',String(!!promptEdit));promptButton.classList.toggle('edited',!!active?.prompt?.edited);
 const editor=root.querySelector<HTMLFormElement>('#cell-prompt')!;const wasOpen=!root.querySelector<HTMLElement>('#widget-prompt-cell')!.hidden;
 if(wasOpen!==!!promptEdit)schedule();
 if(promptEdit)for(const name of ['positive','negative'] as const){const field=editor.elements.namedItem(name) as HTMLTextAreaElement;if(field.value!==promptEdit[name])field.value=promptEdit[name];}
 showSlide();
}
function paint():void{
 if(!root||!current)return;const c=cell();
 textSize?.disconnect();
 root.innerHTML=html`<section id="widget-text-cell" class="widget-text-cell" hidden><div id="cell-attachments">${c.attachments.map((a,i)=>html`<span class="cell-attachment"><img src="${a.data}" alt="" /><span>${esc(a.name)}</span><button type="button" data-remove-attachment="${i}" aria-label="Retirer l’image">×</button></span>`).join('')}</div><form id="cell-text-form"><textarea id="cell-text" rows="2" maxlength="128000" aria-label="Message au compagnon" placeholder="Écris ou colle du texte, ou ajoute une image ici.">${esc(c.draft)}</textarea><div class="cell-text-actions"><label title="Lire à voix haute"><input id="cell-speak" type="checkbox" ${c.speak?'checked':''}/>${icon('mic')}</label><button type="submit" class="icon-button" aria-label="Envoyer" title="Envoyer">${icon('arrow')}</button></div></form></section><section id="widget-image-cell" class="widget-image-cell" hidden><div id="cell-image-layers" class="cell-image-layers" title="Double-clique pour afficher en plein écran"></div><div id="cell-image-placeholder">${icon('image')}<p>La première image peut prendre plus de temps : ComfyUI démarre si nécessaire, puis charge le modèle et les LoRA sélectionnés.</p></div><div class="cell-image-bar" id="cell-image-drag"><span id="cell-image-status"></span><small id="cell-image-count"></small><button id="cell-image-prompt" class="icon-button" title="Voir et modifier le prompt" aria-label="Voir et modifier le prompt">${icon('edit')}</button><button id="cell-image-refresh" class="icon-button" title="Régénérer l’image" aria-label="Régénérer l’image">${icon('spark')}</button></div><button id="cell-image-resize" class="cell-image-resize" title="Redimensionner l’image" aria-label="Redimensionner l’image">${icon('resize')}</button></section><section id="widget-prompt-cell" class="widget-prompt-cell" hidden><form id="cell-prompt" class="cell-prompt"><label>Prompt positif<textarea name="positive" rows="4" maxlength="8000" spellcheck="false"></textarea></label><label>Prompt négatif<textarea name="negative" rows="2" maxlength="8000" spellcheck="false"></textarea></label><div class="cell-prompt-actions"><button type="button" class="button quiet small" data-prompt-auto title="Revenir au prompt préparé par le modèle">Automatique</button><button type="submit" class="button primary small">Générer</button></div></form></section>`;
 root.querySelectorAll<HTMLButtonElement>('[data-remove-attachment]').forEach(b=>b.onclick=()=>{c.attachments.splice(Number(b.dataset.removeAttachment),1);paint();});
 const input=root.querySelector<HTMLTextAreaElement>('#cell-text')!,form=root.querySelector<HTMLFormElement>('form')!;
 if(c.textHeight)input.style.height=c.textHeight+'px';
 textSize=new ResizeObserver(()=>{if(!input.isConnected||!c.text||!input.offsetHeight)return;c.textHeight=input.getBoundingClientRect().height;schedule();});
 textSize.observe(root.querySelector('#widget-text-cell')!);
 input.oninput=()=>{c.draft=input.value;};root.querySelector<HTMLInputElement>('#cell-speak')!.onchange=e=>{c.speak=(e.target as HTMLInputElement).checked;};
 form.onsubmit=e=>{e.preventDefault();const text=input.value.trim();if(!text&&!c.attachments.length)return;const id=profile().id,button=form.querySelector<HTMLButtonElement>('button[type="submit"]')!;if(button.disabled)return;button.disabled=true;void (text==='/new'&&profile().connections.llm_provider==='hermes'&&!c.attachments.length?api.newHermesConversation(id):api.sendMessage(id,text||t('Décris cette image.'),c.speak,[...c.attachments])).then(()=>{c.draft='';c.attachments=[];if(current?.session.profile_id===id){focus=true;paint();}}).catch(report).finally(()=>button.disabled=false);};
 input.onkeydown=e=>{if(e.key==='Enter'&&!e.shiftKey&&!e.isComposing&&e.keyCode!==229){e.preventDefault();form.requestSubmit();}};
 root.querySelector('#cell-image-refresh')!.addEventListener('click',()=>void api.refreshIllustration().catch(report));
 const editor=root.querySelector<HTMLFormElement>('#cell-prompt')!;
 root.querySelector('#cell-image-prompt')!.addEventListener('click',()=>{const prompt=status?.profile_id===profile().id?status.prompt:null;promptEdit=promptEdit||!prompt?null:{positive:prompt.positive,negative:prompt.negative};paintImages();if(promptEdit)requestAnimationFrame(()=>editor.querySelector('textarea')?.focus());});
 editor.oninput=()=>{if(promptEdit)promptEdit={positive:(editor.elements.namedItem('positive') as HTMLTextAreaElement).value,negative:(editor.elements.namedItem('negative') as HTMLTextAreaElement).value};};
 editor.onkeydown=e=>{if(e.key==='Escape'){e.preventDefault();e.stopPropagation();promptEdit=null;paintImages();}};
 editor.onsubmit=e=>{e.preventDefault();if(!promptEdit)return;const value=promptEdit;if(!value.positive.trim()){report(new Error(t('Le prompt positif ne peut pas être vide.')));return;}void api.setIllustrationPrompt(value.positive,value.negative).then(()=>{promptEdit=null;paintImages();}).catch(report);};
 editor.querySelector('[data-prompt-auto]')!.addEventListener('click',()=>void api.resetIllustrationPrompt().then(()=>{promptEdit=null;paintImages();}).catch(report));
 const drag=(event:PointerEvent,resize:boolean)=>{if(event.button!==0)return;event.preventDefault();const target=event.currentTarget as HTMLElement;target.setPointerCapture(event.pointerId);const preview=root!.querySelector<HTMLElement>('#cell-image-layers')!,ratio=preview.offsetWidth/Math.max(1,preview.offsetHeight);const start={...c.image!},x=event.screenX,y=event.screenY;const move=(e:PointerEvent)=>{c.image=resize?{...start,size:Math.max(140,Math.min(1600,screen.availWidth-12,(screen.availHeight-40)*ratio,start.size+Math.max(e.screenX-x,(e.screenY-y)*ratio))),moved:true}:{...start,moved:true,x:Math.max(-800,Math.min(900,start.x+e.screenX-x)),y:Math.max(-600,Math.min(800,start.y+e.screenY-y))};if(resize){c.image.x=Math.min(c.image.x,Math.max(0,screen.availWidth-c.image.size-3));c.image.y=Math.min(c.image.y,Math.max(0,screen.availHeight-c.image.size/ratio-34));}schedule();};const end=()=>{persistPlacement(true);target.removeEventListener('pointermove',move);target.removeEventListener('pointerup',end);target.removeEventListener('pointercancel',end);};target.addEventListener('pointermove',move);target.addEventListener('pointerup',end);target.addEventListener('pointercancel',end);};
 root.querySelector('#cell-image-drag')!.addEventListener('pointerdown',e=>{if(!(e.target as Element).closest('button'))drag(e as PointerEvent,false);});
 root.querySelector('#cell-image-layers')!.addEventListener('pointerdown',e=>drag(e as PointerEvent,false));
 root.querySelector('#cell-image-layers')!.addEventListener('dblclick',()=>{if(slides.length)void api.imageFullscreen(true,profile().id).catch(report);});
 root.querySelector('#cell-image-resize')!.addEventListener('pointerdown',e=>drag(e as PointerEvent,true));
 document.querySelector('#widget-text')?.setAttribute('aria-pressed',String(c.text));
 paintImages();schedule();if(focus&&c.text){focus=false;requestAnimationFrame(()=>input.focus());}
}
async function drop(event:DragEvent):Promise<void>{
 event.preventDefault();if(event.dataTransfer)await addTransfer(event.dataTransfer);
}
async function addTransfer(transfer:DataTransfer):Promise<void>{
 if(!current||!root)return;const c=cell(),owner=profile().id;
 try{const files=[...transfer.files];if(files.length>4)throw new Error(t('Dépose au maximum quatre fichiers.'));const images=files.filter(f=>/^image\//.test(f.type)||/\.(png|jpe?g|webp)$/i.test(f.name));
  if(images.length){const support=await api.attachmentSupport(profile().id);if(!support.images)throw new Error(t('Ce modèle n’accepte pas les images ou sa capacité visuelle n’est pas confirmée.'));}
  if(current?.session.profile_id!==owner)return;
  for(const file of files){if(images.includes(file)){if(file.size>10*1024*1024||c.attachments.length>=4)throw new Error(t('Choisis au maximum quatre images de moins de 10 Mio.'));const data=await new Promise<string>((done,fail)=>{const reader=new FileReader();reader.onload=()=>done(String(reader.result));reader.onerror=()=>fail(new Error(t('Impossible de lire ce fichier.')));reader.readAsDataURL(file);});if(current?.session.profile_id!==owner)return;c.attachments.push({name:file.name,data});}
   else {if(file.size>128000)throw new Error(t('Ce fichier texte est trop volumineux.'));const text=await file.text();if(text.includes('\0'))throw new Error(t('Choisis une image ou un fichier texte.'));if(current?.session.profile_id!==owner)return;c.draft+=(c.draft?'\n\n':'')+text;}}
  if(!files.length)c.draft+=(c.draft?'\n':'')+transfer.getData('text/plain');
  c.text=true;focus=true;paint();
 }catch(e){report(e);}
}
export function mountWidgetCells(snapshot:Snapshot,onError:(e:unknown)=>void):void{
 for(const id of cells.keys())if(!snapshot.profiles.some(p=>p.id===id))cells.delete(id);
 const changed=current?.session.profile_id!==snapshot.session.profile_id;
 current=snapshot;report=onError;root=document.querySelector('#widget-cells');if(!root)return;
 if(changed){base=undefined;pending=undefined;lastSurface='';clearTimeout(saveTimer);const owner=snapshot.session.profile_id;void api.widgetSurface(undefined,owner).then(value=>{if(current?.session.profile_id===owner){base=value;schedule();}}).catch(report);}

 if(!started){started=true;void api.onImageFullscreen(open=>{fullscreen=open;schedule();});document.addEventListener('keydown',e=>{if(e.key==='Escape'&&fullscreen){e.preventDefault();void api.imageFullscreen(false).catch(report);}});void api.onAvatarSize(size=>{if(base){base.base_width=size[0];base.base_height=size[1];schedule();}});void api.onIllustration(value=>{status=value;paintImages();});void api.illustration().then(value=>{status=value;paintImages();}).catch(report);document.addEventListener('sherka:widget-layout',schedule);document.addEventListener('sherka:comfy-service',paintImages);document.addEventListener('dragover',e=>e.preventDefault());document.addEventListener('drop',e=>void drop(e));document.addEventListener('paste',e=>{if(e.clipboardData?.files.length){e.preventDefault();void addTransfer(e.clipboardData);}});void api.onCleared(id=>{const c=cells.get(id);if(c){c.draft='';c.attachments=[];}if(current?.session.profile_id===id)paint();});}
 document.querySelector('#widget-text')?.addEventListener('click',()=>{const c=cell();c.text=!c.text;focus=c.text;if(!c.image)c.image=undefined;paint();});
 document.querySelector('#widget-image')?.addEventListener('click',()=>void api.desktopTool('image',!profile().illustration?.enabled).catch(report));
 document.querySelector('#resize-widget')?.addEventListener('pointerdown',e=>{const event=e as PointerEvent;if(event.button!==0||!base)return;event.preventDefault();const target=event.currentTarget as HTMLElement;target.setPointerCapture(event.pointerId);const w=base.base_width,h=base.base_height,x=event.screenX,y=event.screenY;const move=(v:PointerEvent)=>{base!.base_width=Math.max(188,Math.min(600,w+v.screenX-x));base!.base_height=Math.max(228,Math.min(720,h+v.screenY-y));schedule();};const end=()=>{persistPlacement(true);target.removeEventListener('pointermove',move);target.removeEventListener('pointerup',end);target.removeEventListener('pointercancel',end);};target.addEventListener('pointermove',move);target.addEventListener('pointerup',end);target.addEventListener('pointercancel',end);});
 clearInterval(timer);timer=setInterval(()=>{if(profile().illustration?.enabled&&slides.length>1){slide=(slide+1)%slides.length;showSlide();}},(profile().illustration?.slide_seconds??6)*1000);
 paint();
}
