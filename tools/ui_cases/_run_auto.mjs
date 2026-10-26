import { launch, connect, js, sleep } from './lib.mjs';
const force = process.argv[3] === 'force';
const p = launch(process.argv[2]);
await connect(); await sleep(4000);
const inv = (c, a) => js(`window.__TAURI__.core.invoke('${c}', ${JSON.stringify(a||{})}).then(r=>JSON.stringify(r)).catch(e=>'ERR '+e)`);
console.log(await inv('auto_sync_set', { enabled: true, minutes: 60, autoExport: false }));
console.log(await inv('auto_sync_run_now', { force }));
const old = JSON.parse(await inv('auto_sync_journal'))[0]?.at;
for (let i = 0; i < 60; i++) { await sleep(5000); const j = JSON.parse(await inv('auto_sync_journal')); if (j[0] && j[0].at !== old) break; }
console.log(await inv('auto_sync_state'));
p.kill();
