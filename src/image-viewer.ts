import * as api from './api';
import {html,setLanguage,t} from './i18n';
import {icon} from './icons';
import {paintTheme} from './themes';
import type {IllustrationStatus} from './types';

export async function mountImageViewer():Promise<void> {
  document.body.classList.add('image-viewer-body');
  const app=document.querySelector<HTMLElement>('#app')!;
  let context:Awaited<ReturnType<typeof api.imageViewer>>=null;
  let pictures:string[]=[],index=0,decode=0,revision=0;
  let timer:ReturnType<typeof setInterval>|undefined;
  const close=()=>void api.imageFullscreen(false).catch(error=>{
    const status=app.querySelector<HTMLElement>('[role="alert"]');
    if(status)status.textContent=String(error);
  });
  function paint():void {
    const layers=app.querySelector<HTMLElement>('.fullscreen-image-layers');
    if(!layers||!context)return;
    const source=pictures[index];
    if(!source||layers.querySelector<HTMLImageElement>('img:not(.leaving)')?.src===source)return;
    const request=++decode,img=new Image();
    img.alt=t('Illustration de la conversation');img.draggable=false;img.src=source;
    void img.decode().then(()=>{
      if(request!==decode||!layers.isConnected||!context)return;
      const old=layers.querySelector('img:not(.leaving)');
      layers.querySelectorAll('.leaving').forEach(node=>node.remove());
      if(old)old.classList.add('leaving');layers.append(img);
      if(old){
        const duration=matchMedia('(prefers-reduced-motion: reduce)').matches?0:500;
        img.animate([{opacity:0},{opacity:1}],{duration});
        old.animate([{opacity:1},{opacity:0}],{duration}).onfinish=()=>old.remove();
      }
    }).catch(()=>{});
  }
  function receive(status:IllustrationStatus):void {
    if(!context)return;
    if(status.profile_id!==context.profile_id||status.phase==='disabled'){close();return;}
    const next=status.images.length?status.images:status.image?[status.image]:[];
    if(!next.length){close();return;}
    if(next[0]!==pictures[0])index=0;
    pictures=next;index%=pictures.length;paint();
  }
  async function refresh():Promise<void> {
    const request=++revision,value=await api.imageViewer();
    if(request!==revision)return;
    context=value;clearInterval(timer);++decode;pictures=[];index=0;
    if(!context){app.replaceChildren();return;}
    setLanguage(context.ui_language);paintTheme(context.theme);
    app.innerHTML=html`<main class="fullscreen-illustration" aria-label="Illustration en plein écran"><div class="fullscreen-image-layers" title="Double-clique pour quitter le plein écran"></div><button id="close-image-fullscreen" class="button secondary" title="Quitter le plein écran (Échap)" aria-label="Quitter le plein écran (Échap)">${icon('close')}<span>Quitter le plein écran (Échap)</span></button><p role="alert"></p></main>`;
    app.querySelector('#close-image-fullscreen')!.addEventListener('click',close);
    app.querySelector('.fullscreen-image-layers')!.addEventListener('dblclick',close);
    timer=setInterval(()=>{if(pictures.length>1){index=(index+1)%pictures.length;paint();}},context.slide_seconds*1000);
    const status=await api.illustration();
    if(request===revision)receive(status);
  }
  document.addEventListener('keydown',e=>{if(e.key==='Escape'){e.preventDefault();close();}});
  await api.onImageFullscreen(()=>void refresh());
  await api.onIllustration(receive);
  await refresh();
}
