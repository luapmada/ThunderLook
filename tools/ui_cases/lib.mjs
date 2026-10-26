// Moteur commun des vérifications d'interface : lancement de ThunderLook avec le port de débogage de WebView2 (CDP),
// client CDP minimal, `js`, `until`, `key`, `press`, et les aides partagées par les cas `tools/ui_cases/*.mjs`.
// Rien ici n'écrit dans les données : les aides d'écriture ne sont utilisées que par les cas protégés par `requireTestData`.
import { spawn } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';
import zlib from 'node:zlib';

export { sleep };
export const port = Number(process.env.TL_CDP_PORT) || 9333;

// --- Lancement et client CDP ----------------------------------------------------------------------
export function launch(exe) {
  return spawn(exe, [], {
    env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
    stdio: 'ignore',
  });
}

let cdp;
export async function connect() {
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
        cdp = { ws, send };
        return cdp;
      }
    } catch { /* pas encore prêt */ }
    await sleep(500);
  }
  throw new Error(`pas de page CDP sur le port ${port}`);
}

/** Commande CDP brute (valide après `connect`). */
export const send = (method, params) => cdp.send(method, params);

export async function js(expression) {
  const r = await cdp.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(`JS : ${r.result.exceptionDetails.exception?.description ?? expression}`);
  return r.result?.result?.value;
}
const KEYS = { ArrowRight: 39, ArrowLeft: 37, ArrowDown: 40, ArrowUp: 38, Enter: 13, Escape: 27, Home: 36, End: 35 };
export async function key(name) {
  for (const type of ['rawKeyDown', 'keyUp']) {
    await cdp.send('Input.dispatchKeyEvent', { type, key: name, code: name, windowsVirtualKeyCode: KEYS[name] ?? 0 });
  }
}
/** Attend que `expression` soit vraie (5 s par défaut). */
export async function until(expression, what, ms = 5000) {
  for (let i = 0; i < ms / 100; i++) {
    if (await js(expression)) return;
    await sleep(100);
  }
  throw new Error(`délai dépassé : ${what}`);
}
/** Raccourci clavier délivré à la fenêtre (comme les autres cas : aucune entrée système). */
export const press = (k, mods = {}) =>
  js(`window.dispatchEvent(new KeyboardEvent('keydown', { key: ${JSON.stringify(k)}, ctrlKey: ${!!mods.ctrl}, altKey: ${!!mods.alt}, shiftKey: ${!!mods.shift}, bubbles: true }))`);

export async function clickTab(label) {
  await js(`[...document.querySelectorAll('[role=tab]')].find(t => t.textContent.trim() === ${JSON.stringify(label)}).click()`);
  await sleep(200);
}

// --- Aides DOM ------------------------------------------------------------------------------------
const q = JSON.stringify;

/** Saisie dans un champ texte, date, heure, nombre ou zone de texte (événement `input`, comme au clavier). */
export const setValue = (selector, value) => js(`(() => {
  const e = document.querySelector(${q(selector)});
  if (!e) throw new Error('champ introuvable : ' + ${q(selector)});
  const proto = e.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, 'value').set.call(e, ${q(String(value))});
  e.dispatchEvent(new Event('input', { bubbles: true }));
})()`);
/** Choix dans une liste déroulante, par valeur (événement `change`). */
export const setSelect = (selector, value) => js(`(() => {
  const e = document.querySelector(${q(selector)});
  if (!e) throw new Error('liste introuvable : ' + ${q(selector)});
  e.value = ${q(String(value))};
  if (e.value !== ${q(String(value))}) throw new Error('valeur absente de ' + ${q(selector)} + ' : ' + ${q(String(value))});
  e.dispatchEvent(new Event('change', { bubbles: true }));
})()`);
/** Clic sur le bouton dont le texte est exactement `text`, dans `scope` ; faux si absent. */
export const clickButton = (scope, text) => js(`(() => {
  const b = [...document.querySelectorAll(${q(scope.startsWith('.cmd') ? scope : scope + ' button')})].find(b => b.textContent.trim() === ${q(text)} && b.getAttribute('aria-disabled') !== 'true');
  if (!b) return false;
  b.click();
  return true;
})()`);
export async function mustClick(scope, text) {
  if (!(await clickButton(scope, text))) throw new Error(`bouton introuvable ou grisé : « ${text} » dans ${scope}`);
}
/** Commande du ruban de l'onglet Accueil du module affiché. */
export async function ribbon(label) {
  await clickTab('Accueil');
  await mustClick('.cmd', label).catch(() => { throw new Error(`commande du ruban introuvable ou grisée : « ${label} »`); });
  await sleep(150);
}
/** Passe à un module (Ctrl+1 à Ctrl+5) et attend que `ready` soit vraie. */
export async function openModule(n, ready) {
  await press(String(n), { ctrl: true });
  await until(ready, `module Ctrl+${n}`);
}
/** Ferme une boîte de dialogue restée ouverte (Échap, puis « Annuler »). */
export async function closeDialogs() {
  for (let i = 0; i < 3 && (await js(`!!document.querySelector('.dialog')`)); i++) {
    await js(`(() => { const d = document.querySelector('.dialog'); d.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); })()`);
    await sleep(200);
  }
}

// --- Dates locales --------------------------------------------------------------------------------
const two = (n) => String(n).padStart(2, '0');
/** `AAAA-MM-JJ` du jour décalé de `days` (heure locale). */
export function ymd(days = 0, from = new Date()) {
  const d = new Date(from.getFullYear(), from.getMonth(), from.getDate() + days);
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`;
}
export const hm = (d) => `${two(d.getHours())}:${two(d.getMinutes())}`;

// --- Sécurité : écrire seulement dans une base de test --------------------------------------------
/** Préfixe de tout ce que les cas créent ; le balayage de fin supprime tout ce qui le porte. */
export const PREFIX = 'ZZ-L';
export const tag = (kind) => `${PREFIX}${kind}${Date.now().toString(36)}`;

/**
 * Les cas qui écrivent exigent `THUNDERLOOK_DATA` : un dossier existant, différent du dossier de données habituel
 * (`dataDir` de config.json). Sinon le cas se saute avec un message ; renvoie vrai si on peut écrire.
 */
export function requireTestData(log) {
  const dir = process.env.THUNDERLOOK_DATA;
  if (!dir) {
    log('SAUTÉ : THUNDERLOOK_DATA n\'est pas défini (le cas écrit des éléments « ZZ… » : base de test obligatoire)');
    return false;
  }
  if (!fs.existsSync(dir)) {
    log(`SAUTÉ : THUNDERLOOK_DATA pointe vers un dossier inexistant (${dir})`);
    return false;
  }
  try {
    const config = JSON.parse(fs.readFileSync(path.join(process.env.APPDATA ?? '', 'fr.local.thunderlook', 'config.json'), 'utf8'));
    if (config.dataDir && path.resolve(config.dataDir).toLowerCase() === path.resolve(dir).toLowerCase()) {
      log(`SAUTÉ : THUNDERLOOK_DATA désigne la base habituelle (${dir}), pas une copie de test`);
      return false;
    }
  } catch { /* pas de config.json lisible : seule la variable décide */ }
  return true;
}

// --- Fichier d'image pour le champ photo ----------------------------------------------------------
/** PNG 8×8 uni, écrit dans le dossier temporaire ; renvoie son chemin. */
export function tinyPng() {
  const w = 8, h = 8;
  const raw = Buffer.alloc((w * 3 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 3 + 1)] = 0;
    for (let x = 0; x < w; x++) raw.set([200, 60, 60], y * (w * 3 + 1) + 1 + x * 3);
  }
  const chunk = (type, data) => {
    const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type), data]);
    const crc = Buffer.alloc(4); crc.writeUInt32BE(zlib.crc32(body) >>> 0);
    return Buffer.concat([len, body, crc]);
  };
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr.set([8, 2, 0, 0, 0], 8);
  const png = Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
  const file = path.join(os.tmpdir(), `thunderlook-zz-photo-${process.pid}.png`);
  fs.writeFileSync(file, png);
  return file;
}
/** Renseigne un champ fichier par CDP (sans boîte système). */
export async function setFile(selector, file) {
  const { result: doc } = await cdp.send('DOM.getDocument', { depth: 0 });
  const { result: node } = await cdp.send('DOM.querySelector', { nodeId: doc.root.nodeId, selector });
  if (!node?.nodeId) throw new Error(`champ fichier introuvable : ${selector}`);
  const r = await cdp.send('DOM.setFileInputFiles', { nodeId: node.nodeId, files: [file] });
  if (r.error) throw new Error(`DOM.setFileInputFiles : ${r.error.message}`);
}
