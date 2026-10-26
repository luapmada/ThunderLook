import { launch, connect, js, sleep } from './lib.mjs';
import fs from 'node:fs';
const p = launch(process.argv[2]);
await connect(); await sleep(12000);
const inv = (c, a) => js(`window.__TAURI__.core.invoke('${c}', ${JSON.stringify(a||{})}).then(r=>JSON.stringify(r)).catch(e=>'ERR '+e)`);
const old = JSON.parse(await inv('auto_sync_journal'))[0]?.at;
await inv('auto_sync_run_now', { force: false });
for (let i = 0; i < 60; i++) { await sleep(5000); const j = JSON.parse(await inv('auto_sync_journal')); if (j[0] && j[0].at !== old) { console.log(JSON.stringify(j[0]).slice(0, 500)); break; } }
const cats = ['ADE','ADI','ANIMAUX','APICULTURE','ActiVie','Admin','CEM','Clients','JARDIN','Musique','PERSO','Sorties','TOURS','Trading','Vacances','WEB'];
const dir = 'A:/PROJETS/IA-LAB/rea-outlook-clone/data/store/exchange-undo/';
const f = fs.readdirSync(dir).filter(n => n.startsWith('20261026T1847')).map(n => dir + n)[0];
const ids = JSON.parse(fs.readFileSync(f, 'utf8')).created.filter(c => c.item === 'event').map(c => c.id);
let done = 0, skipped = 0;
for (const id of ids) {
  const r = await inv('appointment_form', { id });
  if (r.startsWith('ERR')) { skipped++; continue; }
  const form = JSON.parse(r);
  const first = (form.subject || '').trim().split(/\s+/)[0].toLowerCase();
  const cat = cats.find(c => c.toLowerCase() === first);
  if (!cat || (form.categories || '').trim()) { skipped++; continue; }
  form.categories = cat;
  const s = await inv('appointment_save', { form });
  if (s.startsWith('ERR')) console.log('ERR', id, s.slice(0, 120)); else done++;
}
console.log('catégories posées:', done, 'ignorés:', skipped, 'sur', ids.length);
console.log(JSON.stringify(JSON.parse(await inv('auto_sync_state')).last).slice(0, 300));
p.kill();
