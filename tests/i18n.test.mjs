import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';

const catalog=JSON.parse(readFileSync('config/locales/ui.json','utf8'));
function ui() {
  const document={documentElement:{lang:''},title:''};
  const context={exports:{},document,require:()=>({__esModule:true,default:catalog})};
  vm.runInNewContext(ts.transpileModule(readFileSync('src/i18n.ts','utf8'),{
    compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}
  }).outputText,context);
  return {...context.exports,document};
}
test('seven locales translate controls and preserve opaque values and HTML syntax',()=>{
  const api=ui();
  for(const [locale] of api.languages) {
    api.setLanguage(locale);
    assert.equal(api.document.documentElement.lang,locale);
    const label=api.t('Réglages');
    assert.equal(label,locale==='fr'?'Réglages':catalog.Réglages[api.languages.findIndex(([c])=>c===locale)-1]);
    const user='Réglages <script>value</script> {0} 中文 日本語';
    const rendered=api.html(['<button title="Réglages">','</button><p>Réglages</p>'],user);
    assert.ok(rendered.includes('title="'+label+'"'));
    assert.ok(rendered.includes('>'+user+'</button>'));
    assert.ok(rendered.includes('<p>'+label+'</p>'));
    assert.equal(api.html(['"><div class="memory-entry"></div>']),'"><div class="memory-entry"></div>');
    assert.equal(api.html(['disabled title="Réglages"']),'disabled title="'+label+'"');
    assert.equal(api.t(' · texte'),' '+api.t('· texte'));
    const icon='<svg></svg>';
    assert.ok(api.html(['<span>',' Enregistrer</span>'],icon).includes(icon));
    const reordered=api.html(['<b>','</b><p>Avec ','</p>'],'unchanged','Réglages');
    assert.ok(reordered.includes('Réglages'));
    assert.ok(!reordered.includes('{1}'));
  }
});
test('feedback translates authored templates without changing names or memories',()=>{
  const api=ui();api.setLanguage('en');
  assert.equal(api.feedback('Microphone coupé'),'Microphone off');
  assert.equal(api.feedback('VoiceStudio est inaccessible (HTTP 503). Vérifie son adresse et son accès dans les réglages.'),
    'VoiceStudio unreachable (HTTP 503). Check its address and access in Settings.');
  assert.equal(api.feedback('Réglages — model identifier'),'Réglages — model identifier');
  assert.equal(api.feedback('Souvenir #42 corrigé : Réglages'),'Memory #42 corrected: Réglages');
  assert.equal(api.t(['Voix enregistrée · ',''],'Réglages'),'Saved voice · Réglages');
});
test('unsupported interface locale falls back to French',()=>{
  const api=ui();api.setLanguage('xx');
  assert.equal(api.document.documentElement.lang,'fr');
  assert.equal(api.t('Réglages'),'Réglages');
});
