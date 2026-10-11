import ts from 'typescript';
import {readFileSync,readdirSync,writeFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const catalogPath='config/locales/ui.json';
const keys = new Set();
const normalize = text => { let index=0;return text.trim().replace(/\{\d+\}/g,()=>`{${index++}}`); };
const add = text => { const key=normalize(text); if(key && /[A-Za-zÀ-ÿ]/.test(key) && !/[<>]|="/.test(key)) keys.add(key); };
for(const file of readdirSync('src').filter(name=>name.endsWith('.ts')&&!['i18n.ts','icons.ts'].includes(name))) {
  const source=readFileSync('src/'+file,'utf8');
  const tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
  function visit(node) {
    if(ts.isCallExpression(node)&&node.expression.getText(tree)==='t'&&node.arguments[0]&&ts.isStringLiteral(node.arguments[0])) add(node.arguments[0].text);
    if(ts.isTaggedTemplateExpression(node)&&['t','html'].includes(node.tag.getText(tree))) {
      const template=node.template;
      const text=ts.isNoSubstitutionTemplateLiteral(template)?template.text:template.head.text+template.templateSpans.map((span,index)=>`{${index}}`+span.literal.text).join('');
      if(node.tag.getText(tree)==='t')add(text);
      else {
        if(text.includes('<')) for(const match of text.matchAll(/(?:^|>)([^<>]*)(?=<|$)/g))add(match[1]);
        for(const match of text.matchAll(/\b(?:aria-label|title|alt|placeholder)="([^"<>]*)"/g))add(match[1]);
      }
    }
    ts.forEachChild(node,visit);
  }
  visit(tree);
}
// Native feedback uses the same authored sources, with opaque format slots.
const prose = text => /[àâäéèêëîïôöùûüçÀÉÈÊ]|(?:comp[a-z]+|[Mm]icrophone|[Ss]ouvenir|[Vv]oix|[Cc]ontexte|[Ii]mage|[Pp]rofil)|\b(?:est|les|pas|trop|vocale|reprendre)\b|^(?:La |Le |Les |L'|Ce |Cette |Un |Une |Aucun |Aucune |Choisis |Chemin )/.test(text) && /\s/.test(text) && !/^(?:SELECT |INSERT |UPDATE |DELETE |PRAGMA |CREATE |DROP |ALTER |Tu es Sherka|Reste honnête|souviens |efface de ta|supprime de ta|peux.tu oublier)/.test(text);
for(const dir of ['crates/sherka-core/src','src-tauri/src']) {
  for(const name of readdirSync(dir).filter(name=>name.endsWith('.rs')&&!name.includes('test')&&!['i18n.rs'].includes(name))) {
    const source=readFileSync(dir+'/'+name,'utf8').split('#[cfg(test)]')[0];
    for(const match of source.matchAll(/"(?:[^"\\]|\\.)*"/g)) {
      let value;try{value=JSON.parse(match[0]);}catch{continue;}
      if(value.length>350||!prose(value)||/[<>]/.test(value))continue;
      let index=0;value=value.replace(/\{(?:[^{}]*)\}/g,()=>`{${index++}}`);
      add(value);
    }
  }
}
for(const key of ['Sherka — Compagnon de bureau','Sherka — Poste de bord','Sherka — Compagnon','Langue de l’interface enregistrée.','Enregistrement…','Détail','Essentiel','corrigé','enregistré'])add(key);
const sorted=[...keys].sort((a,b)=>a.localeCompare(b,'fr'));
if(process.argv.includes('--extract')) {
  writeFileSync('artifacts/i18n-keys.json',JSON.stringify(sorted,null,2));
  writeFileSync('artifacts/i18n-numbered.txt',sorted.map((key,index)=>`${index}: ${key}`).join('\n'));
  console.log(`Extracted ${sorted.length} authored UI/feedback strings.`);
} else {
  const catalog=JSON.parse(readFileSync(catalogPath,'utf8'));
  const placeholders=text=>(text.match(/\{\d+\}/g)??[]).sort().join(',');
  for(const key of sorted) {
    assert.ok(catalog[key],`Missing translation: ${key}`);
    assert.equal(catalog[key].length,6,`Six translations required: ${key}`);
    for(const value of catalog[key]) {
      assert.ok(typeof value==='string'&&value.trim(),`Empty translation: ${key}`);
      assert.equal(placeholders(value),placeholders(key),`Changed placeholders: ${key}`);
      assert.ok(!/[<>]/.test(value),`HTML is forbidden in translated text: ${key}`);
    }
  }
  console.log(`All ${sorted.length} authored messages have 7 languages and intact placeholders.`);
}
