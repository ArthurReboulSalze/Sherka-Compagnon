import { test, expect, chromium, type Browser, type Page } from '@playwright/test';
import { spawn, execFileSync, type ChildProcess } from 'node:child_process';
import { mkdirSync, existsSync, writeFileSync, readFileSync, copyFileSync, readdirSync } from 'node:fs';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { resolve } from 'node:path';
import { randomUUID } from 'node:crypto';

let executable = resolve(process.env.SHERKA_TEST_EXECUTABLE ?? 'target/debug/sherka-compagnon.exe');
let child: ChildProcess;
let browser: Browser;
let panel: Page;
let widget: Page;
let dataDirectory: string;
let consoleErrors: string[];
let debugPort: number;

function modelRoutes(ids: string[], context = 8192, thoughts: Record<string, string[]> = {}) {
  const loaded = new Set(ids);
  return (request: IncomingMessage, response: ServerResponse): boolean => {
    if (request.url === '/api/v1/models') {
      response.writeHead(200, {'Content-Type':'application/json'});
      response.end(JSON.stringify({models:ids.map(id => ({type:'llm',key:id,display_name:id,capabilities:{reasoning:thoughts[id]?{allowed_options:thoughts[id],default:thoughts[id].at(-1)}:undefined},quantization:null,max_context_length:262144,loaded_instances:loaded.has(id) ? [{id,config:{context_length:context}}] : []}))}));
      return true;
    }
    if (request.url === '/api/v1/models/load' || request.url === '/api/v1/models/unload') {
      let body = '';
      request.on('data', bytes => {body += bytes;});
      request.on('end', () => {
        const value = JSON.parse(body);
        if (request.url!.endsWith('/load')) loaded.add(value.model); else loaded.delete(value.instance_id);
        response.writeHead(200, {'Content-Type':'application/json'}); response.end('{}');
      });
      return true;
    }
    return false;
  };
}

async function native<T>(command: string, args?: unknown): Promise<T> {
  return panel.evaluate(async ({ command, args }) => {
    return (window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args?: unknown) => Promise<unknown> } }).__TAURI_INTERNALS__.invoke(command, args);
  }, { command, args }) as Promise<T>;
}

async function startApplication(hidden = true, autostartVoice = false): Promise<void> {
  if (!existsSync(executable)) throw new Error('Construis le binaire : npm run tauri -- build --debug --no-bundle');
  // A new port prevents a closing WebView2 from satisfying the next launch.
  const port = await new Promise<number>((done,fail) => {
    const reservation=createServer();
    reservation.on('error',fail);
    reservation.listen(0,'127.0.0.1',()=>{
      const address=reservation.address();
      if(!address||typeof address==='string')return fail(new Error('Port CDP indisponible'));
      reservation.close(()=>done(address.port));
    });
  });
  debugPort = port;
  child = spawn(executable, [], {
    windowsHide: true,
    stdio: 'pipe',
    env: {
      ...process.env,
      SHERKA_DATA_DIR: dataDirectory,
      SHERKA_AUTOSTART_VOICE: autostartVoice ? '1' : '0',
      SHERKA_TEST_HIDDEN: hidden ? '1' : '0',
      WEBVIEW2_USER_DATA_FOLDER: resolve(dataDirectory, 'webview'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows`,
    },
  });
  child.stderr?.on('data', (data) => process.stderr.write(data));
  child.stdout?.on('data', (data) => process.stdout.write(data));
  try { await expect.poll(async () => {
    if (child.exitCode !== null) throw new Error(`Sherka s'est arrêtée : ${child.exitCode}`);
    try { return (await fetch(`http://127.0.0.1:${port}/json/version`)).ok; } catch { return false; }
  }, { timeout: 30_000 }).toBe(true); } catch (error) {
    if (child.pid) {
      try { console.error('Native startup diagnostics:', execFileSync('powershell.exe', ['-NoProfile','-ExecutionPolicy','Bypass','-File','tests/native/startup-diagnostics.ps1','-TargetProcessId',String(child.pid)], {encoding:'utf8',windowsHide:true,timeout:10_000})); } catch {}
    }
    throw error;
  }
  await expect.poll(async () => {
    const pages = await (await fetch('http://127.0.0.1:' + port + '/json/list')).json() as {url:string}[];
    return pages.some(p => p.url.includes('view=widget')) && pages.some(p => p.url.includes('tauri.localhost') && !p.url.includes('view=widget'));
  }, {timeout:30_000}).toBe(true);
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  await expect.poll(() => browser.contexts().flatMap((c) => c.pages()).length).toBe(2);
  await expect.poll(() => browser.contexts().flatMap((c) => c.pages()).some((p) => p.url().includes('view=widget'))).toBe(true);
  const pages = browser.contexts().flatMap((c) => c.pages());
  widget = pages.find((p) => p.url().includes('view=widget'))!;
  panel = pages.find((p) => !p.url().includes('view=widget'))!;
  for (const page of pages) {
    page.on('pageerror', (e) => consoleErrors.push(e.message));
    page.on('console', (e) => { if (e.type() === 'error') consoleErrors.push(e.text()); });
  }
  await expect(panel.getByRole('heading', { name: 'Tes compagnons.' })).toBeVisible();
}

async function stopApplication(): Promise<void> {
  if (child?.pid && child.exitCode === null) {
    // Only the process tree created by this fixture is terminated.
    try { execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' }); } catch { /* process may already have exited */ }
    await new Promise<void>((done) => { if (child.exitCode !== null) done(); else child.once('exit', () => done()); });
  }
  await browser?.close().catch(() => {});
}

test.beforeEach(async () => {
  consoleErrors = [];
  dataDirectory = resolve('artifacts/native', randomUUID());
  mkdirSync(dataDirectory, { recursive: true });
  await startApplication();
});

test.afterEach(async ({}, info) => {
  if (info.status !== info.expectedStatus && panel && !panel.isClosed()) {
    console.error('Console errors:', consoleErrors);
    console.error('UI feedback:', await panel.locator('.form-error,.toast,.connection-error').allTextContents().catch(() => []));
    await panel.screenshot({ path: info.outputPath('panel-failure.png') }).catch(() => {});
    await widget?.screenshot({ path: info.outputPath('widget-failure.png') }).catch(() => {});
  }
  await stopApplication();
});

test('profils réels, synchronisation du widget, mémoire isolée et redémarrage', async () => {
  await widget.mouse.move(-20, -20);
  await expect(widget.getByText('Microphone coupé')).toBeHidden();
  await expect(panel.getByRole('button', { name: 'Supprimer', exact: true })).toBeDisabled();
  await panel.screenshot({ path: 'artifacts/panel-compagnons.png', fullPage: true });
  await widget.screenshot({ path: 'artifacts/widget.png', omitBackground: true });

  await panel.getByRole('button', { name: 'Nouveau compagnon' }).click();
  await panel.getByLabel('Nom du compagnon').fill('Vega');
  await panel.getByLabel('Personnalité', { exact: true }).fill('Calme, curieuse et précise.');
  await panel.getByRole('button', { name: 'Créer le compagnon', exact: true }).click();
  await expect(panel.getByRole('dialog')).toHaveCount(0);
  await expect(panel.getByRole('heading', { name: 'Vega', exact: true })).toBeVisible();
  await panel.getByRole('button', { name: 'Activer ce compagnon' }).click();
  await expect(widget.locator('.widget-name')).toHaveText('Vega');
  await expect(widget.getByText('Microphone coupé')).toBeHidden();

  await panel.getByRole('button', { name: 'Modifier le compagnon' }).click();
  await panel.getByLabel('Nom du compagnon').fill('Vega II');
  await panel.getByRole('button', { name: 'Enregistrer', exact: true }).click();
  await expect(widget.locator('.widget-name')).toHaveText('Vega II');
  await panel.getByRole('button', { name: 'Dupliquer', exact: true }).click();
  await expect(panel.getByRole('heading', { name: 'Vega II · copie', exact: true })).toBeVisible();
  await expect(panel.locator('.profile-stats')).toContainText(/0\s*messages/);
  await panel.getByRole('button', { name: 'Supprimer', exact: true }).click();
  await expect(panel.getByRole('button', { name: 'Supprimer définitivement' })).toBeDisabled();
  await panel.getByLabel('Écris son nom pour confirmer').fill('Vega II · copie');
  await panel.getByRole('button', { name: 'Supprimer définitivement' }).click();
  await expect(panel.getByRole('dialog')).toHaveCount(0);
  await expect(panel.locator('.profile-card')).toHaveCount(2);

  await panel.getByRole('button', { name: 'Mémoire & histoire', exact: true }).click();
  await expect(panel.getByText('La première page reste à écrire.')).toBeVisible();
  await panel.getByRole('button', { name: 'Réglages', exact: true }).click();
  await panel.getByRole('checkbox', { name: 'Toujours au premier plan' }).uncheck();
  await panel.getByRole('button', { name: 'Enregistrer', exact: true }).click();
  await expect(panel.getByText('Préférences enregistrées.', { exact: true })).toBeVisible();
  await expect(panel.locator('.toast')).toHaveCount(1);
  await expect(panel.locator('.toast')).toHaveCount(0, { timeout: 6_000 });
  await panel.screenshot({ path: 'artifacts/panel-reglages.png', fullPage: true });

  await stopApplication();
  await startApplication();
  await expect(widget.locator('.widget-name')).toHaveText('Vega II');
  await expect(panel.locator('.profile-card')).toHaveCount(2);
  await panel.getByRole('button', { name: 'Réglages', exact: true }).click();
  await expect(panel.getByRole('checkbox', { name: 'Toujours au premier plan' })).not.toBeChecked();
  expect(consoleErrors).toEqual([]);
});

test('widget discret : survol, épinglage au clic, clavier et retour à l’image seule', async () => {
  const portrait = widget.locator('#drag-region');
  const controls = widget.locator('.widget-toolbar');
  await widget.mouse.move(-20, -20);
  await expect(controls).toBeHidden();
  await expect(widget.locator('.widget-name')).toBeHidden();
  await expect(widget.locator('.resize-handle')).toBeHidden();
  const imageBefore = await portrait.boundingBox();
  await portrait.hover();
  await expect(controls).toBeVisible();
  expect(await portrait.boundingBox()).toEqual(imageBefore);
  await widget.screenshot({ path: 'artifacts/widget-survol.png', omitBackground: true });
  await portrait.click();
  await widget.mouse.move(-20, -20);
  await expect(portrait).toHaveAttribute('aria-pressed', 'true');
  await expect(controls).toBeVisible();
  await portrait.click();
  await widget.mouse.move(-20, -20);
  await expect(controls).toBeHidden();
  await portrait.focus();
  await widget.keyboard.press('Space');
  await expect(controls).toBeVisible();
  await widget.keyboard.press('Escape');
  await expect(controls).toBeHidden();
  await widget.screenshot({ path: 'artifacts/widget-repos.png', omitBackground: true });
  expect(consoleErrors).toEqual([]);
});

test('catalogues HTTP, choix par profil, indisponibilité et conservation après redémarrage', async () => {
  const fixture = JSON.parse(readFileSync(resolve('tests/fixtures/services.json'), 'utf8')) as Record<string, unknown>;
  let delay = false;
  const server = createServer((request, response) => {
    const result = fixture[request.url ?? ''];
    const reply = () => { response.writeHead(result ? 200 : 404, { 'Content-Type': 'application/json' }); response.end(JSON.stringify(result ?? {})); };
    if (delay) setTimeout(reply, 900); else reply();
  });
  await new Promise<void>((done) => server.listen(0, '127.0.0.1', done));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Serveur de test indisponible');
  const url = `http://127.0.0.1:${address.port}`;
  try {
    const path = resolve(dataDirectory, 'connexion.sherka.json');
    writeFileSync(path, JSON.stringify({ format: 'sherka-profile', version: 1, profile: { name: 'Vega connectée', personality: '', instructions: '', accent: 'teal', connections: { lm_studio_url: url, voice_studio_url: url } }, avatar_png_base64: null }));
    const chooser = panel.waitForEvent('filechooser');
    await panel.getByRole('button', { name: 'Importer un profil Sherka' }).click();
    await (await chooser).setFiles(path);
    await expect(panel.getByRole('heading', { name: 'Vega connectée', exact: true })).toBeVisible();
    await panel.getByRole('button', { name: 'Réglages', exact: true }).click();
    await expect(panel.getByLabel('Modèle de conversation').locator('option')).toHaveCount(3);
    await expect(panel.getByLabel('Voix du compagnon').locator('option')).toHaveCount(4);
    await expect(panel.getByLabel('Moteur de synthèse vocale').locator('option[value="kitten"]')).toHaveJSProperty('disabled', true);
    await expect(panel.getByLabel('Moteur de synthèse vocale').locator('option[value="cosy"]')).toHaveJSProperty('disabled', true);
    await expect(panel.getByLabel('Modèle de conversation').locator('option[value="vectors"]')).toHaveCount(0);
    await panel.getByLabel('Modèle de conversation').selectOption('my-loaded-chat');
    await panel.getByLabel('Moteur de synthèse vocale').selectOption('omni');
    await panel.getByLabel('Voix du compagnon').selectOption('voice-b');
    await panel.getByLabel('Modèle de transcription').selectOption('faster-whisper');
    await panel.getByRole('button', { name: 'Enregistrer ces choix' }).click();
    await expect(panel.getByRole('button', { name: 'Enregistrer ces choix' })).toBeDisabled();
    await panel.getByLabel('Compagnon à configurer').selectOption({ label: 'Sherka' });
    await expect(panel.getByLabel('Modèle de conversation')).toHaveValue('');
    await panel.getByLabel('Compagnon à configurer').selectOption({ label: 'Vega connectée' });
    await expect(panel.getByLabel('Voix du compagnon')).toHaveValue('voice-b');

    await stopApplication();
    await startApplication();
    await panel.getByRole('button', { name: 'Réglages', exact: true }).click();
    await panel.getByLabel('Compagnon à configurer').selectOption({ label: 'Vega connectée' });
    await expect(panel.getByLabel('Modèle de conversation')).toHaveValue('my-loaded-chat');
    await expect(panel.getByLabel('Voix du compagnon')).toHaveValue('voice-b');
    await expect(panel.locator('[data-refresh="lm"]')).toBeEnabled();
    delay = true;
    await panel.locator('[data-refresh="lm"]').click();
    await panel.getByLabel('Adresse de LM Studio').fill(`${url}/absent`);
    await panel.locator('[data-refresh="lm"]').click();
    await expect(panel.locator('.connection-error')).toContainText('incompatible');
    await expect(panel.getByLabel('Modèle de conversation').locator('option')).toHaveCount(1);
    expect(consoleErrors).toEqual([]);
  } finally { server.closeAllConnections(); await new Promise<void>((done) => server.close(() => done())); }
});

test('démarrage avec le compagnon seul et ouverture du panneau à la demande', async () => {
  await stopApplication();
  await startApplication(false);
  const visibility = (): { widget: boolean; panel: boolean } => JSON.parse(execFileSync('powershell.exe', ['-NoProfile', '-File', resolve('tests/native/window-state.ps1'), '-TargetProcessId', String(child.pid)], { encoding: 'utf8', windowsHide: true }));
  await expect.poll(visibility).toEqual({ widget: true, panel: false });
  await widget.locator('#drag-region').hover();
  await widget.getByRole('button', { name: 'Ouvrir le poste de bord' }).click();
  await expect.poll(visibility).toEqual({ widget: true, panel: true });
});

test('connexion aux logiciels locaux installés', async () => {
  test.skip(process.env.SHERKA_LIVE_SERVICES !== '1', 'Essai explicite des logiciels locaux');
  await panel.getByRole('button', { name: 'Réglages', exact: true }).click();
  await expect(panel.locator('.connection-error')).toHaveCount(0);
  await expect(panel.getByLabel('Modèle de conversation').locator('option')).not.toHaveCount(1);
  await expect(panel.getByLabel('Voix du compagnon').locator('option')).not.toHaveCount(1);
  await expect(panel.locator('.connection-status').filter({ hasText: 'VoiceStudio' })).toBeVisible();
  await panel.screenshot({ path: 'artifacts/connexions-live.png', fullPage: true });
  expect(consoleErrors).toEqual([]);
});

test('imports validés, texte non exécutable et permissions distinctes des fenêtres', async () => {
  const fixturePath = resolve(dataDirectory, 'profil.sherka.json');
  writeFileSync(fixturePath, JSON.stringify({ format: 'sherka-profile', version: 1, profile: { name: '<img src=x onerror=alert(1)>', personality: 'Texte <script> et & caractères.', instructions: '', accent: 'teal' }, avatar_png_base64: null }));
  const chooser = panel.waitForEvent('filechooser');
  await panel.getByRole('button', { name: 'Importer un profil Sherka' }).click();
  await (await chooser).setFiles(fixturePath);
  await expect(panel.getByRole('heading', { name: '<img src=x onerror=alert(1)>', exact: true })).toBeVisible();
  await expect(panel.locator('img[src="x"]')).toHaveCount(0);

  const rejected = await widget.evaluate(async () => {
    const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args?: unknown) => Promise<unknown> } }).__TAURI_INTERNALS__;
    try { await internals.invoke('apply_mutation', { mutation: { kind: 'create_profile', draft: { name: 'Interdit', personality: '', instructions: '', accent: 'amber' } } }); return false; } catch { return true; }
  });
  expect(rejected).toBe(true);
  const deniedDiscovery = await widget.evaluate(async () => {
    const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args?: unknown) => Promise<unknown> } }).__TAURI_INTERNALS__;
    try { await internals.invoke('discover_language_models', { baseUrl: 'http://127.0.0.1:1234' }); return false; } catch { return true; }
  });
  expect(deniedDiscovery).toBe(true);
  await expect(panel.locator('.profile-card')).toHaveCount(2);
  const deniedRemote = await panel.evaluate(async () => {
    try { await fetch('https://example.com/'); return false; } catch { return true; }
  });
  expect(deniedRemote).toBe(true);
  // CSP rejection is intentionally exercised above; other tests enforce a
  // completely clean browser console during normal interaction.
});


test('conversation SSE réelle : texte progressif, arrêt, erreurs, profils et brouillon préservés', async () => {
  const models = modelRoutes(['chat-test']);
  const requests: { model: string; messages: { role: string; content: string }[] }[] = [];
  const timers: ReturnType<typeof setTimeout>[] = [];
  const lateReplies: (() => void)[] = [];
  const server = createServer((request, response) => {
    if (models(request,response)) return;
    let body = '';
    request.on('data', (bytes) => { body += bytes; });
    request.on('end', () => {
      requests.push(JSON.parse(body));
      const number = requests.length;
      if (number === 4) { response.writeHead(503); response.end('{}'); return; }
      response.writeHead(200, { 'Content-Type': 'text/event-stream' });
      // Literal newlines, fragmented UTF-8, and delayed completion cross IPC.
      const writeDelta = (text: string) => response.write('data: ' + JSON.stringify({ choices: [{ delta: { content: text } }] }) + '\n\n');
      const finish = () => { response.write('data: {"choices":[{"delta":{},"finish_reason":"stop"}]}\n\n'); response.end('data: [DONE]\n\n'); };
      if (number === 1) {
        writeDelta('Bonjour, ');
        timers.push(setTimeout(() => { writeDelta('été <script>texte sûr</script>.'); finish(); }, 700));
      } else if (number === 2 || number === 5) {
        writeDelta('Réponse à interrompre.');
        lateReplies.push(() => { writeDelta(' ANCIENNE FIN INTERDITE'); finish(); });
      } else { writeDelta('Nouvelle réponse.'); finish(); }
    });
  });
  await new Promise<void>((done) => server.listen(0, '127.0.0.1', done));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Serveur indisponible');
  try {
    const state = await native<any>('get_snapshot');
    const a = state.profiles[0];
    await native('apply_mutation', { mutation: { kind: 'connections', id: a.id, revision: a.revision, connections: { ...a.connections, lm_studio_url: `http://127.0.0.1:${address.port}`, llm_model: 'chat-test' } } });
    await panel.getByRole('button', { name: 'Conversation', exact: true }).click();
    await panel.getByLabel('Lire la réponse à voix haute').uncheck();
    await panel.getByLabel('Ou écris-lui ici').fill('Bonjour !');
    await panel.getByRole('button', { name: 'Envoyer' }).click();
    await expect(panel.locator('#live-reply')).toContainText('Bonjour,');
    await panel.getByLabel('Ou écris-lui ici').fill('Mon brouillon intact');
    await expect(panel.locator('#conversation-history')).toContainText('été <script>texte sûr</script>.');
    await expect(panel.getByLabel('Ou écris-lui ici')).toHaveValue('Mon brouillon intact');
    expect(requests[0].model).toBe('chat-test');
    expect(requests[0].messages[0].role).toBe('system');
    expect(requests[0].messages[0].content).toContain('Sherka');
    expect(requests[0].messages.at(-1)?.content).toBe('Bonjour !');
    await expect(panel.locator('#conversation-history script')).toHaveCount(0);

    await panel.getByLabel('Ou écris-lui ici').fill('Deuxième phrase');
    await panel.getByRole('button', { name: 'Envoyer' }).click();
    await expect(panel.locator('#live-reply')).toContainText('Réponse à interrompre.');
    await panel.getByRole('button', { name: 'Arrêter la réponse' }).click();
    await expect(panel.locator('#conversation-history')).toContainText('interrompue');
    lateReplies.shift()!();
    await panel.getByLabel('Ou écris-lui ici').fill('Nouvelle phrase');
    await panel.getByRole('button', { name: 'Envoyer' }).click();
    await expect(panel.locator('#conversation-history')).toContainText('Nouvelle réponse.');
    expect(requests[2].messages.some((m) => m.content.includes('Réponse à interrompre.'))).toBe(false);
    await panel.getByLabel('Ou écris-lui ici').fill('Erreur serveur');
    await panel.getByRole('button', { name: 'Envoyer' }).click();
    await expect(panel.locator('#conversation-error')).toContainText('HTTP 503');
    await expect(panel.locator('[data-session-status]')).toContainText('Microphone coupé');

    await panel.getByLabel('Ou écris-lui ici').fill('Changer de compagnon');
    await panel.getByRole('button', { name: 'Envoyer' }).click();
    await expect(panel.locator('#live-reply')).toContainText('Réponse à interrompre.');
    const changed = await native<any>('apply_mutation', { mutation: { kind: 'create_profile', draft: { name: 'Vega isolée', personality: 'Précise.', instructions: '', accent: 'teal' } } });
    const b = changed.profiles.find((p: any) => p.name === 'Vega isolée');
    await native('apply_mutation', { mutation: { kind: 'switch_profile', id: b.id } });
    lateReplies.shift()!();
    await expect(panel.getByRole('heading', { name: 'Avec Vega isolée.' })).toBeVisible();
    await expect(panel.locator('#live-reply')).toBeHidden();
    expect(await native('get_history', { profileId: b.id })).toEqual([]);
    const history = await native<any[]>('get_history', { profileId: a.id });
    expect(history.filter((m) => m.role === 'assistant')).toHaveLength(4);
    expect(history.some((m) => m.content.includes('ANCIENNE FIN INTERDITE'))).toBe(false);
    expect(history.at(-1).delivery).toBe('interrupted');
    expect(history.at(-1).heard_content).toBe('');
    await native('apply_mutation', { mutation: { kind: 'switch_profile', id: a.id } });
    await expect(panel.locator('#conversation-history')).toContainText('Nouvelle réponse.');
    await panel.screenshot({ path: 'artifacts/conversation.png', fullPage: true });
    await stopApplication(); await startApplication();
    expect((await native<any[]>('get_history', { profileId: a.id })).length).toBe(history.length);
    expect(consoleErrors).toEqual([]);
  } finally {
    timers.forEach(clearTimeout); server.closeAllConnections();
    await new Promise<void>((done) => server.close(() => done()));
  }
});

test('mémoire locale : ajout, recherche, correction, sources et isolation après redémarrage', async () => {
  const p=(await native<any>('get_snapshot')).profiles[0];
  await panel.getByRole('button',{name:'Mémoire & histoire',exact:true}).click();
  await panel.getByText('Ajouter un souvenir',{exact:true}).click();
  await panel.getByLabel('Contenu',{exact:true}).fill('Mon projet astronomie utilise une lunette. <script>texte</script>');
  await panel.getByLabel('Thème',{exact:true}).fill('Astronomie');
  await panel.getByLabel('Niveau').selectOption('detail');
  await panel.getByRole('button',{name:'Enregistrer le souvenir',exact:true}).click();
  await expect(panel.locator('.memory-entry')).toHaveCount(1);
  await expect(panel.locator('.memory-entry script')).toHaveCount(0);
  await panel.getByRole('button',{name:'Corriger',exact:true}).click();
  await panel.getByLabel('Contenu',{exact:true}).fill('Mon projet astronomie utilise un Newton.');
  await panel.getByRole('button',{name:'Enregistrer le souvenir',exact:true}).click();
  await expect(panel.locator('.memory-entry')).toContainText('Newton');
  await panel.getByLabel('Rechercher dans les souvenirs').fill('Newton');
  await expect(panel.locator('.memory-entry')).toHaveCount(1);
  await panel.screenshot({path:'artifacts/memoire.png',fullPage:true});
  const copy=await native<any>('apply_mutation',{mutation:{kind:'duplicate_profile',id:p.id}});
  const b=copy.profiles.find((x:any)=>x.id!==p.id);
  expect((await native<any>('get_memories',{profileId:b.id,query:''})).entries).toEqual([]);
  await stopApplication(); await startApplication();
  const memory=await native<any>('get_memories',{profileId:p.id,query:'Newton'});
  expect(memory.entries).toHaveLength(1);
  expect(memory.entries[0].content).toContain('Newton');
  await native('edit_memory',{profileId:p.id,epoch:memory.epoch,edit:{action:'forget',id:memory.entries[0].id}});
  expect((await native<any>('get_memories',{profileId:p.id,query:''})).entries).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test('modèles : ancien déchargé avant le nouveau, réutilisation et mémoire corrigée par le dialogue', async () => {
  const loaded=new Set(['alpha','unrelated']);
  let slowLoad=false;
  const operations:string[]=[];
  const prompts:any[]=[];
  const server=createServer((request,response)=>{
    const json=(value:unknown)=>{response.writeHead(200,{'Content-Type':'application/json'});response.end(JSON.stringify(value));};
    if(request.url==='/api/v1/models') {
      json({models:['alpha','beta','unrelated'].map(id=>({type:'llm',key:id,display_name:id,quantization:null,max_context_length:id==='beta'?262144:8192,loaded_instances:loaded.has(id)?[{id,config:{context_length:id==='beta'?200000:8192}}]:[]}))});return;
    }
    let body='';request.on('data',x=>{body+=x;});
    request.on('end',()=>{
      const value=JSON.parse(body);
      if(request.url==='/api/v1/models/load') {operations.push('load '+value.model);const done=()=>{loaded.add(value.model);json({});};if(slowLoad)setTimeout(done,400);else done();return;}
      if(request.url==='/api/v1/models/unload') {operations.push('unload '+value.instance_id);loaded.delete(value.instance_id);json({});return;}
      if(!value.stream) {
        const data=JSON.parse(value.messages[1].content);
        const old=data.souvenirs_disponibles[0];
        json({choices:[{finish_reason:'stop',message:{content:JSON.stringify({actions:[{action:old?'update':'create',id:old?.id??null,content:old?"Mon chat s'appelle Miko.":"Mon chat s'appelle Milo.",category:'Animaux',kind:'essential'}]})}}]});return;
      }
      prompts.push(value);
      response.writeHead(200,{'Content-Type':'text/event-stream'});
      response.write('data: '+JSON.stringify({choices:[{delta:{content:'D’accord, c’est enregistré.'}}]})+'\n\n');
      response.end('data: '+JSON.stringify({choices:[{delta:{},finish_reason:'stop'}],usage:{prompt_tokens:321,completion_tokens:12}})+'\n\ndata: [DONE]\n\n');
    });
  });
  await new Promise<void>(done=>server.listen(0,'127.0.0.1',done));
  const address=server.address();if(!address||typeof address==='string')throw new Error('Adresse indisponible');
  try {
    const p=(await native<any>('get_snapshot')).profiles[0];
    const config={...p.connections,lm_studio_url:'http://127.0.0.1:'+address.port,llm_model:'alpha'};
    await native('apply_mutation',{mutation:{kind:'connections',id:p.id,revision:p.revision,connections:config}});
    await native('send_message',{profileId:p.id,text:"Retiens que mon chat s'appelle Milo.",speak:false});
    await expect.poll(async()=>(await native<any>('get_memories',{profileId:p.id,query:''})).entries.length).toBe(1);
    await expect.poll(async()=>(await native<any>('get_conversation')).live.tag).toBeNull();
    await native('send_message',{profileId:p.id,text:"Corrige ta mémoire : mon chat s'appelle Miko.",speak:false});
    await expect.poll(async()=>(await native<any>('get_conversation')).live.tag).toBeNull();
    const memory=await native<any>('get_memories',{profileId:p.id,query:''});
    expect(memory.entries[0].content).toContain('Miko');
    expect(prompts.at(-1).messages.some((m:any)=>m.content.includes('Milo'))).toBe(false);
    const live=await native<any>('get_conversation');
    expect(live.live.budget.loaded_context).toBe(8192);
    expect(live.live.usage.prompt_tokens).toBe(321);
    const created=await native<any>('apply_mutation',{mutation:{kind:'create_profile',draft:{name:'Orion test',personality:'Curieux.',instructions:'',accent:'teal',connections:{...config,llm_model:'beta'}}}});
    const b=created.profiles.find((x:any)=>x.name==='Orion test');
    await native('apply_mutation',{mutation:{kind:'switch_profile',id:b.id}});
    await expect.poll(()=>operations.join(',')).toBe('unload alpha,load beta');
    expect([...loaded].sort()).toEqual(['beta','unrelated']);
    await native('send_message',{profileId:b.id,text:'Bonjour Orion.',speak:false});
    await expect.poll(async()=>(await native<any>('get_conversation')).live.tag).toBeNull();
    expect((await native<any>('get_conversation')).live.budget.limit).toBe(200000);
    expect(prompts.at(-1).messages.some((m:any)=>m.content.includes('Miko'))).toBe(false);
    await native('apply_mutation',{mutation:{kind:'switch_profile',id:b.id}});
    await native('send_message',{profileId:b.id,text:'Encore.',speak:false});
    await expect.poll(async()=>(await native<any>('get_conversation')).live.tag).toBeNull();
    expect(operations).toEqual(['unload alpha','load beta']);
    await native('apply_mutation',{mutation:{kind:'switch_profile',id:p.id}});
    await expect.poll(()=>operations.join(',')).toBe('unload alpha,load beta,unload beta,load alpha');
    expect([...loaded].sort()).toEqual(['alpha','unrelated']);
    // Return to alpha while beta's server-side loading is still in flight.
    // Reconciliation must await its completion, then unload it, not orphan it.
    slowLoad=true;
    await native('apply_mutation',{mutation:{kind:'switch_profile',id:b.id}});
    await expect.poll(()=>operations.filter(x=>x==='load beta').length).toBe(2);
    await native('apply_mutation',{mutation:{kind:'switch_profile',id:p.id}});
    await expect.poll(()=>[...loaded].sort().join(','),{timeout:5000}).toBe('alpha,unrelated');
    expect(operations.slice(-4)).toEqual(['unload alpha','load beta','unload beta','load alpha']);
    expect(consoleErrors).toEqual([]);
  } finally {server.closeAllConnections();await new Promise<void>(done=>server.close(()=>done()));}
});

test('LM Studio réel : mémoire autonome et aller-retour Dolphin / Gemma sans doublon VRAM', async () => {
  test.skip(process.env.SHERKA_LIVE_MEMORY !== '1','Activer seulement pour un essai local explicite.');
  test.setTimeout(300_000);
  const dolphin='llama-3-8b-dolphin-uncensored-i1';
  const gemma='gemma4-12b-qat-uncensored-hauhaucs-balanced';
  const catalog=async()=>await (await fetch('http://127.0.0.1:1234/api/v1/models')).json() as any;
  const count=async(key:string)=>(await catalog()).models.find((m:any)=>m.key===key)?.loaded_instances.length ?? 0;
  const p=(await native<any>('get_snapshot')).profiles[0];
  const config={...p.connections,llm_model:dolphin};
  await native('apply_mutation',{mutation:{kind:'connections',id:p.id,revision:p.revision,connections:config}});
  const say=async(text:string)=>{
    await native('send_message',{profileId:p.id,text,speak:false});
    await expect.poll(async()=>(await native<any>('get_conversation')).live.tag,{timeout:90_000,intervals:[300,700,1500]}).toBeNull();
    const result=await native<any>('get_conversation');
    expect(result.live.error).toBeNull();
    return result;
  };
  const created=await say('Retiens que mon mot favori pour nos tests est comète.');
  expect(created.live.memory_receipt).toContain('enregistré');
  expect((await native<any>('get_memories',{profileId:p.id,query:''})).entries).toHaveLength(1);
  const corrected=await say('Corrige le souvenir sur mon mot favori pour nos tests : il est maintenant nébuleuse, et non comète.');
  expect(corrected.live.memory_receipt).toContain('corrigé');
  const correctedMemory=await native<any>('get_memories',{profileId:p.id,query:''});
  expect(correctedMemory.entries).toHaveLength(1);
  expect(correctedMemory.entries[0].content.toLowerCase()).toContain('nébuleuse');
  const forgotten=await say('Oublie complètement le souvenir de mon mot favori pour nos tests.');
  expect(forgotten.live.memory_receipt).toContain('oublié');
  expect((await native<any>('get_memories',{profileId:p.id,query:''})).entries).toHaveLength(0);
  const changed=await native<any>('apply_mutation',{mutation:{kind:'create_profile',draft:{name:'Orion essai réel',personality:'Curieux et concis, avec un humour discret. Tu tutoies.',instructions:'Réponds en français.',accent:'teal',connections:{...config,llm_model:gemma,llm_model_key:gemma,reasoning:'off'}}}});
  const b=changed.profiles.find((x:any)=>x.name==='Orion essai réel');
  const bytes=readFileSync(resolve('public/assets/orion.png'));
  await native('apply_mutation',{mutation:{kind:'set_avatar',id:b.id,revision:b.revision,base64:bytes.toString('base64')}});
  await native('apply_mutation',{mutation:{kind:'switch_profile',id:b.id}});
  await expect.poll(()=>count(gemma),{timeout:120_000,intervals:[1000,2000]}).toBe(1);
  expect(await count(dolphin)).toBe(0);
  await native('send_message',{profileId:b.id,text:'En une phrase : pourquoi la Lune change-t-elle de forme apparente ?',speak:false});
  await expect.poll(async()=>(await native<any>('get_conversation')).live.tag,{timeout:90_000,intervals:[500,1500]}).toBeNull();
  const gemmaLive=await native<any>('get_conversation');
  expect(gemmaLive.live.error).toBeNull();
  expect(gemmaLive.live.reply.length).toBeGreaterThan(15);
  expect(gemmaLive.live.usage.reasoning_tokens).toBe(0);
  await panel.screenshot({path:'artifacts/orion-profile.png',fullPage:true});
  await widget.screenshot({path:'artifacts/orion-widget.png',omitBackground:true});
  const fresh=(await native<any>('get_snapshot')).profiles.find((x:any)=>x.id===b.id);
  await native('apply_mutation',{mutation:{kind:'connections',id:b.id,revision:fresh.revision,connections:{...fresh.connections,reasoning:'on'}}});
  await native('send_message',{profileId:b.id,text:'Combien font 17 fois 13 ? Donne seulement le résultat.',speak:false});
  await expect.poll(async()=>(await native<any>('get_conversation')).live.tag,{timeout:90_000,intervals:[500,1500]}).toBeNull();
  const thinkingLive=await native<any>('get_conversation');
  expect(thinkingLive.live.error).toBeNull();
  expect(thinkingLive.live.usage.reasoning_tokens).toBeGreaterThan(0);
  expect(thinkingLive.live.reply).toContain('221');
  await native('apply_mutation',{mutation:{kind:'switch_profile',id:p.id}});
  await expect.poll(()=>count(dolphin),{timeout:120_000,intervals:[1000,2000]}).toBe(1);
  expect(await count(gemma)).toBe(0);
  writeFileSync(resolve('artifacts/memory-live.json'),JSON.stringify({testedAt:new Date().toISOString(),dolphin:created.live.budget,gemma:gemmaLive.live.budget,create:true,correct:true,forget:true,switchBack:true,reasoningOff:gemmaLive.live.usage.reasoning_tokens,reasoningOn:thinkingLive.live.usage.reasoning_tokens},null,2));
});

test('texte indépendant du TTS, synthèse anticipée et annulation de toute la file audio', async () => {
  const models = modelRoutes(['stream-test']);
  const spoken: { text: string; at: number }[] = [];
  const poem = 'Les chatons endormis rêvent dans le jardin, la lune les éclaire et veille sur leurs jeux.\n'.repeat(20);
  const timers: ReturnType<typeof setTimeout>[] = [];
  let failVoice = false;
  const server = createServer((request, response) => {
    if (models(request,response)) return;
    let body = '';
    request.on('data', (bytes) => { body += bytes; });
    request.on('end', () => {
      if (request.url === '/v1/audio/speech') {
        spoken.push({ text: JSON.parse(body).input, at: Date.now() });
        timers.push(setTimeout(() => {
          response.writeHead(failVoice ? 503 : 200, { 'Content-Type': 'audio/pcm' });
          response.end(failVoice ? '{}' : Buffer.alloc(24_000 * 2 * 4));
        }, 80));
      } else {
        response.writeHead(200, { 'Content-Type': 'text/event-stream' });
        for (const word of poem.split(' ')) response.write('data: ' + JSON.stringify({ choices: [{ delta: { content: word + ' ' } }] }) + '\n\n');
        response.end('data: [DONE]\n\n');
      }
    });
  });
  await new Promise<void>((done) => server.listen(0, '127.0.0.1', done));
  const address = server.address(); if (!address || typeof address === 'string') throw new Error('Adresse manquante');
  try {
    const s = await native<any>('get_snapshot'); const p = s.profiles[0];
    const url = `http://127.0.0.1:${address.port}`;
    await native('apply_mutation', { mutation: { kind: 'connections', id: p.id, revision: p.revision, connections: { ...p.connections, lm_studio_url: url, voice_studio_url: url, llm_model: 'stream-test', tts_engine: 'omni', tts_voice: 'test', volume: 0 } } });
    await panel.getByRole('button', { name: 'Conversation', exact: true }).click();
    await panel.getByLabel('Ou écris-lui ici').fill('Un poème sur les chatons');
    await panel.getByRole('button', { name: 'Envoyer' }).click();
    await expect.poll(() => spoken.length, { timeout: 2500 }).toBeGreaterThanOrEqual(2);
    expect(spoken[1].at - spoken[0].at).toBeLessThan(2000); // first playback lasts 4 seconds
    await expect.poll(async () => (await native<any>('get_conversation')).live.reply.trim(), { timeout: 1500 }).toBe(poem.trim());
    expect((await native<any[]>('get_history', { profileId: p.id })).filter((m) => m.role === 'assistant')).toHaveLength(0);
    await panel.getByRole('button', { name: 'Arrêter la réponse' }).click();
    await expect.poll(async () => (await native<any[]>('get_history', { profileId: p.id })).at(-1)?.delivery).toBe('interrupted');
    expect((await native<any[]>('get_history', { profileId: p.id })).at(-1).heard_content).toBe('');
    expect((await native<any>('get_snapshot')).session.microphone).toBe('off');

    // A failed synthesizer must not stall or truncate the written answer.
    failVoice = true;
    await panel.getByLabel('Ou écris-lui ici').fill('Encore un poème');
    await panel.getByRole('button', { name: 'Envoyer' }).click();
    await expect(panel.locator('#conversation-error')).toContainText('HTTP 503');
    await expect.poll(async () => (await native<any[]>('get_history', { profileId: p.id })).filter((m) => m.role === 'assistant').length).toBe(2);
    expect((await native<any[]>('get_history', { profileId: p.id })).at(-1).content.trim()).toBe(poem.trim());
    expect(consoleErrors).toEqual([]);
  } finally {
    timers.forEach(clearTimeout); server.closeAllConnections();
    await new Promise<void>((done) => server.close(() => done()));
  }
});

test('audio natif : périphériques visibles, consentement et microphone manquant', async () => {
  const fixture = JSON.parse(readFileSync(resolve('tests/fixtures/services.json'), 'utf8'));
  const server = createServer((request, response) => { const body = fixture[request.url ?? '']; response.writeHead(body ? 200 : 404, { 'Content-Type': 'application/json' }); response.end(JSON.stringify(body ?? {})); });
  await new Promise<void>((done) => server.listen(0, '127.0.0.1', done));
  const address = server.address(); if (!address || typeof address === 'string') throw new Error('Adresse manquante');
  try {
    const s = await native<any>('get_snapshot'); const p = s.profiles[0];
    expect(s.session.microphone).toBe('off');
    await native('apply_mutation', { mutation: { kind: 'connections', id: p.id, revision: p.revision, connections: { ...p.connections, lm_studio_url: `http://127.0.0.1:${address.port}`, voice_studio_url: `http://127.0.0.1:${address.port}`, llm_model: 'my-loaded-chat', stt_model: 'faster-whisper', tts_engine: 'omni', tts_voice: 'voice-b' } } });
    await native('apply_mutation', { mutation: { kind: 'audio_settings', audio: { ...s.settings.audio, input_device: 'missing-input-device' } } });
    await panel.getByRole('button', { name: 'Réglages', exact: true }).click();
    await expect(panel.getByRole('combobox', { name: 'Microphone', exact: true })).toHaveValue('missing-input-device', { timeout: 30_000 });
    await expect(panel.getByLabel('Sortie sonore').locator('option')).not.toHaveCount(0);
    await panel.getByRole('button', { name: 'Conversation', exact: true }).click();
    await panel.getByRole('button', { name: 'Activer le micro', exact: true }).click();
    await expect(panel.locator('.toast.error')).toContainText('Microphone introuvable');
    expect((await native<any>('get_snapshot')).session.microphone).toBe('off');
    expect(consoleErrors).toEqual([]);
  } finally { server.closeAllConnections(); await new Promise<void>((done) => server.close(() => done())); }
});

test('conversation locale complète : LM Studio, OmniVoice et sortie WASAPI sans capture', async () => {
  test.skip(process.env.SHERKA_LIVE_CONVERSATION !== '1' || !process.env.SHERKA_TEST_VOICE_ID, 'Inférence explicite sur les services locaux');
  test.setTimeout(150_000);
  const s = await native<any>('get_snapshot'); const p = s.profiles[0];
  await native('apply_mutation', { mutation: { kind: 'connections', id: p.id, revision: p.revision, connections: { ...p.connections, llm_model: 'llama-3-8b-dolphin-uncensored-i1', tts_engine: 'omnivoice', tts_voice: process.env.SHERKA_TEST_VOICE_ID, stt_model: 'sherpa-parakeet-tdt-v3', volume: 0 } } });
  await panel.getByRole('button', { name: 'Conversation', exact: true }).click();
  await panel.getByLabel('Ou écris-lui ici').fill('Dis seulement : Bonjour, je suis Sherka, et la connexion fonctionne.');
  await panel.getByRole('button', { name: 'Envoyer' }).click();
  await expect.poll(async () => (await native<any[]>('get_history', { profileId: p.id })).filter((m) => m.role === 'assistant').length, { timeout: 80_000 }).toBe(1);
  const history = await native<any[]>('get_history', { profileId: p.id });
  // Zero-volume playback exercises WASAPI acknowledgements without claiming
  // that a muted phrase was heard by the user.
  expect(history.at(-1).delivery).toBe('text_only');
  expect(history.at(-1).heard_content).toBe('');
  expect((await native<any>('get_snapshot')).session.microphone).toBe('off');
  await expect(panel.locator('#conversation-error')).toBeHidden();
  await panel.getByLabel('Ou écris-lui ici').fill('Quelle est la distance moyenne entre la Terre et la Lune ? Réponds en une phrase.');
  await panel.getByRole('button', { name: 'Envoyer' }).click();
  await expect.poll(async () => (await native<any[]>('get_history', { profileId: p.id })).filter((m) => m.role === 'assistant').length, { timeout: 60_000 }).toBe(2);
  await expect(panel.locator('#conversation-error')).toBeHidden();
  await panel.getByLabel('Ou écris-lui ici').fill('Écris un petit poème de huit vers sur les chatons, sans introduction.');
  await panel.getByRole('button', { name: 'Envoyer' }).click();
  await expect.poll(async () => (await native<any[]>('get_history', { profileId: p.id })).filter((m) => m.role === 'assistant').length, { timeout: 80_000 }).toBe(3);
  const spokenTurns = (await native<any[]>('get_history', { profileId: p.id })).filter((m) => m.role === 'assistant');
  expect(spokenTurns.every((m) => m.delivery === 'text_only' && m.heard_content === '')).toBe(true);
  expect(spokenTurns.at(-1).content.length).toBeGreaterThan(180);
  await expect(panel.locator('#conversation-error')).toBeHidden();
  await panel.screenshot({ path: 'artifacts/conversation-live.png', fullPage: true });
  expect(consoleErrors).toEqual([]);
});


test('pensée : capacités, intensité, sauvegarde par profil et flux final sans réflexion parlée', async () => {
  const routes=modelRoutes(['binary','graded','plain'],8192,{binary:['off','on'],graded:['off','low','medium','high']});
  const requests:any[]=[];
  const server=createServer((request,response)=>{
    if(routes(request,response))return;
    let body='';request.on('data',bytes=>{body+=bytes;});
    request.on('end',()=>{
      requests.push(JSON.parse(body));
      response.writeHead(200,{'Content-Type':'text/event-stream'});
      response.write('data: '+JSON.stringify({choices:[{delta:{reasoning_content:'PENSEE INTERNE NON LUE'}}]})+'\n\n');
      response.write('data: '+JSON.stringify({choices:[{delta:{content:'221'},finish_reason:'stop'}],usage:{prompt_tokens:30,completion_tokens:9,completion_tokens_details:{reasoning_tokens:6}}})+'\n\n');
      response.end('data: [DONE]\n\n');
    });
  });
  await new Promise<void>(done=>server.listen(0,'127.0.0.1',done));
  const address=server.address();if(!address||typeof address==='string')throw new Error('Serveur indisponible');
  try {
    const p=(await native<any>('get_snapshot')).profiles[0];
    await native('apply_mutation',{mutation:{kind:'connections',id:p.id,revision:p.revision,connections:{...p.connections,lm_studio_url:'http://127.0.0.1:'+address.port,llm_model:'binary'}}});
    await panel.getByRole('button',{name:'Réglages',exact:true}).click();
    await expect(panel.getByLabel('Pensée')).toBeVisible();
    const choice=panel.getByLabel('Pensée');
    expect(await choice.locator('option').allTextContents()).toEqual(['Réglage de LM Studio · Activée','Désactivée','Activée']);
    const save=async()=>{await panel.getByRole('button',{name:'Enregistrer ces choix',exact:true}).click();await expect(panel.getByRole('button',{name:'Enregistrer ces choix',exact:true})).toBeDisabled();};
    const say=async()=>{await native('send_message',{profileId:p.id,text:'Combien font 17 fois 13 ?',speak:false});await expect.poll(async()=>(await native<any>('get_conversation')).live.tag).toBeNull();return (await native<any>('get_conversation')).live;};
    await choice.selectOption('off');await save();
    expect((await say()).reply).toBe('221');expect(requests.at(-1).reasoning_effort).toBe('none');
    await choice.selectOption('on');await save();
    expect((await say()).usage.reasoning_tokens).toBe(6);expect(requests.at(-1).reasoning_effort).toBe('medium');
    await panel.getByLabel('Modèle de conversation').selectOption('graded');
    await choice.selectOption('high');await save();
    await say();expect(requests.at(-1).reasoning_effort).toBe('high');
    await stopApplication();await startApplication();
    expect((await native<any>('get_snapshot')).profiles[0].connections.reasoning).toBe('high');
    await panel.getByRole('button',{name:'Réglages',exact:true}).click();
    await expect(panel.getByLabel('Pensée')).toHaveValue('high');
    await panel.screenshot({path:'artifacts/pensee.png',fullPage:true});
    await panel.getByLabel('Modèle de conversation').selectOption('plain');await save();
    await expect(panel.getByLabel('Pensée')).toHaveCount(0);
    await say();expect(requests.at(-1).reasoning_effort).toBeUndefined();
    expect(consoleErrors).toEqual([]);
  } finally {server.closeAllConnections();await new Promise<void>(done=>server.close(()=>done()));}
});

test('widget : afficher / masquer synchronisé, brouillon conservé et choix direct du compagnon', async () => {
  const visibility = (): { widget: boolean; panel: boolean } => JSON.parse(execFileSync('powershell.exe', ['-NoProfile', '-File', resolve('tests/native/window-state.ps1'), '-TargetProcessId', String(child.pid)], { encoding: 'utf8', windowsHide: true }));
  await panel.getByRole('button', { name: 'Afficher le widget', exact: true }).click();
  await expect.poll(() => visibility().widget).toBe(true);
  await expect(panel.getByRole('button', { name: 'Masquer le widget', exact: true })).toHaveAttribute('aria-pressed','true');
  await panel.getByRole('button', { name: 'Conversation', exact: true }).click();
  await panel.getByLabel('Ou écris-lui ici').fill('Mon brouillon à conserver');
  await panel.getByRole('button', { name: 'Masquer le widget', exact: true }).click();
  await expect.poll(() => visibility().widget).toBe(false);
  await native('widget_action', { action: 'restore_widget' }); // same path as tray / second launch
  await expect(panel.getByRole('button', { name: 'Masquer le widget', exact: true })).toBeVisible();
  await widget.locator('#drag-region').hover();
  await widget.getByRole('button', { name: 'Masquer le compagnon', exact: true }).click();
  await expect(panel.getByRole('button', { name: 'Afficher le widget', exact: true })).toBeVisible();
  await expect(panel.getByLabel('Ou écris-lui ici')).toHaveValue('Mon brouillon à conserver');
  const a = (await native<any>('get_snapshot')).profiles[0];
  const result = await native<any>('apply_mutation',{mutation:{kind:'create_profile',draft:{name:'Orion widget',personality:'Curieux.',instructions:'',accent:'teal',connections:a.connections}}});
  const b = result.profiles.find((p:any)=>p.name==='Orion widget');
  await native('widget_action',{action:'show_widget'});
  await widget.locator('#drag-region').hover();
  await widget.getByRole('button',{name:'Changer de compagnon',exact:true}).click();
  await expect(widget.getByRole('group',{name:'Choisir un compagnon'})).toBeVisible();
  await widget.screenshot({path:'artifacts/widget-companions.png',omitBackground:true});
  await widget.getByRole('button',{name:'Orion widget',exact:true}).click();
  await expect(widget.locator('.widget-name')).toHaveText('Orion widget');
  const selected = await native<any>('get_snapshot');
  expect(selected.session.profile_id).toBe(b.id);
  expect(selected.session.microphone).toBe('off');
  expect(selected.profiles.find((p:any)=>p.id===b.id).message_count).toBe(0);
  const rejected = await widget.evaluate(async id => {
    try { await (window as any).__TAURI_INTERNALS__.invoke('switch_companion',{id}); return false; } catch { return true; }
  },randomUUID());
  expect(rejected).toBe(true);
  expect((await native<any>('get_snapshot')).session.profile_id).toBe(b.id);
  await widget.locator('#drag-region').hover();
  await widget.getByRole('button',{name:'Changer de compagnon',exact:true}).click();
  await widget.keyboard.press('Escape');
  await expect(widget.locator('#widget-companions')).toBeHidden();
  await widget.keyboard.press('Escape');
  await widget.mouse.move(-20,-20);
  await expect(widget.locator('.widget-toolbar')).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test('voix propres aux compagnons : sélection, requêtes TTS, bascule et redémarrage', async () => {
  const fixture = JSON.parse(readFileSync(resolve('tests/fixtures/services.json'),'utf8'));
  fixture['/v1/audio/voices'].voices = [
    {voice_id:'voice-a',name:'Sherka',type:'profile',language:'French'},
    {voice_id:'voice-b',name:'French Narrator',type:'profile',language:'French'}
  ];
  const models=modelRoutes(['voice-chat']);
  const spoken:any[]=[];
  let late: (()=>void)|undefined;
  const server=createServer((request,response)=>{
    if(models(request,response))return;
    if(request.method==='GET') { const data=fixture[request.url??''];response.writeHead(data?200:404,{'Content-Type':'application/json'});response.end(JSON.stringify(data??{}));return; }
    let body='';request.on('data',x=>{body+=x;});request.on('end',()=>{
      const data=JSON.parse(body);
      if(request.url==='/v1/audio/speech') {
        spoken.push(data);
        const done=()=>{response.writeHead(200,{'Content-Type':'audio/pcm'});response.end(Buffer.alloc(24_000*2/10));};
        if(data.voice==='voice-a' && spoken.length===1)late=done;else done();
      } else {
        response.writeHead(200,{'Content-Type':'text/event-stream'});
        response.end('data: '+JSON.stringify({choices:[{delta:{content:data.messages.at(-1).content+'.'}}]})+'\n\ndata: [DONE]\n\n');
      }
    });
  });
  await new Promise<void>(done=>server.listen(0,'127.0.0.1',done));
  const address=server.address();if(!address||typeof address==='string')throw new Error('Adresse indisponible');
  const url='http://127.0.0.1:'+address.port;
  try {
    const a=(await native<any>('get_snapshot')).profiles[0];
    const connections={...a.connections,lm_studio_url:url,voice_studio_url:url,llm_model:'voice-chat',tts_engine:'omni',tts_voice:'voice-a',volume:1};
    await native('apply_mutation',{mutation:{kind:'connections',id:a.id,revision:a.revision,connections}});
    const snapshot=await native<any>('apply_mutation',{mutation:{kind:'create_profile',draft:{name:'Orion voix',personality:'Curieux',instructions:'',accent:'teal',connections}}});
    const b=snapshot.profiles.find((p:any)=>p.name==='Orion voix');
    await panel.locator(`button[data-profile="${b.id}"]`).click();
    await panel.getByRole('button',{name:'Choisir sa voix'}).click();
    await expect(panel.getByRole('combobox',{name:/Voix de Orion voix/})).toHaveValue('voice-a');
    await panel.getByRole('combobox',{name:/Voix de Orion voix/}).selectOption('voice-b');
    await panel.screenshot({path:'artifacts/companion-voice.png',fullPage:true});
    await panel.getByRole('button',{name:'Associer cette voix'}).click();
    await expect(panel.locator('dialog')).toHaveCount(0);
    await expect(panel.locator('#profile-voice-label')).toHaveText('French Narrator');
    expect((await native<any>('get_snapshot')).profiles.find((p:any)=>p.id===a.id).connections.tts_voice).toBe('voice-a');
    await native('send_message',{profileId:a.id,text:'Bonjour Sherka',speak:true});
    await expect.poll(()=>spoken.length).toBe(1);
    await widget.locator('#drag-region').hover();
    await widget.getByRole('button',{name:'Changer de compagnon',exact:true}).click();
    await widget.getByRole('button',{name:'Orion voix',exact:true}).click();
    late!();
    await native('send_message',{profileId:b.id,text:'Bonjour Orion',speak:true});
    await expect.poll(()=>spoken.length).toBe(2);
    await expect.poll(async()=>(await native<any>('get_conversation')).live.tag).toBeNull();
    expect(spoken.map(p=>p.voice)).toEqual(['voice-a','voice-b']);
    const history=await native<any[]>('get_history',{profileId:b.id});
    expect(history.at(-1).heard_content).toBe('Bonjour Orion.');
    expect(history.some(m=>m.content.includes('Bonjour Sherka'))).toBe(false);
    expect((await native<any[]>('get_history',{profileId:a.id})).at(-1).delivery).toBe('interrupted');
    await stopApplication();await startApplication();
    const saved=await native<any>('get_snapshot');
    expect(saved.settings.active_profile).toBe(b.id);
    expect(saved.profiles.find((p:any)=>p.id===a.id).connections.tts_voice).toBe('voice-a');
    expect(saved.profiles.find((p:any)=>p.id===b.id).connections.tts_voice).toBe('voice-b');
    await native('send_message',{profileId:b.id,text:'Après redémarrage',speak:true});
    await expect.poll(()=>spoken.length).toBe(3);
    expect(spoken[2].voice).toBe('voice-b');
    expect(consoleErrors).toEqual([]);
  } finally { server.closeAllConnections();await new Promise<void>(done=>server.close(()=>done())); }
});

test('quitter pendant une bascule attend le chargement puis libère le modèle sans toucher aux autres', async () => {
  const loaded=new Set(['alpha','unrelated']);
  const operations:string[]=[];
  let finishLoad:(()=>void)|undefined;
  const server=createServer((request,response)=>{
    const json=(value:unknown)=>{response.writeHead(200,{'Content-Type':'application/json'});response.end(JSON.stringify(value));};
    if(request.url==='/api/v1/models') {json({models:['alpha','beta','unrelated'].map(id=>({type:'llm',key:id,display_name:id,max_context_length:8192,loaded_instances:loaded.has(id)?[{id,config:{context_length:8192}}]:[]}))});return;}
    let body='';request.on('data',x=>{body+=x;});request.on('end',()=>{
      const value=JSON.parse(body);
      if(request.url==='/api/v1/models/load') {operations.push('load '+value.model);finishLoad=()=>{loaded.add(value.model);json({});};}
      else if(request.url==='/api/v1/models/unload') {operations.push('unload '+value.instance_id);loaded.delete(value.instance_id);json({});}
      else {response.writeHead(404);response.end('{}');}
    });
  });
  await new Promise<void>(done=>server.listen(0,'127.0.0.1',done));
  const address=server.address();if(!address||typeof address==='string')throw new Error('Adresse indisponible');
  try {
    const a=(await native<any>('get_snapshot')).profiles[0];
    const connections={...a.connections,lm_studio_url:'http://127.0.0.1:'+address.port,llm_model:'alpha'};
    await native('apply_mutation',{mutation:{kind:'connections',id:a.id,revision:a.revision,connections}});
    await expect.poll(async()=>(await native<any>('get_conversation')).live.notice).toContain('alpha chargé');
    const created=await native<any>('apply_mutation',{mutation:{kind:'create_profile',draft:{name:'Orion sortie',personality:'',instructions:'',accent:'teal',connections:{...connections,llm_model:'beta'}}}});
    const b=created.profiles.find((p:any)=>p.name==='Orion sortie');
    await widget.evaluate(async id=>(window as any).__TAURI_INTERNALS__.invoke('switch_companion',{id}),b.id);
    await expect.poll(()=>operations.join(',')).toBe('unload alpha,load beta');
    await native('widget_action',{action:'quit'});
    await expect(panel.locator('#quit-status')).toContainText('déchargement');
    expect(child.exitCode).toBeNull();
    expect(await native('apply_mutation',{mutation:{kind:'switch_profile',id:a.id}}).catch(String)).toContain('en train de décharger');
    finishLoad!();
    await expect.poll(()=>child.exitCode).toBe(0);
    expect(operations).toEqual(['unload alpha','load beta','unload beta']);
    expect([...loaded]).toEqual(['unrelated']);
    expect(consoleErrors).toEqual([]);
  } finally {server.closeAllConnections();await new Promise<void>(done=>server.close(()=>done()));}
});

test('sortie après redémarrage : échec de déchargement signalé, puis nouvelle tentative', async () => {
  const loaded=new Set(['alpha','unrelated']);
  let reject=true;
  const unloads:string[]=[];
  const server=createServer((request,response)=>{
    if(request.url==='/api/v1/models') {response.writeHead(200,{'Content-Type':'application/json'});response.end(JSON.stringify({models:[...loaded].map(id=>({type:'llm',key:id,display_name:id,max_context_length:8192,loaded_instances:[{id,config:{context_length:8192}}]}))}));return;}
    let body='';request.on('data',x=>{body+=x;});request.on('end',()=>{
      if(request.url==='/api/v1/models/unload') {const id=JSON.parse(body).instance_id;unloads.push(id);if(!reject)loaded.delete(id);response.writeHead(reject?503:200,{'Content-Type':'application/json'});response.end('{}');}
      else {response.writeHead(404);response.end('{}');}
    });
  });
  await new Promise<void>(done=>server.listen(0,'127.0.0.1',done));
  const address=server.address();if(!address||typeof address==='string')throw new Error('Adresse indisponible');
  try {
    const a=(await native<any>('get_snapshot')).profiles[0];
    await native('apply_mutation',{mutation:{kind:'connections',id:a.id,revision:a.revision,connections:{...a.connections,lm_studio_url:'http://127.0.0.1:'+address.port,llm_model:'alpha'}}});
    await expect.poll(async()=>(await native<any>('get_conversation')).live.notice).toContain('alpha chargé');
    await stopApplication();await startApplication(); // ownership recovered from active preset
    await native('widget_action',{action:'quit'});
    await expect(panel.getByRole('heading',{name:'Le modèle n’a pas pu être déchargé'})).toBeVisible();
    expect(child.exitCode).toBeNull();
    expect([...loaded]).toEqual(['alpha','unrelated']);
    reject=false;
    await panel.getByRole('button',{name:'Réessayer',exact:true}).click();
    await expect.poll(()=>child.exitCode).toBe(0);
    expect(unloads).toEqual(['alpha','alpha']);
    expect([...loaded]).toEqual(['unrelated']);
    expect(consoleErrors).toEqual([]);
  } finally {server.closeAllConnections();await new Promise<void>(done=>server.close(()=>done()));}
});

test('quitter préserve une autre instance du même modèle et décharge seulement son alias', async () => {
  const loaded=new Set(['sherka-alias']);
  const unloads:string[]=[];
  const server=createServer((request,response)=>{
    if(request.url==='/api/v1/models') {response.writeHead(200,{'Content-Type':'application/json'});response.end(JSON.stringify({models:[{type:'llm',key:'shared-model',display_name:'Shared',max_context_length:8192,loaded_instances:[...loaded].map(id=>({id,config:{context_length:8192}}))}]}));return;}
    let body='';request.on('data',x=>{body+=x;});request.on('end',()=>{
      if(request.url==='/api/v1/models/unload') {const id=JSON.parse(body).instance_id;unloads.push(id);loaded.delete(id);response.writeHead(200,{'Content-Type':'application/json'});response.end('{}');}
      else {response.writeHead(404);response.end('{}');}
    });
  });
  await new Promise<void>(done=>server.listen(0,'127.0.0.1',done));
  const address=server.address();if(!address||typeof address==='string')throw new Error('Adresse indisponible');
  try {
    const a=(await native<any>('get_snapshot')).profiles[0];
    await native('apply_mutation',{mutation:{kind:'connections',id:a.id,revision:a.revision,connections:{...a.connections,lm_studio_url:'http://127.0.0.1:'+address.port,llm_model:'shared-model',llm_model_key:'shared-model'}}});
    await expect.poll(async()=>(await native<any>('get_conversation')).live.notice).toContain('Shared chargé');
    loaded.add('another-app-alias');
    await native('widget_action',{action:'quit'});
    await expect.poll(()=>child.exitCode).toBe(0);
    expect(unloads).toEqual(['sherka-alias']);
    expect([...loaded]).toEqual(['another-app-alias']);
    expect(consoleErrors).toEqual([]);
  } finally {server.closeAllConnections();await new Promise<void>(done=>server.close(()=>done()));}
});

test('fermeture réelle : libération du modèle actif dans LM Studio', async () => {
  test.skip(process.env.SHERKA_LIVE_SHUTDOWN !== '1','Déchargement réel explicitement demandé.');
  const model=process.env.SHERKA_LIVE_SHUTDOWN_MODEL;
  if(!model)throw new Error('Précise le modèle à libérer pour cet essai réel.');
  const catalog=async()=>await (await fetch('http://127.0.0.1:1234/api/v1/models')).json() as any;
  const loaded=(data:any)=>data.models.flatMap((m:any)=>(m.loaded_instances??[]).map((i:any)=>i.id)).sort();
  const before=await catalog();
  const target=before.models.find((m:any)=>m.key===model || m.loaded_instances?.some((i:any)=>i.id===model));
  expect(target).toBeDefined();
  const previous=loaded(before);
  const a=(await native<any>('get_snapshot')).profiles[0];
  await native('apply_mutation',{mutation:{kind:'connections',id:a.id,revision:a.revision,connections:{...a.connections,llm_model:model,llm_model_key:target.key}}});
  await expect.poll(async()=>(await native<any>('get_conversation')).live.notice,{timeout:120_000}).toContain('chargé');
  const active=await catalog();
  const own=active.models.find((m:any)=>m.key===target.key).loaded_instances.map((i:any)=>i.id);
  expect(own.length).toBe(1);
  await panel.getByRole('button',{name:'Réglages',exact:true}).click();
  await panel.getByRole('button',{name:'Quitter Sherka',exact:true}).click();
  await expect.poll(()=>child.exitCode,{timeout:30_000}).toBe(0);
  const after=loaded(await catalog());
  expect(after).toEqual(previous.filter((id:string)=>!own.includes(id)));
  writeFileSync('artifacts/shutdown-live-0.3.1.json',JSON.stringify({testedAt:new Date().toISOString(),model:target.key,before:previous,unloaded:own,after,exitCode:child.exitCode},null,2));
  expect(consoleErrors).toEqual([]);
});

test('VoiceStudio : attente au lancement, erreur, nouvelle tentative sans perdre le brouillon', async () => {
  let mode = 503;
  const requests: string[] = [];
  const server = createServer((request, response) => {
    requests.push(request.url!);
    response.writeHead(mode, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify(request.url === '/health' ? { status: 'ok', version: '0.5.6', device: 'cpu' } : { protocol: 'voicestudio.speech.v1' }));
  });
  await new Promise<void>(done => server.listen(0, '127.0.0.1', done));
  const address = server.address(); if (!address || typeof address === 'string') throw new Error('Port indisponible');
  try {
    const p = (await native<any>('get_snapshot')).profiles[0];
    const url = `http://127.0.0.1:${address.port}`;
    await native('apply_mutation', { mutation: { kind: 'connections', id: p.id, revision: p.revision, connections: { ...p.connections, voice_studio_url: url, lm_studio_url: url } } });
    await stopApplication(); requests.length = 0;
    await startApplication(true, true);
    await expect(panel.locator('#voice-service-status')).toContainText('VoiceStudio se prépare');
    await panel.getByRole('button', { name: 'Conversation', exact: true }).click();
    await panel.locator('#chat-text').fill('Mon brouillon reste ici.');
    expect((await native<any>('get_snapshot')).session.microphone).toBe('off');
    mode = 401;
    await expect(panel.locator('#voice-service-status')).toContainText('401');
    await expect(panel.locator('#chat-text')).toHaveValue('Mon brouillon reste ici.');
    await panel.screenshot({ path: 'artifacts/voice-startup-error.png' });
    mode = 200;
    await panel.getByRole('button', { name: 'Réessayer VoiceStudio' }).click();
    await expect.poll(async () => (await native<any>('get_voice_service')).phase).toBe('ready');
    await expect(panel.locator('#voice-service-status')).toHaveCount(0);
    await expect(panel.locator('#chat-text')).toHaveValue('Mon brouillon reste ici.');
    expect(requests.every(path => ['/health', '/.well-known/voicestudio-speech'].includes(path))).toBe(true);
    expect((await native<any>('get_snapshot')).session.microphone).toBe('off');
    await widget.locator('#drag-region').hover();
    await expect(widget.locator('[data-session-status]')).toHaveText('Microphone coupé');
    expect(await widget.evaluate(() => (window as any).__TAURI_INTERNALS__.invoke('retry_voice_service').catch(String))).toContain('not allowed');
    expect(consoleErrors).toEqual([]);
  } finally { server.closeAllConnections(); await new Promise<void>(done => server.close(() => done())); }
});

test('VoiceStudio : réglages et fermeture disponibles pendant une demande vocale en attente', async () => {
  const routes = modelRoutes(['startup-test']);
  let healthRequests = 0;
  const server = createServer((request, response) => {
    if (routes(request, response)) return;
    if (request.url === '/health') healthRequests++;
    response.writeHead(503); response.end('{}');
  });
  await new Promise<void>(done => server.listen(0, '127.0.0.1', done));
  const address = server.address(); if (!address || typeof address === 'string') throw new Error('Port indisponible');
  try {
    const p = (await native<any>('get_snapshot')).profiles[0];
    const url = `http://127.0.0.1:${address.port}`;
    await native('apply_mutation', { mutation: { kind: 'connections', id: p.id, revision: p.revision, connections: { ...p.connections, voice_studio_url: url, lm_studio_url: url, llm_model: 'startup-test', stt_model: 'parakeet', tts_engine: 'omni', tts_voice: 'test' } } });
    await expect.poll(async () => (await native<any>('get_conversation')).live.notice).toContain('chargé');
    await stopApplication(); await startApplication(true, true);
    await expect.poll(() => healthRequests).toBeGreaterThan(0);
    await panel.evaluate(() => { (window as any).pendingMicrophone = (window as any).__TAURI_INTERNALS__.invoke('widget_action', { action: 'toggle_microphone' }).catch(String); });
    expect((await native<any>('get_snapshot')).session.microphone).toBe('off');
    const active = (await native<any>('get_snapshot')).profiles[0];
    const editStarted = Date.now();
    await native('apply_mutation', { mutation: { kind: 'connections', id: active.id, revision: active.revision, connections: { ...active.connections, voice_studio_url: url + '/second' } } });
    expect(Date.now() - editStarted).toBeLessThan(4_000);
    expect(await panel.evaluate(() => (window as any).pendingMicrophone)).toContain('annulée');
    expect((await native<any>('get_snapshot')).session.microphone).toBe('off');
    await panel.evaluate(id => { (window as any).pendingSpeech = (window as any).__TAURI_INTERNALS__.invoke('send_message', { profileId: id, text: 'Message en attente', speak: true }).catch(String); }, active.id);
    expect(await native<any[]>('get_history', { profileId: active.id })).toEqual([]);
    await native('widget_action', { action: 'pause_voice' });
    expect(await panel.evaluate(() => (window as any).pendingSpeech)).toContain('annulée');
    expect(await native<any[]>('get_history', { profileId: active.id })).toEqual([]);
    await panel.evaluate(() => { (window as any).pendingMicrophone = (window as any).__TAURI_INTERNALS__.invoke('widget_action', { action: 'toggle_microphone' }).catch(String); });
    const started = Date.now();
    await native('widget_action', { action: 'quit' });
    await expect.poll(() => child.exitCode, { timeout: 8_000 }).toBe(0);
    expect(Date.now() - started).toBeLessThan(8_000);
    expect(consoleErrors).toEqual([]);
  } finally { server.closeAllConnections(); await new Promise<void>(done => server.close(() => done())); }
});

test('VoiceStudio réel : démarrage automatique puis réutilisation du même processus', async () => {
  test.skip(process.env.SHERKA_LIVE_VOICE_STARTUP !== '1', 'Démarrage réel de VoiceStudio explicitement demandé.');
  test.setTimeout(360_000);
  const processes = () => JSON.parse(execFileSync('powershell.exe', ['-NoProfile', '-Command', "@{voice=@(Get-Process VoiceStudio -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id);lm=@(Get-Process 'LM Studio' -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)} | ConvertTo-Json -Compress"], { windowsHide: true, encoding: 'utf8' }));
  const before = processes();
  await stopApplication();
  const started = Date.now();
  await startApplication(false, true);
  await expect.poll(async () => (await native<any>('get_voice_service')).phase, { timeout: 310_000, intervals: [500, 1500, 3000] }).toBe('ready');
  const after = processes();
  expect(after.voice.length).toBeGreaterThan(0);
  expect(after.lm.sort()).toEqual(before.lm.sort());
  if (before.voice.length) expect(after.voice.sort()).toEqual(before.voice.sort());
  const health = await (await fetch('http://127.0.0.1:3900/health')).json();
  const capabilities = await (await fetch('http://127.0.0.1:3900/.well-known/voicestudio-speech')).json() as any;
  expect(capabilities.protocol).toBe('voicestudio.speech.v1');
  const voiceWindows = JSON.parse(execFileSync('powershell.exe', ['-NoProfile', '-File', 'tests/native/voice-window-state.ps1'], { windowsHide: true, encoding: 'utf8' }));
  if (!before.voice.length) {
    expect(voiceWindows.length).toBeGreaterThan(0);
    expect(voiceWindows.every((window: any) => window.minimized)).toBe(true);
  }
  const readyMs = Date.now() - started;
  expect((await native<any>('get_snapshot')).session.microphone).toBe('off');
  const windows = JSON.parse(execFileSync('powershell.exe', ['-NoProfile', '-File', 'tests/native/window-state.ps1', '-TargetProcessId', String(child.pid)], { windowsHide: true, encoding: 'utf8' }));
  expect(windows).toEqual({ widget: true, panel: false });
  await native('retry_voice_service');
  await expect.poll(async () => (await native<any>('get_voice_service')).phase).toBe('ready');
  expect(processes().voice.sort()).toEqual(after.voice.sort());
  await native('widget_action', { action: 'quit' });
  await expect.poll(() => child.exitCode).toBe(0);
  expect((await fetch('http://127.0.0.1:3900/health')).ok).toBe(true);
  await stopApplication(); await startApplication(false, true);
  await expect.poll(async () => (await native<any>('get_voice_service')).phase).toBe('ready');
  expect(processes().voice.sort()).toEqual(after.voice.sort());
  await native('widget_action', { action: 'quit' });
  await expect.poll(() => child.exitCode).toBe(0);
  writeFileSync('artifacts/voice-startup-live-0.3.2.json', JSON.stringify({ testedAt: new Date().toISOString(), executable, coldStart: before.voice.length === 0, readyMs, elapsedMs: Date.now() - started, before, after, health, windows, voiceWindows, microphone: 'off', reusedAfterRetryAndRestart: true, voiceSurvivesSherka: true }, null, 2));
  expect(consoleErrors).toEqual([]);
});

test('updates: manual check, retry, widget isolation and invalid signature rejection', async () => {
  let mode = 'current'; let requests = 0; let downloads = 0;
  const server = createServer((request, response) => {
    if (request.url === '/latest.json') {
      ++requests;
      if (mode === 'error') { response.writeHead(503); response.end(); return; }
      const address = server.address() as { port: number };
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify({ version: mode === 'current' ? '0.3.3' : '0.3.4', notes: '<script>untrusted notes</script>', platforms: {
        'windows-x86_64-portable': { url: `http://127.0.0.1:${address.port}/ArthurReboulSalze/Sherka-Compagnon/releases/download/v${mode === 'current' ? '0.3.3' : '0.3.4'}/sherka-compagnon.exe`, signature: 'invalid' }
      }}));
    } else { ++downloads; response.writeHead(200); response.end('not a signed executable'); }
  });
  await new Promise<void>(done => server.listen(0, '127.0.0.1', done));
  process.env.SHERKA_TEST_UPDATE_URL = `http://127.0.0.1:${(server.address() as {port:number}).port}/latest.json`;
  try {
    await stopApplication(); await startApplication();
    await panel.getByRole('button', { name: 'Réglages', exact: true }).click();
    await expect(panel.locator('#update-version')).toContainText('Sans installation');
    expect(requests).toBe(0);
    const denied = await widget.evaluate(async () => {
      try { await (window as any).__TAURI_INTERNALS__.invoke('check_update'); return false; } catch { return true; }
    });
    expect(denied).toBe(true);
    await panel.locator('#check-update').click();
    await expect(panel.locator('#update-status')).toContainText('dernière version');
    mode = 'error'; await panel.locator('#check-update').click();
    await expect(panel.locator('#update-status')).toContainText('Impossible de vérifier');
    mode = 'available'; await panel.locator('#check-update').click();
    await expect(panel.locator('#update-status')).toContainText('0.3.4 est disponible');
    expect(await panel.locator('#update-notes script').count()).toBe(0);
    await panel.locator('#install-update').click();
    await expect(panel.locator('#update-status')).toContainText('signature est invalide');
    expect(downloads).toBe(1);
    expect(child.exitCode).toBeNull();
    expect((await native<any>('get_snapshot')).session.microphone).toBe('off');
    expect(consoleErrors).toEqual([]);
  } finally { delete process.env.SHERKA_TEST_UPDATE_URL; server.closeAllConnections(); await new Promise<void>(done => server.close(() => done())); }
});

test('updates: signed portable replacement, graceful model unload and preserved profile', async () => {
  test.skip(!process.env.SHERKA_TEST_UPDATE_BINARY || !process.env.SHERKA_TEST_UPDATE_SIGNATURE, 'Explicit signed debug fixture required');
  test.setTimeout(180_000);
  const original = executable;
  const target = resolve(dataDirectory, 'portable/sherka-compagnon.exe');
  mkdirSync(resolve(dataDirectory, 'portable'), {recursive:true}); copyFileSync(original, target);
  const bytes = readFileSync(process.env.SHERKA_TEST_UPDATE_BINARY!);
  const signature = readFileSync(process.env.SHERKA_TEST_UPDATE_SIGNATURE!, 'utf8').trim();
  const routes = modelRoutes(['update-model']); let unloads = 0; let updateVersion = '0.3.4';
  const server = createServer((request,response) => {
    if (request.url === '/latest.json') {
      const address = server.address() as {port:number};
      response.writeHead(200, {'Content-Type':'application/json'});
      response.end(JSON.stringify({version:updateVersion,platforms:{'windows-x86_64-portable':{url:`http://127.0.0.1:${address.port}/ArthurReboulSalze/Sherka-Compagnon/releases/download/v${updateVersion}/sherka-compagnon.exe`,signature}}}));
    } else if (request.url?.endsWith('/sherka-compagnon.exe')) { response.writeHead(200, {'Content-Length':bytes.length}); response.end(bytes); }
    else { if(request.url?.endsWith('/unload'))++unloads; if(!routes(request,response)){ response.writeHead(404); response.end(); } }
  });
  await new Promise<void>(done => server.listen(0,'127.0.0.1',done));
  const address = `http://127.0.0.1:${(server.address() as {port:number}).port}`;
  process.env.SHERKA_TEST_UPDATE_URL = address+'/latest.json';
  process.env.SHERKA_TEST_ALLOW_CURRENT_UPDATE = '1';
  let restartedPid: number | undefined;
  try {
    await stopApplication(); executable=target; await startApplication(false);
    const s=await native<any>('get_snapshot'); const p=s.profiles[0];
    await native('apply_mutation',{mutation:{kind:'connections',id:p.id,revision:p.revision,connections:{...p.connections,lm_studio_url:address,llm_model:'update-model'}}});
    await expect.poll(async()=> (await native<any>('get_conversation')).live.notice).toContain('update-model chargé');
    await panel.getByRole('button',{name:'Réglages',exact:true}).click();
    // A real signed binary must also match the version claimed by the manifest.
    await panel.locator('#check-update').click(); await expect(panel.locator('#install-update')).toBeVisible();
    await panel.locator('#install-update').click(); await expect(panel.locator('#update-status')).toContainText('signature est invalide');
    expect(child.exitCode).toBeNull(); expect(unloads).toBe(0);
    updateVersion = '0.3.3';
    await panel.locator('#check-update').click(); await expect(panel.locator('#install-update')).toBeVisible();
    await panel.locator('#install-update').click();
    await expect.poll(()=>child.exitCode,{timeout:60_000}).toBe(0);
    await expect.poll(()=>readdirSync(resolve(dataDirectory,'updates')).some(id=>existsSync(resolve(dataDirectory,'updates',id,'result.json'))),{timeout:30_000}).toBe(true);
    const id=readdirSync(resolve(dataDirectory,'updates')).find(id=>existsSync(resolve(dataDirectory,'updates',id,'result.json')))!;
    restartedPid=JSON.parse(readFileSync(resolve(dataDirectory,'updates',id,'result.json'),'utf8')).restarted_pid;
    expect(readFileSync(target).equals(bytes)).toBe(true); expect(unloads).toBe(1);
    await expect.poll(()=>JSON.parse(execFileSync('powershell.exe',['-NoProfile','-File','tests/native/window-state.ps1','-TargetProcessId',String(restartedPid)],{windowsHide:true,encoding:'utf8'})).widget,{timeout:20_000}).toBe(true);
    await browser.close().catch(()=>{});
    await expect.poll(async()=>{try{return (await fetch(`http://127.0.0.1:${debugPort}/json/version`)).ok}catch{return false}},{timeout:30_000}).toBe(true);
    browser=await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`);
    await expect.poll(()=>browser.contexts().flatMap(c=>c.pages()).length).toBe(2);
    const pages=browser.contexts().flatMap(c=>c.pages());
    panel=pages.find(p=>!p.url().includes('view=widget'))!; widget=pages.find(p=>p.url().includes('view=widget'))!;
    const after=await native<any>('get_snapshot');
    expect(after.profiles[0].id).toBe(p.id); expect(after.profiles[0].connections.llm_model).toBe('update-model');
    expect(after.session.microphone).toBe('off');
    writeFileSync('artifacts/update-portable-validation.json',JSON.stringify({verifiedSignature:true,rejectedWrongSignedVersion:true,replaced:true,restarted:true,modelUnloaded:true,dataDirectory,profileId:p.id},null,2));
    expect(consoleErrors).toEqual([]);
  } finally {
    if(restartedPid) {try{execFileSync('taskkill',['/PID',String(restartedPid),'/T','/F'],{windowsHide:true,stdio:'ignore'});}catch{}}
    delete process.env.SHERKA_TEST_UPDATE_URL; delete process.env.SHERKA_TEST_ALLOW_CURRENT_UPDATE;
    executable=original; server.closeAllConnections(); await new Promise<void>(done=>server.close(()=>done()));
  }
});

test('public screenshot: clean companion without personal data', async () => {
  test.skip(process.env.SHERKA_PUBLIC_SCREENSHOT !== '1', 'Explicit documentation capture');
  await native('widget_action',{action:'show_panel'});
  mkdirSync('docs/images',{recursive:true});
  await panel.screenshot({path:'docs/images/sherka.png'});
  const history = await native<any[]>('get_history',{profileId:(await native<any>('get_snapshot')).profiles[0].id});
  expect(history).toHaveLength(0);
  expect(consoleErrors).toEqual([]);
});

test('published update manifest: release binary checks the official GitHub version', async () => {
  test.skip(process.env.SHERKA_RELEASE_UPDATE_CHECK !== '1', 'Check only after publishing the release');
  await panel.getByRole('button',{name:'Réglages',exact:true}).click();
  await expect(panel.locator('#update-version')).toContainText('0.3.3');
  await panel.locator('#check-update').click();
  await expect(panel.locator('#update-status')).toContainText('dernière version',{timeout:30_000});
  expect((await native<any>('get_update_status')).phase).toBe('current');
  expect(consoleErrors).toEqual([]);
});
