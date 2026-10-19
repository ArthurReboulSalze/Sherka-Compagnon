import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';

const workflow=JSON.parse(readFileSync('config/workflows/illustrious.json','utf8'));
function storage(){const data=new Map();return {data,getItem:key=>data.get(key)??null,setItem:(key,value)=>data.set(key,value)};}
function cache(localStorage){
  const context={exports:{},URL,structuredClone,localStorage,require:()=>({__esModule:true,default:workflow})};
  vm.runInNewContext(ts.transpileModule(readFileSync('src/illustration-catalog.ts','utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);
  return context.exports;
}
const config={server_url:'http://127.0.0.1:8188',workflow:null};
const catalog={checkpoints:['Models/landscape.safetensors'],loras:['Style/watercolor.safetensors'],samplers:['euler','dpmpp_2m'],turbo_available:true,missing_nodes:['ttN text']};
test('catalog survives a fresh app session, is shared by server, and scopes node checks to its workflow',async()=>{
  const disk=storage(),a=cache(disk);let scans=0;
  await a.scanCatalog(config,async()=>{scans++;return catalog;});
  const b=cache(disk),same={...config,server_url:'HTTP://127.0.0.1:8188/'};
  assert.equal(b.cachedCatalog(same).loras[0],catalog.loras[0]);assert.equal(scans,1);
  const changedGraph={...same,workflow:{'1':{class_type:'CheckpointLoaderSimple'}}};
  assert.equal(b.cachedCatalog(changedGraph).checkpoints[0],catalog.checkpoints[0]);
  assert.equal(b.cachedCatalog(changedGraph).missing_nodes.length,0);
  assert.equal(b.cachedCatalog(config).missing_nodes[0],'ttN text');
  assert.equal(b.cachedCatalog({...config,server_url:'http://127.0.0.1:8189'}),undefined);
  const returned=b.cachedCatalog(config);returned.loras.length=0;
  assert.equal(b.cachedCatalog(config).loras.length,1);
  assert.equal(b.catalogKey({...config,server_url:'http://secret@127.0.0.1:8188'}),undefined);
});
test('simultaneous requests share a scan, failed refresh keeps the last usable list, and success replaces stale filenames',async()=>{
  const a=cache(storage());let scans=0,finish;
  const discover=()=>{scans++;return new Promise(done=>finish=done);};
  const first=a.scanCatalog(config,discover),second=a.scanCatalog(config,discover);
  await Promise.resolve();assert.equal(scans,1);assert.equal(first,second);finish(catalog);await first;
  assert.equal(a.pendingCatalog(config),undefined);
  await assert.rejects(a.scanCatalog(config,async()=>{throw new Error('offline');}));
  assert.equal(a.cachedCatalog(config).loras[0],catalog.loras[0]);
  await a.scanCatalog(config,async()=>({...catalog,loras:['Style/new.safetensors']}));
  assert.equal(a.cachedCatalog(config).loras.length,1);assert.equal(a.cachedCatalog(config).loras[0],'Style/new.safetensors');
});
test('corrupt storage or a full storage quota never prevents scanning and selecting models',async()=>{
  for(const disk of [{getItem:()=>'{bad json',setItem:()=>{}},{getItem:()=>null,setItem:()=>{throw new Error('quota');}}]){
    const a=cache(disk);assert.equal(a.cachedCatalog(config),undefined);
    await a.scanCatalog(config,async()=>catalog);assert.equal(a.cachedCatalog(config).loras.length,1);
  }
});
