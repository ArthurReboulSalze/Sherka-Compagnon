import type {IllustrationCatalog,IllustrationConfig} from './types';
import defaultWorkflow from '../config/workflows/illustrious.json';

// This device-only cache contains filenames/capabilities, never prompts, keys
// or companion data. Image generation still validates against the real server.
const storageKey='sherka.comfyui.catalog.v1',maxEntries=8,maxCharacters=2_000_000;
type Entry={server:string;workflow:string;catalog:IllustrationCatalog};
const entries=new Map<string,Entry>(),pending=new Map<string,Promise<IllustrationCatalog>>();
let loaded=false;
function scope(config:IllustrationConfig):{server:string;workflow:string}|undefined {
  try {
    const url=new URL(config.server_url.trim());
    if(!['http:','https:'].includes(url.protocol)||url.username||url.password||url.search||url.hash||url.pathname!=='/')return;
    const classes=Object.values(config.workflow??defaultWorkflow).map(node=>node&&typeof node==='object'&&'class_type'in node?node.class_type:undefined).filter((value):value is string=>typeof value==='string');
    return {server:url.origin,workflow:JSON.stringify([...new Set(classes)].sort())};
  }catch{return;}
}
export function catalogKey(config:IllustrationConfig):string|undefined {
  const value=scope(config);return value?JSON.stringify([value.server,value.workflow]):undefined;
}
function validCatalog(value:unknown):value is IllustrationCatalog {
  if(!value||typeof value!=='object')return false;
  const c=value as IllustrationCatalog;
  return typeof c.turbo_available==='boolean'&&[c.checkpoints,c.loras,c.samplers,c.missing_nodes,c.ipadapters??[],c.clip_visions??[],c.upscalers??[]].every(list=>Array.isArray(list)&&list.length<=20_000&&list.every(name=>typeof name==='string'&&name.length<=512&&!/[\u0000-\u001f]/.test(name)));
}
function load():void {
  if(loaded)return;loaded=true;
  try {
    const raw=localStorage.getItem(storageKey);if(!raw||raw.length>maxCharacters)return;
    const saved:unknown=JSON.parse(raw);if(!Array.isArray(saved)||saved.length>maxEntries)return;
    for(const row of saved) {
      if(!row||typeof row.server!=='string'||row.server.length>2048||typeof row.workflow!=='string'||row.workflow.length>8192||!validCatalog(row.catalog))continue;
      entries.set(row.server,row as Entry);
    }
  }catch{/* A corrupt or unavailable cache never blocks the settings. */}
}
function persist():void {
  const saved=[...entries.values()].filter(entry=>JSON.stringify(entry).length<=maxCharacters);
  while(saved.length&&JSON.stringify(saved).length>maxCharacters)saved.shift();
  try {
    // Evict older servers if the browser's available storage quota is lower.
    while(saved.length) {
      try{localStorage.setItem(storageKey,JSON.stringify(saved));return;}catch{saved.shift();}
    }
  }catch{/* Browsing the in-memory catalog still works when storage is full. */}
}
export function cachedCatalog(config:IllustrationConfig):IllustrationCatalog|undefined {
  load();const current=scope(config);if(!current)return;
  const entry=entries.get(current.server);if(!entry)return;
  const catalog=structuredClone(entry.catalog);
  // Model/LoRA lists are shared by the server; node checks belong to the graph.
  if(entry.workflow!==current.workflow)catalog.missing_nodes=[];
  return catalog;
}
export function pendingCatalog(config:IllustrationConfig):Promise<IllustrationCatalog>|undefined {
  const key=catalogKey(config);return key?pending.get(key):undefined;
}
export function scanCatalog(config:IllustrationConfig,discover:(config:IllustrationConfig)=>Promise<IllustrationCatalog>):Promise<IllustrationCatalog> {
  const key=catalogKey(config),existing=key?pending.get(key):undefined;if(existing)return existing;
  const captured=structuredClone(config),current=scope(captured);
  const request=Promise.resolve().then(()=>discover(captured)).then(catalog=>{
    if(current&&validCatalog(catalog)) {
      load();entries.delete(current.server);entries.set(current.server,{...current,catalog:structuredClone(catalog)});
      while(entries.size>maxEntries)entries.delete(entries.keys().next().value!);
      persist();
    }
    return catalog;
  });
  const tracked=request.finally(()=>{if(key&&pending.get(key)===tracked)pending.delete(key);});
  if(key)pending.set(key,tracked);return tracked;
}
