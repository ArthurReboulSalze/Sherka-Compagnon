import {html,t} from './i18n';
import type {IllustrationConfig,IllustrationCatalog,ImageResolution,ImageReference} from './types';
const esc=(s:string)=>s.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
export const resolutions=[[768,768],[1024,1024],[768,1024],[1024,768],[768,1152],[1152,768],[576,1024],[1024,576]] as const;
export const resolution=(c:IllustrationConfig):ImageResolution=>c.resolution??{width:c.advanced.enabled?c.advanced.size:768,height:c.advanced.enabled?c.advanced.size:768,latent_scale:1,upscale_model:null};
export const reference=(c:IllustrationConfig):ImageReference=>({...{enabled:false,model:null,clip_vision:null,weight:0.75},...c.reference});
function format(w:number,h:number):string {const gcd=(a:number,b:number):number=>b?gcd(b,a%b):a;const d=gcd(w,h);return `${w} × ${h} · ${w/d}:${h/d}`;}
export function imageOptions(c:IllustrationConfig):string {
 const r=resolution(c),ref=reference(c);
 // Include a legacy custom square without rewriting its saved dimensions.
 const options:number[][]=[...resolutions.map(([w,h])=>[w,h])];if(!options.some(([w,h])=>w===r.width&&h===r.height))options.push([r.width,r.height]);
 return html`<fieldset class="image-output-options"><legend>Format et qualité</legend><div class="connection-selects"><label>Résolution de départ<select name="image_resolution">${options.map(([w,h])=>`<option value="${w}x${h}" ${w===r.width&&h===r.height?'selected':''}>${format(w!,h!)}</option>`).join('')}</select></label><label>Agrandissement entre les samplers<select name="image_latent_scale"><option value="1" ${r.latent_scale===1?'selected':''}>×1</option><option value="1.5" ${r.latent_scale===1.5?'selected':''}>×1,5</option></select></label><label>Upscale final ×2<select name="image_upscaler"><option value="">${t('Désactivé')}</option>${r.upscale_model?`<option selected value="${esc(r.upscale_model)}">${esc(r.upscale_model)}</option>`:''}</select></label></div><p id="image-output-size" class="connection-note"></p><p class="connection-note">Les modèles ×2 sont les plus directs. Un modèle ×4 est ramené à la taille ×2 annoncée. La vitesse dépend du modèle choisi.</p></fieldset>
 <fieldset class="image-reference-options"><legend>Référence visuelle du compagnon</legend><label class="setting-row"><span><strong>Utiliser le portrait neutre avec IP-Adapter</strong><small>Activé : le portrait guide les scènes avec le compagnon. Désactivé : seuls les textes guident l’image. Ce choix ne change pas la création des portraits.</small></span><input class="switch" name="image_reference" type="checkbox" ${ref.enabled?'checked':''}/></label><div class="connection-selects"><label>IP-Adapter SDXL<select name="image_ipadapter"><option value="">${t('Choisir un modèle')}</option>${ref.model?`<option selected value="${esc(ref.model)}">${esc(ref.model)}</option>`:''}</select></label><label>CLIP Vision<select name="image_clip_vision"><option value="">${t('Choisir un modèle')}</option>${ref.clip_vision?`<option selected value="${esc(ref.clip_vision)}">${esc(ref.clip_vision)}</option>`:''}</select></label><label>Fidélité au portrait<span class="range-control"><input name="image_reference_weight" type="range" min="0" max="1.5" step="0.05" value="${ref.weight}"/><output>${ref.weight.toFixed(2)}</output></span></label></div><p class="connection-note">Associe le bon encodeur : IP-Adapter SDXL standard utilise CLIP ViT-bigG ; les variantes ViT-H utilisent CLIP ViT-H. FaceID est exclu pour préserver aussi les robots et les objets.</p></fieldset>`;
}
export function captureImageOptions(form:HTMLFormElement):Pick<IllustrationConfig,'resolution'|'reference'>{
 const f=(n:string)=>form.elements.namedItem(n) as HTMLInputElement;
 const [width,height]=f('image_resolution').value.split('x').map(Number);
 return {resolution:{width:width!,height:height!,latent_scale:Number(f('image_latent_scale').value),upscale_model:f('image_upscaler').value||null},
 reference:{enabled:f('image_reference').checked,model:f('image_ipadapter').value||null,clip_vision:f('image_clip_vision').value||null,weight:Number(f('image_reference_weight').value)}};
}
export function paintImageSize(root:HTMLElement,c:IllustrationConfig):void {
 const r=resolution(c),w=Math.round(r.width*r.latent_scale/8)*8,h=Math.round(r.height*r.latent_scale/8)*8,m=r.upscale_model?2:1;
 const node=root.querySelector('#image-output-size');if(node)node.textContent=t`Départ ${r.width} × ${r.height} → seconde passe ${w} × ${h} → sortie ${w*m} × ${h*m} px`;
}
export function paintImageCatalog(form:HTMLFormElement,c:IllustrationConfig,catalog?:IllustrationCatalog):void {
 const ref=reference(c),r=resolution(c);
 if(!ref.model&&catalog?.ipadapters?.includes('ip-adapter_sdxl.safetensors'))ref.model='ip-adapter_sdxl.safetensors';
 if(!ref.clip_vision&&ref.model==='ip-adapter_sdxl.safetensors')ref.clip_vision=catalog?.clip_visions?.find(s=>/bigG|clip_vision_g\./i.test(s))??null;
 c.reference=ref;
 const set=(name:string,selected:string|null,values:string[])=>{
  const el=form.elements.namedItem(name) as HTMLSelectElement;
  el.innerHTML=`<option value="">${name==='image_upscaler'?t('Désactivé'):t('Choisir un modèle')}</option>`+[...new Set([...(selected?[selected]:[]),...values])].map(s=>`<option value="${esc(s)}" ${s===selected?'selected':''}>${esc(s)}</option>`).join('');
 };
 set('image_upscaler',r.upscale_model,[...(catalog?.upscalers??[])].sort((a,b)=>Number(!/x2|2x/i.test(a))-Number(!/x2|2x/i.test(b))||a.localeCompare(b)));
 set('image_ipadapter',ref.model,catalog?.ipadapters??[]);
 set('image_clip_vision',ref.clip_vision,catalog?.clip_visions??[]);
 paintImageSize(form,c);
}
