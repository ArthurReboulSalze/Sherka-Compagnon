import {html,t} from './i18n';
import type {Theme} from './types';
export const themes=():{id:Theme;name:string}[]=>[
  {id:'gold',name:t('Or orbital')},{id:'sage',name:t('Jardin de sauge')},{id:'ocean',name:t('Océan nocturne')},
  {id:'iris',name:t('Nébuleuse iris')},{id:'rose',name:t('Rose crépuscule')},{id:'mono',name:t('Graphite')},
];
export function paintTheme(theme:Theme):void {document.documentElement.dataset.theme=theme??'gold';}
export function themesView(selected:Theme):string {
  return html`<section class="settings-card"><h2>Ambiance de l’application</h2><p>Choisis les couleurs du poste de bord. Chaque compagnon garde sa propre couleur de halo.</p><div class="theme-gallery" role="group" aria-label="Thèmes de couleurs">${themes().map(theme=>html`<button type="button" class="theme-swatch theme-${theme.id}" data-theme-choice="${theme.id}" aria-pressed="${selected===theme.id}"><i></i>${theme.name}</button>`).join('')}</div></section>`;
}
