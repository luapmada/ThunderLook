// Capture d'écran de la page de ThunderLook par CDP (Page.captureScreenshot), après un script facultatif.
// Lancer ThunderLook avec WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>.
// Usage : node tools/shot.mjs <fichier.png> ["<expression JS à évaluer avant>"]   (CDP_PORT, défaut 9222)
import fs from 'node:fs';
import { setTimeout as sleep } from 'node:timers/promises';

const port = process.env.CDP_PORT ?? '9222';
const [out, before] = process.argv.slice(2);
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((t) => t.type === 'page' && !t.url.startsWith('devtools'));
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((ok, ko) => { ws.onopen = ok; ws.onerror = ko; });
let id = 0;
const pending = new Map();
ws.onmessage = (m) => {
  const msg = JSON.parse(m.data);
  if (pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
};
const send = (method, params = {}) => new Promise((ok) => { const i = ++id; pending.set(i, ok); ws.send(JSON.stringify({ id: i, method, params })); });
if (before) {
  await send('Runtime.evaluate', { expression: before, awaitPromise: true });
  await sleep(1200);
}
const r = await send('Page.captureScreenshot', { format: 'png' });
fs.writeFileSync(out, Buffer.from(r.result.data, 'base64'));
console.log(`capture : ${out}`);
ws.close();
