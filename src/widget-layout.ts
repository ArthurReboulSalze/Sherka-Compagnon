// Anchor chrome to visible pixels, rather than the transparent square canvas.
// This keeps controls close to wide cars and avatars with generous margins.
export type Bounds={x:number;y:number;width:number;height:number};
const bounds=new Map<string,Bounds>();
export async function alphaBounds(src:string):Promise<Bounds> {
  const saved=bounds.get(src);if(saved)return saved;
  const img=new Image();img.src=src;await img.decode();
  const scale=Math.min(1,384/Math.max(img.naturalWidth,img.naturalHeight));
  const canvas=document.createElement('canvas');canvas.width=Math.max(1,Math.round(img.naturalWidth*scale));canvas.height=Math.max(1,Math.round(img.naturalHeight*scale));
  const ctx=canvas.getContext('2d',{willReadFrequently:true})!;ctx.drawImage(img,0,0,canvas.width,canvas.height);
  const rgba=ctx.getImageData(0,0,canvas.width,canvas.height).data;
  let left=canvas.width,top=canvas.height,right=0,bottom=0;
  for(let y=0;y<canvas.height;y++)for(let x=0;x<canvas.width;x++)if(rgba[(y*canvas.width+x)*4+3]!>24){left=Math.min(left,x);top=Math.min(top,y);right=Math.max(right,x+1);bottom=Math.max(bottom,y+1);}
  const result=right>left?{x:left/canvas.width,y:top/canvas.height,width:(right-left)/canvas.width,height:(bottom-top)/canvas.height}:{x:0,y:0,width:1,height:1};
  if(bounds.size>=40)bounds.delete(bounds.keys().next().value!);bounds.set(src,result);return result;
}
/** The opaque pixels' rectangle, in `root` coordinates, for an `object-fit: contain` image filling `box`. */
export function visibleRect(image:HTMLImageElement,box:DOMRect,root:DOMRect,alpha:Bounds):{left:number;top:number;width:number;height:number} {
  const scale=Math.min(box.width/(image.naturalWidth||1),box.height/(image.naturalHeight||1));
  const w=image.naturalWidth*scale,h=image.naturalHeight*scale;
  return {left:box.left-root.left+(box.width-w)/2+alpha.x*w,top:box.top-root.top+(box.height-h)/2+alpha.y*h,width:alpha.width*w,height:alpha.height*h};
}
let stop:(()=>void)|undefined;
export function layoutWidget():void {
  stop?.();
  const surface=document.querySelector<HTMLElement>('.widget');const portrait=surface?.querySelector<HTMLElement>('.widget-portrait');
  if(!surface||!portrait)return;
  let frame=0,request=0;
  const update=async()=>{
    const version=++request;const image=portrait.querySelector<HTMLImageElement>('img:not(.avatar-outgoing)');if(!image)return;
    const toolbar=surface.querySelector<HTMLElement>('.widget-toolbar')!;
    portrait.style.bottom=`${toolbar.offsetHeight+52}px`;
    let alpha:Bounds;
    try{alpha=await alphaBounds(image.src);}catch{alpha={x:0,y:0,width:1,height:1};}
    if(version!==request||!surface.isConnected)return;
    const box=portrait.getBoundingClientRect(),root=surface.getBoundingClientRect();
    const visible=visibleRect(image,box,root,alpha);
    const name=surface.querySelector<HTMLElement>('.widget-name')!;const status=surface.querySelector<HTMLElement>('.widget-status')!;
    name.style.top=`${Math.max(3,visible.top-name.offsetHeight-6)}px`;
    const statusTop=Math.min(surface.clientHeight-status.offsetHeight-toolbar.offsetHeight-30,visible.top+visible.height+6);
    status.style.top=`${statusTop}px`;toolbar.style.top=`${statusTop+status.offsetHeight+5}px`;
    const resize=surface.querySelector<HTMLElement>('.resize-handle')!;
    resize.style.top=`${Math.max(4,Math.min(surface.clientHeight-22,visible.top+visible.height+3))}px`;
    resize.style.left=`${Math.max(4,Math.min(surface.clientWidth-23,visible.left+visible.width+2))}px`;
    document.dispatchEvent(new Event('sherka:widget-layout'));
  };
  const schedule=()=>{cancelAnimationFrame(frame);frame=requestAnimationFrame(()=>void update());};
  const size=new ResizeObserver(schedule);size.observe(surface);size.observe(surface.querySelector('.widget-toolbar')!);
  const images=new MutationObserver(schedule);images.observe(portrait,{childList:true,subtree:true,attributes:true,attributeFilter:['src']});
  portrait.addEventListener('load',schedule,true);schedule();
  stop=()=>{++request;cancelAnimationFrame(frame);size.disconnect();images.disconnect();portrait.removeEventListener('load',schedule,true);};
}
