import { formatLocale } from './i18n';

export function bindRangeControls(root: ParentNode): void {
  const percentage = new Intl.NumberFormat(formatLocale(), { style: 'percent', maximumFractionDigits: 0 });
  root.querySelectorAll<HTMLInputElement>('input[type="range"]').forEach(input => {
    const output = input.parentElement?.querySelector<HTMLOutputElement>('output');
    const update = () => {
      const progress = (input.valueAsNumber - Number(input.min)) / (Number(input.max) - Number(input.min)) * 100;
      input.style.setProperty('--range-progress', `${progress}%`);
      if (output) output.value = percentage.format(input.valueAsNumber * Number(output.dataset.percentScale ?? 1));
    };
    update();
    input.addEventListener('input', update);
  });
}
