// Pilote la page de ThunderLook par le protocole de débogage de WebView2 (CDP) : évaluation de JavaScript et
// touches envoyées à la page seule (aucune entrée système, rien ne peut atteindre une autre application).
// Lancer ThunderLook avec WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222.
// Usage : node tools/cdp.mjs eval "<expression JS>"   |   node tools/cdp.mjs key ArrowRight
const port = process.env.CDP_PORT ?? '9222';
const [cmd, arg] = process.argv.slice(2);
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((t) => t.type === 'page');
if (!page) throw new Error('aucune page');
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((ok, ko) => { ws.onopen = ok; ws.onerror = ko; });
let id = 0;
const pending = new Map();
ws.onmessage = (m) => {
  const msg = JSON.parse(m.data);
  if (pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
};
const send = (method, params = {}) => new Promise((ok) => { const i = ++id; pending.set(i, ok); ws.send(JSON.stringify({ id: i, method, params })); });

if (cmd === 'eval') {
  const r = await send('Runtime.evaluate', { expression: arg, awaitPromise: true, returnByValue: true });
  console.log(JSON.stringify(r.result?.result?.value ?? r.result?.exceptionDetails ?? r, null, 1));
} else if (cmd === 'key') {
  const codes = { ArrowRight: 39, ArrowLeft: 37, ArrowDown: 40, ArrowUp: 38, Enter: 13, Home: 36, End: 35 };
  for (const type of ['rawKeyDown', 'keyUp']) {
    await send('Input.dispatchKeyEvent', { type, key: arg, code: arg, windowsVirtualKeyCode: codes[arg] ?? 0 });
  }
  console.log(`touche ${arg} envoyée à la page`);
} else {
  throw new Error('commande : eval | key');
}
ws.close();
