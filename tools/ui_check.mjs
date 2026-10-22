// Vérification d'interface des jalons, sur l'application réelle et ses données.
// Lance ThunderLook avec le port de débogage de WebView2 (CDP), pilote la page de l'intérieur (aucune entrée
// système, rien ne peut atteindre une autre application), contrôle, puis remet les réglages et ferme l'application.
// Usage : node tools/ui_check.mjs [chemin\thunderlook.exe]      (code de sortie 1 si une vérification échoue)
// Chaque jalon ajoute ses vérifications dans CHECKS.
import { spawn } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';

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
    if (e.classList.contains('rules-origin')) { on = e.title.startsWith('pst:'); continue; }
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

CHECKS.push(['J6 : actions rapides (galerie, première utilisation, éditeur, exécution annulée par Ctrl+Z)', async (log) => {
  // Le fichier des actions rapides est remis tel quel à la fin, et la catégorie posée est annulée.
  const file = path.join(process.env.APPDATA, 'fr.local.thunderlook', 'quicksteps.json');
  const saved = fs.existsSync(file) ? fs.readFileSync(file) : null;
  const STEP = 'Vérification ThunderLook';
  const CAT = 'Vérif ThunderLook';
  const btn = (text) => `[...document.querySelectorAll('.dialog.wide button')].find(b => b.textContent.trim().startsWith(${JSON.stringify(text)}))`;
  const openMenu = async () => {
    await clickTab('Accueil');
    await js(`document.querySelector('button[aria-label="Actions rapides"]').click()`);
    await until(`document.querySelectorAll('[role=menuitem]').length > 0`, 'menu Actions rapides');
    return js(`[...document.querySelectorAll('[role=menuitem]')].map(b => b.textContent.trim())`);
  };
  const pick = async (label) => js(`[...document.querySelectorAll('[role=menuitem]')].find(b => b.textContent.trim() === ${JSON.stringify(label)}).click()`);
  try {
    await js(`(() => { const t = [...document.querySelectorAll('.treeitem')].find(e => /Boîte de réception/.test(e.textContent)); t.click(); })()`);
    await sleep(900);
    await js(`${ROW(1)}.click()`);
    const items = await openMenu();
    log(`galerie : ${items.join(' | ')}`);
    if (!items.includes('Déplacer vers : ?')) throw new Error('« Déplacer vers : ? » absent de la galerie');
    // Première utilisation : l'éditeur s'ouvre au lieu d'exécuter.
    await pick('Déplacer vers : ?');
    await until(`document.querySelector('.dialog.wide')?.textContent.includes('Première utilisation')`, 'éditeur de première utilisation');
    log('« Déplacer vers : ? » : éditeur de première utilisation, annulé');
    await js(`${btn('Annuler')}.click()`);
    await until(`!document.querySelector('.dialog.wide')`, 'fermeture de l\'éditeur');
    // Création d'une action rapide « classer » dans l'éditeur.
    await openMenu();
    await pick('Créer une action rapide…');
    await until(`document.querySelector('.qs-row select')`, 'éditeur');
    await js(`(() => { const i = document.querySelector('.dialog.wide .field input'); i.value = ${JSON.stringify(STEP)}; i.dispatchEvent(new Event('input', { bubbles: true })); })()`);
    await js(`(() => { const s = document.querySelector('.qs-row select'); s.value = [...s.options].find(o => o.textContent === 'Classer le message').value; s.dispatchEvent(new Event('change', { bubbles: true })); })()`);
    await until(`document.querySelector('.qs-row input[aria-label="Catégorie"]')`, 'champ Catégorie');
    await js(`(() => { const i = document.querySelector('.qs-row input[aria-label="Catégorie"]'); i.value = ${JSON.stringify(CAT)}; i.dispatchEvent(new Event('input', { bubbles: true })); })()`);
    await js(`${btn('Enregistrer')}.click()`);
    await until(`!document.querySelector('.dialog.wide')`, 'enregistrement');
    const after = await openMenu();
    if (!after.includes(STEP)) throw new Error(`« ${STEP} » absente de la galerie : ${after.join(' | ')}`);
    await pick(STEP);
    await until(`!!${ROW(1)}.querySelector('.cat[title=${JSON.stringify(CAT)}]')`, 'catégorie posée sur le message');
    log(`« ${STEP} » exécutée : catégorie « ${CAT} » sur la ligne 1`);
    await js(`document.querySelector('[role=listbox]').focus()`);
    await cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'z', code: 'KeyZ', windowsVirtualKeyCode: 90, modifiers: 2 });
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'z', code: 'KeyZ', windowsVirtualKeyCode: 90, modifiers: 2 });
    await until(`!${ROW(1)}.querySelector('.cat[title=${JSON.stringify(CAT)}]')`, 'annulation par Ctrl+Z');
    log('Ctrl+Z : catégorie retirée');
    // Gérer : l'action rapide créée est supprimée.
    await openMenu();
    await pick('Gérer les actions rapides…');
    await until(`document.querySelectorAll('.dialog.wide .rules-item').length > 0`, 'boîte Gérer');
    await js(`[...document.querySelectorAll('.dialog.wide .rules-item')].find(e => e.textContent.trim() === ${JSON.stringify(STEP)}).click()`);
    await js(`${btn('Supprimer')}.click()`);
    await until(`![...document.querySelectorAll('.dialog.wide .rules-item')].some(e => e.textContent.trim() === ${JSON.stringify(STEP)})`, 'suppression');
    log(`Gérer : ${await js(`document.querySelectorAll('.dialog.wide .rules-item').length`)} actions rapides après suppression`);
    await js(`${btn('OK')}.click()`);
  } finally {
    await sleep(300);
    if (saved) fs.writeFileSync(file, saved); else if (fs.existsSync(file)) fs.unlinkSync(file);
  }
}]);

// Critère du J7 : envoi réel puis réception, sur le compte de test uniquement (data/test-account.json, non versionné).
// Le compte est ajouté s'il manque, puis retiré s'il a été ajouté ici ; les deux messages de test sont supprimés.
/** Envoi réel puis réception sur le compte de test ; `blocked` : l'expéditeur (le compte lui-même) est bloqué, le
 * message doit arriver dans son Courrier indésirable et non dans sa Boîte de réception. */
async function realSend(log, blocked) {
  const junkFile = path.join(process.env.APPDATA, 'fr.local.thunderlook', 'junk.json');
  const junkSaved = fs.existsSync(junkFile) ? fs.readFileSync(junkFile) : null;
  const accountFile = path.join(root, 'data', 'test-account.json');
  if (!fs.existsSync(accountFile)) throw new Error('data/test-account.json absent : compte de test non configuré');
  const acc = JSON.parse(fs.readFileSync(accountFile, 'utf8'));
  const section = `[...document.querySelectorAll('.tree-section, [role=treeitem]')].find(e => e.textContent.includes(${JSON.stringify(acc.address)}))`;
  const fill = (label, value) => js(`(() => { const i = document.querySelector('.dialog input[aria-label=${JSON.stringify(label)}]'); i.value = ${JSON.stringify(value)}; i.dispatchEvent(new Event('input', { bubbles: true })); })()`);
  const click = (selector, text) => js(`[...document.querySelectorAll(${JSON.stringify(selector)})].find(b => b.textContent.trim().startsWith(${JSON.stringify(text)})).click()`);
  let added = false;
  let cleanupError = null;
  if (!(await js(`!!${section}`))) {
    await clickTab('Envoyer/Recevoir');
    await js(`document.querySelector('button[aria-label="Paramètres du compte"]').click()`);
    await until(`!!document.querySelector('.dialog')`, 'boîte Paramètres du compte');
    await click('.dialog button', 'Ajouter');
    await fill('Adresse', acc.address);
    await fill('Nom', 'ThunderLook test');
    await fill('Mot de passe', acc.password);
    await click('.dialog button', 'Enregistrer');
    for (let i = 0; i < 12 && !(await js(`!!${section}`)); i++) await sleep(5000);
    if (!(await js(`!!${section}`))) throw new Error(`compte absent du volet des dossiers ; boîte : « ${await js(`document.querySelector('.dialog [role=status]')?.textContent ?? '(pas de message)'`)} »`);
    await js(`document.querySelector('.dialog')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`);
    added = true;
    log(`compte ${acc.address} ajouté`);
  }
  const subject = `ThunderLook J7${blocked ? ' bloqué' : ''} ${new Date().toISOString()}`;
  const inbox = `[...document.querySelectorAll('.treeitem')].find(e => (e.getAttribute('data-source') ?? '').includes(${JSON.stringify(acc.address)}) && /Boîte de réception/.test(e.textContent))`;
  const junk = `[...document.querySelectorAll('.treeitem')].find(e => (e.getAttribute('data-source') ?? '').includes(${JSON.stringify(acc.address)}) && /Courrier indésirable/.test(e.textContent))`;
  const target = blocked ? junk : inbox;
  if (blocked) {
    fs.writeFileSync(junkFile, JSON.stringify({ blocked: [acc.address.toLowerCase()], safe: [] }));
    log(`expéditeur ${acc.address} bloqué le temps de la vérification`);
  }
  try {
    // Rédaction et envoi.
    // Rédaction depuis la Boîte de réception du compte de test : « De » propose ce compte (jamais un compte réel).
    await clickTab('Accueil');
    for (let i = 0; i < 30 && !(await js(`!!${inbox}`)); i++) await sleep(2000);
    if (!(await js(`!!${inbox}`))) throw new Error('Boîte de réception du compte de test absente du volet des dossiers après 60 s');
    await js(`${inbox}.click()`);
    await sleep(800);
    await js(`document.querySelector('button[aria-label="Nouveau message"]').click()`);
    await until(`document.querySelector('.pane.compose iframe')?.contentDocument?.designMode === 'on'`, 'volet de rédaction');
    await until(`(document.querySelector('.compose-field select[aria-label="De"]')?.selectedOptions[0]?.textContent ?? '').includes(${JSON.stringify(acc.address)})`, `champ « De » sur ${acc.address}`);
    log(`De : ${await js(`document.querySelector('.compose-field select[aria-label="De"]').selectedOptions[0].textContent`)}`);
    await js(`(() => { const f = [...document.querySelectorAll('.compose-field input')]; const set = (i, v) => { i.value = v; i.dispatchEvent(new Event('input', { bubbles: true })); };
      set(f[0], ${JSON.stringify(acc.address)}); set(f[2], ${JSON.stringify(subject)}); })()`);
    await js(`(() => { const d = document.querySelector('.pane.compose iframe').contentDocument; d.body.focus(); d.execCommand('insertText', false, 'Envoi réel de vérification du J7.'); })()`);
    // Les passages d'envoi/réception se suivent vite (IDLE) : on écoute l'événement « sync » du cœur plutôt que la
    // barre d'état, qui ne montre que le dernier.
    await js(`(() => { window.__tlSync = []; window.__TAURI__.event.listen('sync', e => window.__tlSync.push(e.payload)); })()`);
    await sleep(300);
    const t0 = Date.now();
    await click('.compose-bar button', 'Envoyer');
    const sentEvent = `(window.__tlSync ?? []).some(e => !e.running && e.sent > 0)`;
    for (let i = 0; i < 60 && !(await js(sentEvent)); i++) await sleep(1000);
    if (!(await js(sentEvent))) throw new Error(`aucun passage avec envoi en 60 s ; événements : ${await js(`JSON.stringify((window.__tlSync ?? []).map(e => e.message))`)}`);
    log(`envoyé en ${((Date.now() - t0) / 1000).toFixed(1)} s : « ${subject} »`);
    // Réception : F9, puis le message apparaît dans la Boîte de réception du compte.
    await js(`document.querySelector('[role=listbox]').focus()`);
    await cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'F9', code: 'F9', windowsVirtualKeyCode: 120 });
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'F9', code: 'F9', windowsVirtualKeyCode: 120 });
    await js(`${target}.click()`);
    let found = false;
    for (let i = 0; i < 45 && !found; i++) {
      await sleep(2000);
      found = await js(`[...document.querySelectorAll('.msg')].some(e => (e.getAttribute('aria-label') ?? '').includes(${JSON.stringify(subject)}))`);
      if (!found && i % 5 === 4) await js(`${target}.click()`);
    }
    if (!found) throw new Error(`message envoyé non reçu en 90 s dans ${blocked ? 'le Courrier indésirable' : 'la Boîte de réception'}`);
    if (blocked) {
      await js(`${inbox}.click()`);
      await sleep(1500);
      if (await js(`[...document.querySelectorAll('.msg')].some(e => (e.getAttribute('aria-label') ?? '').includes(${JSON.stringify(subject)}))`)) {
        throw new Error('message d\'un expéditeur bloqué resté dans la Boîte de réception');
      }
    }
    log(`reçu dans ${blocked ? 'le Courrier indésirable (pas dans la Boîte de réception)' : 'la Boîte de réception'} du compte ${((Date.now() - t0) / 1000).toFixed(1)} s après l'envoi`);
  } finally {
    if (blocked) {
      if (junkSaved) fs.writeFileSync(junkFile, junkSaved);
      else if (fs.existsSync(junkFile)) fs.unlinkSync(junkFile);
    }
    // Nettoyage : les messages de test (reçu et envoyé) sont supprimés définitivement, sur le serveur aussi.
    for (const folder of [/Boîte de réception/, /Courrier indésirable/, /Éléments envoyés|Envoyés|Sent/]) {
      const ok = await js(`(() => { const t = [...document.querySelectorAll('.treeitem')].find(e => (e.getAttribute('data-source') ?? '').includes(${JSON.stringify(acc.address)}) && ${folder}.test(e.textContent)); if (!t) return false; t.click(); return true; })()`);
      if (!ok) continue;
      await sleep(1200);
      const pos = await js(`(() => { const r = [...document.querySelectorAll('.msg')].find(e => (e.getAttribute('aria-label') ?? '').includes(${JSON.stringify(subject)})); return r ? +r.getAttribute('aria-posinset') : 0; })()`);
      if (!pos) continue;
      await js(`${ROW(pos)}.click(); document.querySelector('[role=listbox]').focus()`);
      await cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'Delete', code: 'Delete', windowsVirtualKeyCode: 46, modifiers: 8 });
      await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Delete', code: 'Delete', windowsVirtualKeyCode: 46, modifiers: 8 });
      await until(`document.querySelector('.dialog')`, 'confirmation de suppression');
      await js(`[...document.querySelectorAll('.dialog button')].find(b => b.textContent.trim() === 'OK').click()`);
      await sleep(800);
    }
    // Les suppressions sont reportées sur le serveur au passage suivant. Le compte de test ne doit pas être retiré
    // avant : sa file de changements partirait avec lui et les messages resteraient sur le serveur. On relance donc
    // des passages (F9) jusqu'à ce que la file du compte soit vide (« pending » de la commande accounts), en suivant
    // l'événement « sync » (fin d'un passage commencé après la demande) plutôt que la barre d'état.
    const pendingOf = `window.__TAURI__.core.invoke('accounts').then(l => (l.find(a => a.address.toLowerCase() === ${JSON.stringify(acc.address.toLowerCase())}) ?? { pending: -1 }).pending)`;
    await js(`(() => { if (!window.__tlSync) { window.__tlSync = []; window.__TAURI__.event.listen('sync', e => window.__tlSync.push(e.payload)); } })()`);
    let pending = await js(pendingOf);
    for (let i = 0; i < 10 && pending > 0; i++) {
      const before = await js(`window.__tlSync.length`);
      await js(`document.querySelector('[role=listbox]').focus()`);
      await cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'F9', code: 'F9', windowsVirtualKeyCode: 120 });
      await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'F9', code: 'F9', windowsVirtualKeyCode: 120 });
      for (let k = 0; k < 120 && !(await js(`window.__tlSync.slice(${before}).some(e => !e.running)`)); k++) await sleep(500);
      pending = await js(pendingOf);
    }
    if (pending === 0) log('suppressions reportées sur le serveur (file du compte de test vide)');
    else cleanupError = `nettoyage incomplet : ${pending} changement(s) du compte de test non reporté(s) sur le serveur (messages de test restés sur le serveur ; target/tl-scripts/purge_test_j7.py)`;
    if (added) {
      await clickTab('Envoyer/Recevoir');
      await js(`document.querySelector('button[aria-label="Paramètres du compte"]').click()`);
      await until(`!!document.querySelector('.dialog')`, 'boîte Paramètres du compte');
      await js(`[...document.querySelectorAll('.dialog .rules-item')].find(e => e.textContent.includes(${JSON.stringify(acc.address)}))?.click()`);
      await click('.dialog button', 'Supprimer');
      await sleep(1500);
      await js(`document.querySelector('.dialog')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`);
      log('compte de test retiré de ThunderLook (rien n\'est effacé sur le serveur)');
    }
  }
  if (cleanupError) throw new Error(cleanupError);
}
CHECKS.push(['J7 : envoi réel puis réception sur le compte de test', (log) => realSend(log, false)]);
CHECKS.push(['J7 : expéditeur bloqué, message reçu dans le Courrier indésirable', (log) => realSend(log, true)]);

CHECKS.push(['J9 : calendrier (vues, navigateur, superposition, détail), comparé aux occurrences d\'Outlook', async (log) => {
  // Le choix des calendriers affichés (calendar.json) et le type de vue sont remis tels quels à la fin.
  const file = path.join(process.env.APPDATA, 'fr.local.thunderlook', 'calendar.json');
  const saved = fs.existsSync(file) ? fs.readFileSync(file) : null;
  // Calendrier importé (étape 5) : l'export .ics d'Outlook, déposé comme le ferait « Ouvrir un calendrier ».
  const config = JSON.parse(fs.readFileSync(path.join(process.env.APPDATA, 'fr.local.thunderlook', 'config.json'), 'utf8'));
  const importedDir = path.join(config.dataDir, 'calendars');
  const importedFile = path.join(importedDir, 'Vérification ThunderLook.ics');
  fs.mkdirSync(importedDir, { recursive: true });
  fs.copyFileSync(path.join(root, 'data', 'samples', '031023-Calendrier de Paul Adam  Adam- déclic.ics'), importedFile);
  const press = (k, mods = {}) => js(`window.dispatchEvent(new KeyboardEvent('keydown', { key: ${JSON.stringify(k)}, ctrlKey: ${!!mods.ctrl}, altKey: ${!!mods.alt}, bubbles: true }))`);
  const title = () => js(`document.querySelector('.cal-title')?.textContent ?? ''`);
  const KINDS = ['Jour', 'Semaine de travail', 'Semaine', 'Mois'];
  const kind = (label) => press(String(KINDS.indexOf(label) + 1), { ctrl: true, alt: true });
  const kindIs = (label) => `document.querySelector('.cal-kind[aria-checked=true]')?.textContent === ${JSON.stringify(label)}`;
  /** Navigateur de dates : mois précédent jusqu'à `caption`, puis clic sur le jour. */
  const goTo = async (caption, date) => {
    for (let i = 0; i < 80 && (await js(`document.querySelector('.nav-title')?.textContent`)) !== caption; i++) {
      const before = await js(`document.querySelector('.nav-title')?.textContent`);
      await js(`document.querySelector('button[aria-label="Mois précédent"]').click()`);
      await until(`document.querySelector('.nav-title')?.textContent !== ${JSON.stringify(before)}`, `mois précédent (${before})`);
    }
    await js(`document.querySelector('.nav-day[aria-label="${date}"]').click()`);
    await until(`document.querySelector('.nav-day[aria-label="${date}"]')?.classList.contains('selected')`, `jour ${date} affiché`);
  };
  let initialKind;
  try {
    await press('2', { ctrl: true });
    await until(`!!document.querySelector('.pane.calendar') && (document.querySelector('.cal-title')?.textContent ?? '') !== ''`, 'module Calendrier (Ctrl+2)');
    initialKind = await js(`document.querySelector('.cal-kind[aria-checked=true]')?.textContent`);
    await kind('Semaine');
    await until(`${kindIs('Semaine')} && document.querySelectorAll('.cal-dayhead').length === 7`, 'vue Semaine (Ctrl+Alt+3)');
    const today = await js(`!!document.querySelector('.nav-day.today') && !!document.querySelector('.cal-col.today .cal-now')`);
    if (!today) throw new Error('aujourd\'hui sans repère (navigateur ou ligne de l\'heure)');
    log(`Ctrl+2 puis Ctrl+Alt+3 : « ${await title()} », aujourd'hui et la ligne de l'heure marqués`);
    const note = await js(`document.querySelector('.cal-note')?.textContent ?? ''`);
    log(`mention : ${note}`);

    // Semaine du 6 février 2023, calendrier principal : bandeau « Journée » d'après les occurrences d'Outlook.
    await goTo('février 2023', '2023-02-08');
    await until(`document.querySelector('.cal-title')?.textContent === '6 – 12 février 2023'`, 'titre de la semaine du 6 février 2023');
    const band = await js(`[...document.querySelectorAll('.cal-allday-cell')].map(c => c.querySelectorAll('.cal-ev').length)`);
    if (band.join() !== '0,0,2,1,3,0,0') throw new Error(`bandeau Journée : ${band.join()} ; attendu 0,0,2,1,3,0,0 (Outlook)`);
    const bilan = await js(`[...document.querySelectorAll('.cal-allday-cell')].filter(c => c.textContent.includes('BILAN ADI')).length`);
    if (bilan !== 3) throw new Error(`« BILAN ADI… » sur ${bilan} jours, attendu 3`);
    log('semaine du 6 février 2023 : bandeau 0,0,2,1,3,0,0 comme Outlook ; « BILAN ADI… » sur 3 jours');

    // Jour du 6 mars 2023 avec « Calendar Annexe » superposé : trois rendez-vous aux heures d'Outlook.
    await kind('Jour');
    await until(kindIs('Jour'), 'vue Jour (Ctrl+Alt+1)');
    await js(`document.querySelector('.nav-day[aria-label="2023-03-06"]').click()`);
    await until(`document.querySelector('.cal-title')?.textContent === 'lundi 6 mars 2023'`, 'jour du 6 mars 2023');
    const annexe = `[...document.querySelectorAll('.cal-item')].find(l => l.querySelector('.name')?.textContent === 'Calendar Annexe')?.querySelector('input')`;
    const wasOn = await js(`${annexe}?.checked`);
    if (wasOn === undefined) throw new Error('calendrier « Calendar Annexe » absent de la liste');
    if (!wasOn) await js(`${annexe}.click()`);
    await until(`document.querySelectorAll('.cal-timed').length >= 3`, 'rendez-vous de Calendar Annexe superposés');
    const timed = await js(`[...document.querySelectorAll('.cal-timed')].map(t => ({ top: parseFloat(t.style.top), s: t.querySelector('.cal-subject').textContent }))`);
    const want = [[14, 'AV martine magné'], [15.5, 'Gendarmerie'], [16.5, 'ADI retour dieumegard ?']];
    for (const [h, s] of want) {
      if (!timed.some((t) => t.s === s && Math.abs(t.top - h * 44) < 0.5)) throw new Error(`« ${s} » absent ou mal placé (${JSON.stringify(timed)})`);
    }
    log(`6 mars 2023 avec Calendar Annexe : ${want.map(([h, s]) => `${s} à ${h} h`).join(', ')}`);
    await js(`[...document.querySelectorAll('.cal-timed .cal-ev')].find(b => b.textContent.includes('Gendarmerie')).click()`);
    await until(`document.querySelector('.cal-details h3')?.textContent === 'Gendarmerie'`, 'détail du rendez-vous');
    const when = await js(`document.querySelector('.cal-details dd')?.textContent`);
    if (!when.startsWith('15:30 – 16:30')) throw new Error(`détail : « ${when} »`);
    log(`clic : détail « Gendarmerie », ${when}`);
    await key('Escape');
    if (!wasOn) await js(`${annexe}.click()`);
    await until(`document.querySelectorAll('.cal-timed').length === 0`, 'Calendar Annexe masqué');

    // Le même jour, depuis le calendrier importé (export .ics d'Outlook d'octobre 2023) : mêmes trois rendez-vous.
    const imported = `[...document.querySelectorAll('.cal-item')].find(l => l.querySelector('.name')?.textContent === 'Paul Adam | Adam- déclic')`;
    const section = await js(`[...document.querySelectorAll('.cal-section')].map(s => s.textContent).includes('Calendriers importés')`);
    if (!section || !(await js(`!!${imported}`))) throw new Error('calendrier importé absent de la liste');
    await js(`${imported}.querySelector('input').click()`);
    await until(`document.querySelectorAll('.cal-timed').length >= 3`, 'rendez-vous du calendrier importé');
    const fromIcs = await js(`[...document.querySelectorAll('.cal-timed')].map(t => ({ top: parseFloat(t.style.top), s: t.querySelector('.cal-subject').textContent }))`);
    for (const [h, s] of want) {
      if (!fromIcs.some((t) => t.s === s && Math.abs(t.top - h * 44) < 0.5)) throw new Error(`calendrier importé : « ${s} » absent ou mal placé (${JSON.stringify(fromIcs)})`);
    }
    log('calendrier importé (.ics d\'Outlook) : section « Calendriers importés », mêmes trois rendez-vous le 6 mars 2023');
    await js(`${imported}.querySelector('button[aria-label^="Retirer"]').click()`);
    await until(`!${imported}`, 'retrait du calendrier importé');
    if (fs.existsSync(importedFile)) throw new Error('fichier du calendrier importé toujours présent');
    log('× : calendrier importé retiré (fichier supprimé)');

    await kind('Mois');
    await until(`${kindIs('Mois')} && document.querySelectorAll('.cal-cell').length === 42`, 'vue Mois (Ctrl+Alt+4)');
    log(`Ctrl+Alt+4 : « ${await title()} », 42 cases`);
  } finally {
    if (fs.existsSync(importedFile)) fs.unlinkSync(importedFile);
    if (initialKind) await kind(initialKind);
    await sleep(300);
    if (saved) fs.writeFileSync(file, saved);
    else if (fs.existsSync(file)) fs.unlinkSync(file);
    await press('1', { ctrl: true });
    await sleep(300);
  }
}]);

CHECKS.push(['Clic droit : les actions des sous-menus s\'exécutent (Règles › Gérer, Créer une règle)', async (log) => {
  // Régression du 22/10 : les entrées d'un sous-menu du clic droit ne faisaient rien (rappels d'une portée détruite).
  const ctx = `(() => { const e = document.querySelector('.msg[aria-posinset="1"]'); e.click(); const b = e.getBoundingClientRect(); e.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: b.left + 80, clientY: b.top + 20 })); })()`;
  const item = (t) => `[...document.querySelectorAll('[role=menuitem]')].find(e => e.textContent.trim().startsWith(${JSON.stringify(t)}))`;
  const escape = () => js(`document.querySelector('.dialog')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`);
  for (const [entry, check] of [['Gérer les règles', `!!document.querySelector('.dialog.wide .rules-item')`], ['Créer une règle', `document.querySelector('#create-rule-title')?.textContent === 'Créer une règle'`]]) {
    await js(ctx);
    await until(`!!${item('Règles')}`, 'menu contextuel');
    await js(`${item('Règles')}.click()`);
    await until(`!!${item(entry)}`, 'sous-menu Règles');
    await js(`${item(entry)}.click()`);
    await until(check, `boîte ouverte par « ${entry} »`);
    log(`clic droit › Règles › ${entry}… : boîte ouverte`);
    await escape();
    await sleep(300);
  }
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
  // UI_ONLY=J9 : seulement les vérifications dont le nom commence ainsi.
  for (const [name, run] of CHECKS.filter(([n]) => !process.env.UI_ONLY || n.startsWith(process.env.UI_ONLY))) {
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
