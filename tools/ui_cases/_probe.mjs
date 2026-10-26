import { launch, connect, js, sleep } from './lib.mjs';
const p = launch(process.argv[2]); await connect(); await sleep(8000);
console.log(await js(`location.href + ' | ' + typeof window.__TAURI__ + ' | ' + document.body.innerText.slice(0,200)`));
p.kill();
