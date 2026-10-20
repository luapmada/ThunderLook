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

CHECKS.push(['J5 : répondre à tous, écrire, enregistrer en brouillon, supprimer le brouillon', async (log) => {
  await toggleConversations(false);
  const openFolder = async (pattern) => {
    const ok = await js(`(() => { const t = [...document.querySelectorAll('.treeitem')].find(e => ${pattern}.test(e.textContent)); if (!t) return false; t.click(); return true; })()`);
    if (!ok) throw new Error(`dossier introuvable : ${pattern}`);
    await sleep(900);
  };
  const draftsCount = `(() => { const t = [...document.querySelectorAll('.treeitem')].find(e => /^Brouillons/.test(e.textContent.trim())); return t ? t.textContent.trim() : ''; })()`;
  const before = await js(draftsCount);
  await openFolder('/Boîte de réception/');
  await js(`${ROW(1)}.click()`);
  await until(`document.querySelector('.reading-subject')`, 'message affiché');
  const original = await js(`document.querySelector('.reading-subject').textContent`);
  await js(`[...document.querySelectorAll('.reading-actions button')].find(b => b.textContent.includes('Répondre à tous')).click()`);
  await until(`document.querySelector('.pane.compose iframe')?.contentDocument?.designMode === 'on'`, 'volet de rédaction');
  const fields = await js(`[...document.querySelectorAll('.compose-field input')].map(i => i.value)`);
  const [to, , subject] = fields;
  if (!to) throw new Error('« À » est vide');
  if (!subject.startsWith('RE: ')) throw new Error(`objet inattendu : ${subject}`);
  const quoted = await js(`(() => { const b = document.querySelector('.pane.compose iframe').contentDocument.body.innerHTML; return ['De&nbsp;:', 'Envoyé&nbsp;:', 'Objet&nbsp;:'].every(x => b.includes(x)); })()`);
  if (!quoted) throw new Error('bloc cité absent du corps');
  log(`réponse à tous de « ${original.slice(0, 50)} » : À = ${to.slice(0, 60)}, objet = « ${subject.slice(0, 50)} », bloc cité présent`);
  // Texte saisi dans l'éditeur (designMode), puis enregistrement.
  await js(`(() => { const d = document.querySelector('.pane.compose iframe').contentDocument; d.body.focus(); d.execCommand('insertText', false, 'Vérification automatique ThunderLook. '); })()`);
  await js(`[...document.querySelectorAll('.compose-bar button')].find(b => b.textContent.includes('Enregistrer')).click()`);
  await until(`(document.querySelector('[role=status], .statusbar')?.textContent ?? '').includes('Brouillon enregistré')`, 'enregistrement du brouillon');
  await sleep(800);
  log(`brouillon enregistré ; Brouillons : « ${before} » → « ${await js(draftsCount)} »`);
  await js(`[...document.querySelectorAll('.compose-bar button')].find(b => b.textContent.includes('Ignorer')).click()`);
  // Le brouillon est le plus récent de « Brouillons » : on le vérifie, puis on le supprime définitivement.
  await openFolder('/^Brouillons/');
  const label = await js(`${ROW(1)}?.getAttribute('aria-label') ?? ''`);
  if (!label.includes(subject.slice(0, 30))) throw new Error(`brouillon introuvable en tête de « Brouillons » : ${label}`);
  await js(`${ROW(1)}.click(); document.querySelector('[role=listbox]').focus()`);
  // Entrée rouvre le brouillon avec le texte saisi.
  await key('Enter');
  await until(`document.querySelector('.pane.compose iframe')?.contentDocument?.body?.innerHTML?.includes('Vérification automatique ThunderLook')`, 'réouverture du brouillon');
  log('Entrée sur le brouillon : rouvert avec le texte saisi');
  await js(`[...document.querySelectorAll('.compose-bar button')].find(b => b.textContent.includes('Ignorer')).click()`);
  await js(`document.querySelector('[role=listbox]').focus()`);
  await cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'Delete', code: 'Delete', windowsVirtualKeyCode: 46, modifiers: 8 });
  await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Delete', code: 'Delete', windowsVirtualKeyCode: 46, modifiers: 8 });
  await until(`document.querySelector('.dialog')`, 'confirmation de suppression définitive');
  await js(`[...document.querySelectorAll('.dialog button')].find(b => b.textContent.trim() === 'OK').click()`);
  await sleep(1200);
  const after = await js(draftsCount);
  if (after !== before) throw new Error(`Brouillons : « ${before} » avant, « ${after} » après la suppression`);
  log(`brouillon supprimé définitivement ; Brouillons revenu à « ${after} »`);
}]);

CHECKS.push(['J6 : Règles et alertes (liste, description, case active, aperçu d\'Exécuter maintenant)', async (log) => {
  const openRules = async () => {
    await clickTab('Accueil');
    await js(`document.querySelector('button[aria-label="Règles"]').click()`);
    await until(`[...document.querySelectorAll('[role=menuitem]')].some(b => b.textContent.includes('Gérer les règles'))`, 'menu Règles');
    await js(`[...document.querySelectorAll('[role=menuitem]')].find(b => b.textContent.includes('Gérer les règles')).click()`);
    await until(`document.querySelectorAll('.dialog.wide .rules-item').length > 0`, 'liste des règles');
  };
  const closeRules = async () => {
    await js(`document.querySelector('.dialog.wide').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`);
    await until(`!document.querySelector('.dialog.wide')`, 'fermeture de la boîte des règles');
  };
  await openRules();
  const origins = await js(`[...document.querySelectorAll('.rules-origin')].map(e => e.textContent.trim())`);
  const n = await js(`document.querySelectorAll('.rules-item').length`);
  log(`${n} règles, jeux : ${origins.join(' ; ')}`);
  // Règle du flux du PST : sa description reprend les libellés d'Outlook.
  const idx = await js(`[...document.querySelectorAll('.rules-item')].findIndex(e => e.textContent.includes('contact@electromagnetisme.fr (1)'))`);
  if (idx < 0) throw new Error('règle « contact@electromagnetisme.fr (1) » absente');
  await js(`document.querySelectorAll('.rules-item')[${idx}].click()`);
  await until(`document.querySelector('.rules-desc')?.textContent.includes('envoyé à contact@electromagnetisme.fr')`, 'description de la règle');
  log(`description : ${await js(`[...document.querySelectorAll('.rules-desc p')].map(p => p.textContent).join(' / ')`)}`);
  // Case « active » : décochée, gardée après réouverture, puis recochée (la base revient à l'état initial).
  const box = (i) => `document.querySelectorAll('.rules-item')[${i}].querySelector('input')`;
  const before = await js(`${box(idx)}.checked`);
  await js(`${box(idx)}.click()`);
  await sleep(400);
  await closeRules();
  await openRules();
  const kept = await js(`${box(idx)}.checked`);
  await js(`${box(idx)}.click()`);
  await sleep(400);
  if (kept === before) throw new Error('la case « active » n\'a pas été gardée');
  if ((await js(`${box(idx)}.checked`)) !== before) throw new Error('la case « active » n\'est pas revenue à son état');
  log(`case active : ${before} → ${kept} après réouverture → ${before}`);
  // Exécuter maintenant : aperçu sur la Boîte de réception, sans rien modifier.
  await js(`[...document.querySelectorAll('.dialog.wide button')].find(b => b.textContent.includes('Exécuter les règles')).click()`);
  await until(`document.querySelector('.dialog.wide h2').textContent.includes('Exécuter les règles maintenant')`, 'mode Exécuter');
  await js(`(() => { const s = document.querySelector('.dialog.wide select'); const o = [...s.options].find(o => o.textContent.trim().startsWith('Boîte de réception')); s.value = o.value; s.dispatchEvent(new Event('change', { bubbles: true })); })()`);
  // Seules les règles du PST : les jeux .rwz visent d'autres fichiers de données.
  await js(`(() => { let on = false; for (const e of document.querySelector('.rules-list').children) {
    if (e.classList.contains('rules-origin')) { on = !e.title.includes('.rwz'); continue; }
    const c = e.querySelector('input'); if (c.checked !== (on && c.checked)) c.click(); } })()`);
  const checked = await js(`[...document.querySelectorAll('.rules-item input')].filter(c => c.checked).length`);
  await js(`[...document.querySelectorAll('.dialog.wide button')].find(b => b.textContent.trim() === 'Aperçu').click()`);
  for (let i = 0; i < 6 && !(await js(`!!document.querySelector('.rules-report-head')`)); i++) await sleep(5000);
  const head = await js(`document.querySelector('.rules-report-head')?.textContent ?? ''`);
  if (!head.startsWith('Aperçu')) throw new Error(`pas de rapport d'aperçu : ${head}`);
  const lines = await js(`[...document.querySelectorAll('.rules-report summary')].map(s => s.textContent)`);
  if (lines.length !== checked) throw new Error(`${lines.length} lignes de rapport pour ${checked} règles cochées`);
  if (checked !== 12) throw new Error(`${checked} règles du PST cochées, 12 actives attendues`);
  log(`${checked} règles du PST : ${head}`);
  log(lines.slice(0, 4).join(' | '));
  await closeRules();
}]);

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
