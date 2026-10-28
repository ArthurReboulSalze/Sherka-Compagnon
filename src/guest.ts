import { html, setLanguage } from './i18n';
import * as api from './api';
import { icon } from './icons';
import { paintGuestAvatar, portrait } from './avatar';
import { paintTheme } from './themes';
import { alphaBounds, visibleRect, type Bounds } from './widget-layout';
import type { Snapshot } from './types';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';

const escape = (text: string) => text.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!));

/** Anchors the name chip, remove button and resize handle to the portrait's visible pixels, like the master widget. */
function layoutGuest(root: HTMLElement): () => void {
  const portraitBox = root.querySelector<HTMLElement>('.guest-portrait')!;
  const name = root.querySelector<HTMLElement>('.guest-name')!;
  const remove = root.querySelector<HTMLElement>('.guest-remove')!;
  const resize = root.querySelector<HTMLElement>('.guest-resize')!;
  let frame = 0, request = 0;
  const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(Math.max(min, max), value));
  const update = async () => {
    const version = ++request;
    const image = portraitBox.querySelector<HTMLImageElement>('img:not(.avatar-outgoing)');
    if (!image || !image.getAttribute('src')) return;
    let alpha: Bounds;
    try { alpha = await alphaBounds(image.src); } catch { alpha = { x: 0, y: 0, width: 1, height: 1 }; }
    if (version !== request || !root.isConnected || !image.naturalWidth) return;
    const visible = visibleRect(image, portraitBox.getBoundingClientRect(), root.getBoundingClientRect(), alpha);
    const width = root.clientWidth, height = root.clientHeight;
    const half = name.offsetWidth / 2;
    name.style.left = `${clamp(visible.left + visible.width / 2, half + 4, width - half - 4)}px`;
    name.style.top = `${clamp(visible.top - name.offsetHeight - 6, 3, height - name.offsetHeight - 3)}px`;
    remove.style.left = `${clamp(visible.left + visible.width + 2, 4, width - remove.offsetWidth - 4)}px`;
    remove.style.top = `${clamp(visible.top - remove.offsetHeight - 2, 4, height - remove.offsetHeight - 4)}px`;
    resize.style.left = `${clamp(visible.left + visible.width + 2, 4, width - resize.offsetWidth - 4)}px`;
    resize.style.top = `${clamp(visible.top + visible.height + 3, 4, height - resize.offsetHeight - 4)}px`;
  };
  const schedule = () => { cancelAnimationFrame(frame); frame = requestAnimationFrame(() => void update()); };
  const size = new ResizeObserver(schedule);
  size.observe(root);
  const images = new MutationObserver(schedule);
  images.observe(portraitBox, { childList: true, subtree: true, attributes: true, attributeFilter: ['src'] });
  portraitBox.addEventListener('load', schedule, true);
  schedule();
  return () => { ++request; cancelAnimationFrame(frame); size.disconnect(); images.disconnect(); portraitBox.removeEventListener('load', schedule, true); };
}

/** The guest companion's own small window: portrait, name and a speaking halo. */
export async function mountGuest(report: (error: unknown) => void): Promise<void> {
  document.body.classList.add('guest-body', 'widget-body');
  const app = document.querySelector<HTMLDivElement>('#app')!;
  let state: Snapshot | undefined;
  let version = 0;
  let identity = '';
  let stopLayout: (() => void) | undefined;
  let pointerInside = false, keyboardControls = false, pinned = false;

  const view = () => app.querySelector<HTMLElement>('.guest-view');
  const updateControls = () => {
    view()?.classList.toggle('controls-visible', pointerInside || keyboardControls || pinned);
    app.querySelector('.guest-portrait')?.setAttribute('aria-pressed', String(pinned));
  };
  const dismissControls = () => {
    pointerInside = keyboardControls = pinned = false;
    const root = view();
    if (root?.contains(document.activeElement)) (document.activeElement as HTMLElement).blur();
    updateControls();
  };

  function bind(root: HTMLElement): void {
    const portraitButton = root.querySelector<HTMLButtonElement>('.guest-portrait')!;
    root.querySelector<HTMLButtonElement>('.guest-remove')!.addEventListener('click', event => {
      const button = event.currentTarget as HTMLButtonElement;
      button.disabled = true;
      api.setGuestCompanion(null).catch(error => { report(error); button.disabled = false; });
    });
    root.addEventListener('focusin', event => {
      if ((event.target as Element).matches(':focus-visible')) { keyboardControls = true; updateControls(); }
    });
    root.addEventListener('focusout', event => {
      if (root.contains(event.relatedTarget as Node | null)) return;
      setTimeout(() => {
        if (!root.isConnected || root.contains(document.activeElement)) return;
        keyboardControls = pinned = false;
        updateControls();
      }, 0);
    });
    // Dragging the portrait moves the window, once the pointer has travelled 5 px.
    let press: { x: number; y: number } | undefined;
    let dragged = false;
    portraitButton.addEventListener('pointerdown', event => {
      if (event.button !== 0) return;
      press = { x: event.screenX, y: event.screenY };
      dragged = false;
      portraitButton.setPointerCapture(event.pointerId);
    });
    portraitButton.addEventListener('pointermove', event => {
      if (!press || Math.hypot(event.screenX - press.x, event.screenY - press.y) < 5) return;
      press = undefined;
      dragged = true;
      event.stopPropagation();
      portraitButton.releasePointerCapture(event.pointerId);
      dismissControls();
      void getCurrentWindow().startDragging().catch(report);
    });
    for (const name of ['pointerup', 'pointercancel']) portraitButton.addEventListener(name, () => { press = undefined; });
    portraitButton.addEventListener('click', () => {
      if (dragged) return;
      pinned = !pinned;
      updateControls();
    });
    // Resizing keeps the same limits as the main widget's avatar.
    root.querySelector<HTMLElement>('.guest-resize')!.addEventListener('pointerdown', event => {
      if (event.button !== 0) return;
      event.preventDefault();
      const handle = event.currentTarget as HTMLElement;
      handle.setPointerCapture(event.pointerId);
      const start = { x: event.screenX, y: event.screenY, width: window.innerWidth, height: window.innerHeight };
      let next: LogicalSize | undefined;
      let sending = false;
      const flush = async () => {
        if (sending) return;
        sending = true;
        try {
          while (next) {
            const size = next;
            next = undefined;
            await getCurrentWindow().setSize(size);
          }
        } catch (error) { report(error); }
        finally { sending = false; }
      };
      const move = (e: PointerEvent) => {
        next = new LogicalSize(Math.max(188, Math.min(600, start.width + e.screenX - start.x)), Math.max(228, Math.min(720, start.height + e.screenY - start.y)));
        void flush();
      };
      const end = () => {
        handle.removeEventListener('pointermove', move);
        handle.removeEventListener('pointerup', end);
        handle.removeEventListener('pointercancel', end);
      };
      handle.addEventListener('pointermove', move);
      handle.addEventListener('pointerup', end);
      handle.addEventListener('pointercancel', end);
    });
  }

  function paint(): void {
    if (!state) return;
    setLanguage(state.settings.ui_language);
    paintTheme(state.settings.theme);
    const guest = state.profiles.find(p => p.id === state!.session.guest_profile_id);
    // Without a guest, show nothing: the desktop shell closes this window.
    if (!guest) { identity = ''; stopLayout?.(); stopLayout = undefined; app.replaceChildren(); return; }
    app.setAttribute('aria-busy', 'false');
    const key = JSON.stringify([guest.id, guest.name, guest.accent, state.settings.ui_language]);
    if (key !== identity || !view()) {
      identity = key;
      app.innerHTML = html`<div class="guest-canvas accent-${guest.accent}"><main class="guest-view"><div class="guest-name"><span>${escape(guest.name)}</span></div><button type="button" class="guest-portrait" aria-label="Afficher ou masquer les commandes de ${escape(guest.name)}" aria-pressed="false" title="Survole ou clique pour les commandes, fais glisser pour déplacer"><img draggable="false" src="${portrait(guest)}" alt="${escape(guest.name)}" /></button><button type="button" class="icon-button guest-remove guest-chrome" aria-label="Retirer de la conversation" title="Retirer de la conversation">${icon('close')}</button><button type="button" class="guest-resize guest-chrome" aria-label="Redimensionner" title="Redimensionner">${icon('resize')}</button></main></div>`;
      stopLayout?.();
      bind(view()!);
      stopLayout = layoutGuest(view()!);
      updateControls();
    }
    paintGuestAvatar(state, app.querySelector<HTMLImageElement>('.guest-portrait > img:not(.avatar-outgoing)')!);
    view()!.classList.toggle('speaking', state.live.tag?.profile_id === guest.id && state.session.response === 'speaking');
  }

  async function refresh(): Promise<void> {
    const request = ++version;
    try {
      const next = await api.getSnapshot();
      if (request !== version) return;
      state = next;
      paint();
    } catch (error) { report(error); }
  }

  // Hover and focus state can go stale around native window drags: follow the live pointer.
  document.addEventListener('pointermove', () => { if (!pointerInside) { pointerInside = true; updateControls(); } });
  document.addEventListener('pointerleave', dismissControls);
  window.addEventListener('blur', dismissControls);
  document.addEventListener('visibilitychange', () => { if (document.hidden) dismissControls(); });
  await api.onChanged(() => void refresh());
  await api.onLive(value => {
    if (!state || value.session.profile_id !== state.settings.active_profile) return;
    state.session = value.session;
    state.live = value.live;
    paint();
  });
  await refresh();
}
