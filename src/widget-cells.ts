import * as api from './api';
import {html,t} from './i18n';
import {icon} from './icons';
import type {Snapshot,IllustrationStatus,WidgetSurface,Attachment} from './types';
const esc=(s:string)=>s.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
type Cell={text:boolean;draft:string;speak:boolean;attachments:Attachment[];image?:{x:number;y:number;size:number;moved?:boolean}};
const cells=new Map<string,Cell>();
let current:Snapshot|undefined,root:HTMLElement|null=null,base:WidgetSurface|undefined,status:IllustrationStatus|undefined;
let report:(e:unknown)=>void=()=>{},started=false,focus=false,frame=0,inflight=false,pending:WidgetSurface|undefined,lastSurface='';
let slides:string[]=[],slide=0,first='',timer:ReturnType<typeof setInterval>|undefined,decode=0;
const profile=()=>current!.profiles.find(p=>p.id===current!.session.profile_id)!;
function cell():Cell {const id=profile().id;if(!cells.has(id))cells.set(id,{text:false,draft:'',speak:true,attachments:[]});return cells.get(id)!;}
export function captureCellFocus():void {focus=document.activeElement?.id==='cell-text';}
function schedule():void {cancelAnimationFrame(frame);frame=requestAnimationFrame(layout);}
async function flush():Promise<void>{
 if(inflight)return;inflight=true;
 try{while(pending){const value=pending;pending=undefined;const key=JSON.stringify(value);if(key===lastSurface)continue;await api.widgetSurface(value);lastSurface=key;}}
 catch(e){report(e);}finally{inflight=false;}
}
function layout():void{
 if(!root?.isConnected||!base)return;
 const c=cell(),p=profile(),widget=document.querySelector<HTMLElement>('.widget')!,canvas=document.querySelector<HTMLElement>('#widget-canvas')!;
 canvas.style.setProperty('--avatar-width',base.base_width+'px');canvas.style.setProperty('--avatar-height',base.base_height+'px');
 const box=widget.getBoundingClientRect(),toolbar=widget.querySelector('.widget-toolbar')!.getBoundingClientRect();
 const y=toolbar.bottom-box.top+7,text=root.querySelector<HTMLElement>('#widget-text-cell')!,image=root.querySelector<HTMLElement>('#widget-image-cell')!;
 text.hidden=!c.text;image.hidden=!p.illustration?.enabled||p.connections.llm_provider==='hermes';
 const textWidth=Math.max(230,Math.min(320,base.base_width+20)),textX=(base.base_width-textWidth)/2;
 text.style.width=textWidth+'px';text.style.left=textX+'px';text.style.top=y+'px';
 if(!c.image){const size=Math.min(260,Math.max(210,base.base_width));c.image={x:(base.base_width-size)/2,y:y+(c.text?text.offsetHeight+7:0),size};}
 if(!c.image.moved){c.image.x=(base.base_width-c.image.size)/2;c.image.y=y+(c.text?text.offsetHeight+7:0);}
 const pos=c.image;image.style.left=pos.x+'px';image.style.top=pos.y+'px';image.style.width=pos.size+'px';
 const minX=Math.floor(Math.min(0,c.text?textX:0,!image.hidden?pos.x:0)),minY=Math.floor(Math.min(0,!image.hidden?pos.y:0));
 const right=Math.ceil(Math.max(base.base_width,c.text?textX+textWidth+3:0,!image.hidden?pos.x+pos.size+3:0));
 const bottom=Math.ceil(Math.max(base.base_height,c.text?y+text.offsetHeight+3:0,!image.hidden?pos.y+image.offsetHeight+3:0));
 canvas.style.left=-minX+'px';canvas.style.top=-minY+'px';
 const notices=document.querySelector<HTMLElement>('.notifications');if(notices)notices.style.left=(-minX+base.base_width/2)+'px';
 pending={left:minX,top:minY,width:Math.max(188,right-minX),height:Math.max(228,bottom-minY),base_width:base.base_width,base_height:base.base_height};void flush();
}
function showSlide():void{
 const layers=root?.querySelector<HTMLElement>('#cell-image-layers');if(!layers)return;const version=++decode;
 root!.querySelector('#cell-image-count')!.textContent=slides.length?`${slide+1}/${slides.length}`:'';
 if(!slides.length){layers.replaceChildren();return;}
 const src=slides[slide]!;if(layers.querySelector<HTMLImageElement>('img:not(.leaving)')?.src===src)return;
 const img=new Image();img.src=src;img.alt=t('Illustration de la conversation');img.draggable=false;
 void img.decode().then(()=>{if(version!==decode||!layers.isConnected)return;const old=layers.querySelector('img:not(.leaving)');layers.querySelectorAll('.leaving').forEach(el=>el.remove());if(old)old.classList.add('leaving');layers.append(img);if(old){const duration=matchMedia('(prefers-reduced-motion: reduce)').matches?0:500;img.animate([{opacity:0},{opacity:1}],{duration});old.animate([{opacity:1},{opacity:0}],{duration}).onfinish=()=>old.remove();}}).catch(report);
}
function paintImages():void{
 if(!root||!current)return;const active=status?.profile_id===profile().id?status:undefined;
 const images=active?.images??(active?.image?[active.image]:[]);if(images[0]!==first){first=images[0]??'';slide=0;}slides=images;slide=slides.length?slide%slides.length:0;
 const message=active?.message??(active?.phase==='planning'?t('Préparation de la scène…'):active?.phase==='generating'?t('ComfyUI dessine la scène…'):active?.phase==='ready'?t('Image du dernier échange'):t('La prochaine conversation inspirera une image.'));
 root.querySelector('#cell-image-status')!.textContent=message;
 root.querySelector<HTMLElement>('#cell-image-placeholder')!.hidden=slides.length>0;
 root.querySelector<HTMLButtonElement>('#cell-image-refresh')!.disabled=['planning','generating','disabled'].includes(active?.phase??'disabled');
 showSlide();
}
function paint():void{
 if(!root||!current)return;const c=cell();
 root.innerHTML=html`<section id="widget-text-cell" class="widget-text-cell" hidden><div id="cell-attachments">${c.attachments.map((a,i)=>html`<span class="cell-attachment"><img src="${a.data}" alt="" /><span>${esc(a.name)}</span><button type="button" data-remove-attachment="${i}" aria-label="Retirer l’image">×</button></span>`).join('')}</div><form id="cell-text-form"><textarea id="cell-text" rows="2" maxlength="128000" aria-label="Message au compagnon" placeholder="Écris ou colle ton texte ici.">${esc(c.draft)}</textarea><div class="cell-text-actions"><label title="Lire à voix haute"><input id="cell-speak" type="checkbox" ${c.speak?'checked':''}/>${icon('mic')}</label><button type="submit" class="icon-button" aria-label="Envoyer" title="Envoyer">${icon('arrow')}</button></div></form></section><section id="widget-image-cell" class="widget-image-cell" hidden><div id="cell-image-layers" class="cell-image-layers"></div><div id="cell-image-placeholder">${icon('image')}</div><div class="cell-image-bar" id="cell-image-drag"><span id="cell-image-status"></span><small id="cell-image-count"></small><button id="cell-image-refresh" class="icon-button" title="Régénérer l’image" aria-label="Régénérer l’image">${icon('spark')}</button></div><button id="cell-image-resize" class="cell-image-resize" title="Redimensionner l’image" aria-label="Redimensionner l’image">${icon('resize')}</button></section>`;
 root.querySelectorAll<HTMLButtonElement>('[data-remove-attachment]').forEach(b=>b.onclick=()=>{c.attachments.splice(Number(b.dataset.removeAttachment),1);paint();});
 const input=root.querySelector<HTMLTextAreaElement>('#cell-text')!,form=root.querySelector<HTMLFormElement>('form')!;
 input.oninput=()=>{c.draft=input.value;};root.querySelector<HTMLInputElement>('#cell-speak')!.onchange=e=>{c.speak=(e.target as HTMLInputElement).checked;};
 form.onsubmit=e=>{e.preventDefault();const text=input.value.trim();if(!text&&!c.attachments.length)return;const id=profile().id,button=form.querySelector<HTMLButtonElement>('button[type="submit"]')!;if(button.disabled)return;button.disabled=true;void (text==='/new'&&profile().connections.llm_provider==='hermes'&&!c.attachments.length?api.newHermesConversation(id):api.sendMessage(id,text||t('Décris cette image.'),c.speak,[...c.attachments])).then(()=>{c.draft='';c.attachments=[];if(current?.session.profile_id===id){focus=true;paint();}}).catch(report).finally(()=>button.disabled=false);};
 input.onkeydown=e=>{if(e.key==='Enter'&&!e.shiftKey&&!e.isComposing&&e.keyCode!==229){e.preventDefault();form.requestSubmit();}};
 root.querySelector('#cell-image-refresh')!.addEventListener('click',()=>void api.refreshIllustration().catch(report));
 const drag=(event:PointerEvent,resize:boolean)=>{if(event.button!==0)return;event.preventDefault();const target=event.currentTarget as HTMLElement;target.setPointerCapture(event.pointerId);const start={...c.image!},x=event.screenX,y=event.screenY;const move=(e:PointerEvent)=>{c.image=resize?{...start,size:Math.max(140,Math.min(600,start.size+Math.max(e.screenX-x,e.screenY-y)))}:{...start,moved:true,x:Math.max(-800,Math.min(900,start.x+e.screenX-x)),y:Math.max(-600,Math.min(800,start.y+e.screenY-y))};schedule();};const end=()=>{target.removeEventListener('pointermove',move);target.removeEventListener('pointerup',end);target.removeEventListener('pointercancel',end);};target.addEventListener('pointermove',move);target.addEventListener('pointerup',end);target.addEventListener('pointercancel',end);};
 root.querySelector('#cell-image-drag')!.addEventListener('pointerdown',e=>{if(!(e.target as Element).closest('button'))drag(e as PointerEvent,false);});
 root.querySelector('#cell-image-layers')!.addEventListener('pointerdown',e=>drag(e as PointerEvent,false));
 root.querySelector('#cell-image-resize')!.addEventListener('pointerdown',e=>drag(e as PointerEvent,true));
 document.querySelector('#widget-text')?.setAttribute('aria-pressed',String(c.text));
 paintImages();schedule();if(focus&&c.text){focus=false;requestAnimationFrame(()=>input.focus());}
}
async function drop(event:DragEvent):Promise<void>{
 event.preventDefault();if(!current||!root)return;const transfer=event.dataTransfer;if(!transfer)return;const c=cell(),owner=profile().id;
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
 current=snapshot;report=onError;root=document.querySelector('#widget-cells');if(!root)return;
 if(!started){started=true;void api.onAvatarSize(size=>{if(base){base.base_width=size[0];base.base_height=size[1];schedule();}});void api.widgetSurface().then(value=>{base=value;schedule();}).catch(report);void api.onIllustration(value=>{status=value;paintImages();});void api.illustration().then(value=>{status=value;paintImages();}).catch(report);document.addEventListener('sherka:widget-layout',schedule);document.addEventListener('dragover',e=>e.preventDefault());document.addEventListener('drop',e=>void drop(e));void api.onCleared(id=>{cells.delete(id);if(current?.session.profile_id===id)paint();});}
 document.querySelector('#widget-text')?.addEventListener('click',()=>{const c=cell();c.text=!c.text;focus=c.text;if(!c.image)c.image=undefined;paint();});
 document.querySelector('#widget-image')?.addEventListener('click',()=>void api.desktopTool('image',!profile().illustration?.enabled).catch(report));
 document.querySelector('#resize-widget')?.addEventListener('pointerdown',e=>{const event=e as PointerEvent;if(event.button!==0||!base)return;event.preventDefault();const target=event.currentTarget as HTMLElement;target.setPointerCapture(event.pointerId);const w=base.base_width,h=base.base_height,x=event.screenX,y=event.screenY;const move=(v:PointerEvent)=>{base!.base_width=Math.max(188,Math.min(600,w+v.screenX-x));base!.base_height=Math.max(228,Math.min(720,h+v.screenY-y));schedule();};const end=()=>{target.removeEventListener('pointermove',move);target.removeEventListener('pointerup',end);target.removeEventListener('pointercancel',end);};target.addEventListener('pointermove',move);target.addEventListener('pointerup',end);target.addEventListener('pointercancel',end);});
 clearInterval(timer);timer=setInterval(()=>{if(profile().illustration?.enabled&&slides.length>1){slide=(slide+1)%slides.length;showSlide();}},(profile().illustration?.slide_seconds??6)*1000);
 paint();
}
