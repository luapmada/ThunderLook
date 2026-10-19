// Vérification d'interface des jalons, sur l'application réelle et ses données.
// Lance ThunderLook avec le port de débogage de WebView2 (CDP), pilote la page de l'intérieur (aucune entrée
// système, rien ne peut atteindre une autre application), contrôle, puis remet les réglages et ferme l'application.
// Usage : node tools/ui_check.mjs [chemin\thunderlook.exe]      (code de sortie 1 si une vérification échoue)
// Chaque jalon ajoute ses vérifications dans CHECKS.
import { spawn } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const exe = process.argv[2] ?? path.join(root, 'target', 'release', 'thunderlook.exe');
const port = 9333;

// --- Client CDP minimal ---------------------------------------------------------------------------
async function connect() {
  for (let i = 0; i < 60; i++) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = targets.find((t) => t.type === 'page' && !t.url.startsWith('devtools'));
      if (page) {
        const ws = new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((ok, ko) => { ws.onopen = ok; ws.onerror = ko; });
        let id = 0;
        const pending = new Map();
        ws.onmessage = (m) => {
          const msg = JSON.parse(m.data);
          if (pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
        };
        const send = (method, params = {}) =>
          new Promise((ok) => { const i = ++id; pending.set(i, ok); ws.send(JSON.stringify({ id: i, method, params })); });
        return { ws, send };
      }
    } catch { /* pas encore prêt */ }
    await sleep(500);
  }
  throw new Error(`pas de page CDP sur le port ${port}`);
}

let cdp;
async function js(expression) {
  const r = await cdp.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(`JS : ${r.result.exceptionDetails.exception?.description ?? expression}`);
  return r.result?.result?.value;
}
const KEYS = { ArrowRight: 39, ArrowLeft: 37, ArrowDown: 40, ArrowUp: 38, Enter: 13, Escape: 27, Home: 36, End: 35 };
async function key(name) {
  for (const type of ['rawKeyDown', 'keyUp']) {
    await cdp.send('Input.dispatchKeyEvent', { type, key: name, code: name, windowsVirtualKeyCode: KEYS[name] ?? 0 });
  }
}
/** Attend que `expression` soit vraie (5 s au plus). */
async function until(expression, what) {
  for (let i = 0; i < 50; i++) {
    if (await js(expression)) return;
    await sleep(100);
  }
  throw new Error(`délai dépassé : ${what}`);
}

// --- Aides propres à ThunderLook ------------------------------------------------------------------
const ROW = (pos) => `document.querySelector('.msg[aria-posinset="${pos}"]')`;
const settingsOn = () => js(`document.querySelector('[aria-label="Afficher en tant que conversations"]')?.getAttribute('aria-pressed') === 'true'`);
async function clickTab(label) {
  await js(`[...document.querySelectorAll('[role=tab]')].find(t => t.textContent.trim() === ${JSON.stringify(label)}).click()`);
  await sleep(200);
}
async function toggleConversations(on) {
  await clickTab('Affichage');
  if ((await settingsOn()) !== on) {
    await js(`document.querySelector('[aria-label="Afficher en tant que conversations"]').click()`);
    await until(`document.querySelector('[aria-label="Afficher en tant que conversations"]').getAttribute('aria-pressed') === '${on}'`, 'bascule des conversations');
  }
  await sleep(800);
}
/** Coche ou décoche « Marquer l'élément comme lu quand la sélection change » ; renvoie l'état d'avant. */
async function setMarkRead(on) {
  await clickTab('Affichage');
  await js(`document.querySelector('[aria-label="Réglages de l\\'affichage"]').click()`);
  await until(`document.querySelector('.dialog')`, 'boîte Réglages de l\'affichage');
  const box = `[...document.querySelectorAll('.dialog label.check')].find(l => l.textContent.includes('comme lu')).querySelector('input')`;
  const before = await js(`${box}.checked`);
  if (before !== on) await js(`${box}.click()`);
  await js(`[...document.querySelectorAll('.dialog button')].find(b => b.textContent.trim() === 'OK').click()`);
  await until(`!document.querySelector('.dialog')`, 'fermeture de la boîte');
  return before;
}

/** Ouvre chaque dossier de courrier jusqu'à trouver un en-tête de conversation rendu à l'écran. */
async function findConversation() {
  const count = await js(`document.querySelectorAll('.treeitem').length`);
  for (let i = 0; i < count; i++) {
    await js(`document.querySelectorAll('.treeitem')[${i}].click()`);
    await sleep(700);
    const found = await js(`(() => {
      const h = [...document.querySelectorAll('.msg.conv')].find(e => e.getAttribute('aria-expanded') === 'false');
      return h ? { pos: +h.getAttribute('aria-posinset'), label: h.getAttribute('aria-label'),
                   folder: document.querySelectorAll('.treeitem')[${i}].textContent.trim() } : null;
    })()`);
    if (found) return found;
  }
  throw new Error('aucune conversation de plusieurs éléments dans les premières lignes des dossiers');
}

// --- Vérifications --------------------------------------------------------------------------------
const CHECKS = [
  ['J3b : conversations (chevron, ← →, volet de lecture)', async (log) => {
    await toggleConversations(true);
    const h = await findConversation();
    const n = +h.label.match(/conversation de (\d+) éléments/)[1];
    log(`« ${h.label.slice(0, 70)}… » (${h.folder}, ligne ${h.pos}, ${n} éléments)`);
    const kids = `document.querySelectorAll('.msg.child').length`;
    const expanded = (v) => `${ROW(h.pos)}?.getAttribute('aria-expanded') === '${v}'`;

    await js(`${ROW(h.pos)}.querySelector('.conv-toggle').click()`);
    await until(`${expanded(true)} && ${kids} > 0`, 'dépliage au chevron');
    const shown = await js(`[...document.querySelectorAll('.msg.child')].map(e => +e.getAttribute('aria-posinset')).sort((a, b) => a - b)`);
    if (shown[0] !== h.pos + 1) throw new Error(`premier élément déplié en ligne ${shown[0]}, attendu ${h.pos + 1}`);
    log(`chevron : déplié, ${shown.length} éléments rendus sous l'en-tête`);

    const subject = await js(`document.querySelector('.reading-subject')?.textContent ?? ''`);
    const first = await js(`${ROW(h.pos + 1)}.getAttribute('aria-label')`);
    log(`volet de lecture : « ${subject} » ; premier élément : « ${first.slice(0, 60)} »`);

    await js(`document.querySelector('[role=listbox]').focus()`);
    await key('ArrowLeft');
    await until(`${expanded(false)} && ${kids} === 0`, '← sur l\'en-tête');
    log('← sur l\'en-tête : replié');
    await key('ArrowRight');
    await until(`${expanded(true)} && ${kids} > 0`, '→ sur l\'en-tête');
    log('→ sur l\'en-tête : déplié');
    await key('ArrowDown');
    await until(`document.querySelector('.msg.focused')?.classList.contains('child')`, '↓ vers un élément déplié');
    await key('ArrowLeft');
    await until(`${expanded(false)} && document.querySelector('.msg.focused')?.getAttribute('aria-posinset') === '${h.pos}'`, '← sur un élément déplié');
    log('← sur un élément déplié : replié, sélection sur l\'en-tête');
  }],
];

// --- Exécution ------------------------------------------------------------------------------------
const app = spawn(exe, [], {
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
  stdio: 'ignore',
});
let failed = 0;
let initialConversations;
let initialMarkRead;
try {
  cdp = await connect();
  await until(`document.querySelectorAll('.treeitem').length > 0`, 'arborescence chargée');
  await clickTab('Affichage');
  initialConversations = await settingsOn();
  // Les vérifications déplacent la sélection : sans cela, des messages réels seraient marqués lus.
  initialMarkRead = await setMarkRead(false);
  for (const [name, run] of CHECKS) {
    const lines = [];
    try {
      await run((l) => lines.push(l));
      console.log(`OK    ${name}`);
    } catch (e) {
      failed++;
      console.log(`ÉCHEC ${name} : ${e.message}`);
    }
    for (const l of lines) console.log(`      ${l}`);
  }
} catch (e) {
  failed++;
  console.log(`ÉCHEC démarrage : ${e.message}`);
} finally {
  try {
    if (initialConversations !== undefined) await toggleConversations(initialConversations);
    if (initialMarkRead !== undefined) await setMarkRead(initialMarkRead);
    await clickTab('Accueil');
  } catch { /* fermeture quand même */ }
  cdp?.ws.close();
  app.kill();
}
console.log(failed ? `${failed} vérification(s) en échec` : 'toutes les vérifications passent');
process.exit(failed ? 1 : 0);
