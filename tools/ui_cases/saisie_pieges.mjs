// Cas L-saisie-pièges : défauts de saisie relevés par l'audit du 28/10 (notes/cloud/saisie-*.md), essayés sur l'application.
// Chaque boîte est annulée : rien n'est écrit, pas de base de test.
//  - Rendez-vous : « Intervalle de répétition » vidé puis 3 → « 3 » (et non « 13 ») ;
//  - Rendez-vous : avancer le début d'un jour déplace la fin d'autant ;
//  - Rendez-vous : Ctrl+N dans la boîte ne remplace pas le rendez-vous en cours de saisie ;
//  - Contact : Ctrl+2 dans la boîte ne change pas de module derrière elle.
import { js, until, press, send, sleep, clickTab, closeDialogs, setValue } from './lib.mjs';

export const name = 'L-saisie-pièges : intervalle, début → fin, Ctrl+N et Ctrl+2 dans une boîte';

const q = JSON.stringify;
const input = (label) => `document.querySelector('.dialog [aria-label=${q(label)}]')`;

async function openBox(module, command) {
  await closeDialogs();
  await press(String(module), { ctrl: true });
  await sleep(2500);
  await clickTab('Accueil');
  await js(`[...document.querySelectorAll('.cmd')].find(b => b.textContent.trim() === ${q(command)})?.click()`);
  await until(`!!document.querySelector('.dialog')`, `ouverture de « ${command} »`, 8000);
  await sleep(500);
}
async function cancel() {
  await js(`[...document.querySelectorAll('.dialog button')].find(b => b.textContent.trim() === 'Annuler')?.click()`);
  await until(`!document.querySelector('.dialog')`, 'fermeture par Annuler');
}
const addDays = (ymd, n) => {
  const [y, m, d] = ymd.split('-').map(Number);
  const t = new Date(y, m - 1, d + n);
  return `${t.getFullYear()}-${String(t.getMonth() + 1).padStart(2, '0')}-${String(t.getDate()).padStart(2, '0')}`;
};

export async function run(log) {
  const faults = [];
  try {
    // 1. Intervalle de répétition.
    await openBox(2, 'Nouveau rendez-vous');
    await js(`(() => { const s = ${input('Répéter')}; s.value = 'weekly'; s.dispatchEvent(new Event('change', { bubbles: true })); })()`);
    await until(`!!${input('Intervalle de répétition')}`, 'champ « Intervalle de répétition »');
    await js(`${input('Intervalle de répétition')}.focus()`);
    for (let i = 0; i < 3; i++) {
      await send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Backspace', code: 'Backspace', windowsVirtualKeyCode: 8 });
      await send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Backspace', code: 'Backspace', windowsVirtualKeyCode: 8 });
    }
    await send('Input.insertText', { text: '3' });
    await sleep(300);
    const every = await js(`({ v: ${input('Intervalle de répétition')}.value, focus: document.activeElement === ${input('Intervalle de répétition')} })`);
    if (every.v !== '3' || !every.focus) faults.push(`Intervalle : vidé puis « 3 » donne « ${every.v} »${every.focus ? '' : ', focus perdu'}`);
    log(`intervalle vidé puis 3 : « ${every.v} »`);

    // 2. Début → fin.
    const before = await js(`({ s: ${input('Début (date)')}?.value, e: ${input('Fin (date)')}?.value })`);
    if (!before.s) {
      log('champs « Début (date) » / « Fin (date) » introuvables (sauté)');
    } else {
      await setValue(`.dialog [aria-label="Début (date)"]`, addDays(before.s, 1));
      await js(`${input('Début (date)')}.dispatchEvent(new Event('change', { bubbles: true }))`);
      await sleep(300);
      const end = await js(`${input('Fin (date)')}.value`);
      if (end !== addDays(before.e, 1)) faults.push(`Début avancé d'un jour : fin ${before.e} → ${end}, attendu ${addDays(before.e, 1)}`);
      log(`début ${before.s} → ${addDays(before.s, 1)} : fin ${before.e} → ${end}`);
    }

    // 3. Ctrl+N dans la boîte.
    await setValue(`.dialog [aria-label="Objet"]`, 'zq9 objet');
    await js(`document.querySelector('.dialog input[type=checkbox]')?.focus()`);
    await press('n', { ctrl: true });
    await sleep(800);
    const subject = await js(`${input('Objet')}?.value ?? null`);
    if (subject !== 'zq9 objet') faults.push(`Ctrl+N dans la boîte Rendez-vous : objet « ${subject} » (saisie perdue)`);
    log(`Ctrl+N dans la boîte : objet « ${subject} »`);
    await cancel();

    // 4. Ctrl+2 dans la fiche contact.
    await openBox(3, 'Nouveau contact');
    await setValue(`.dialog [aria-label="Nom"]`, 'zq9');
    await press('2', { ctrl: true });
    await sleep(800);
    const still = await js(`({ boite: !!document.querySelector('.dialog'), nom: ${input('Nom')}?.value ?? null, calendrier: !!document.querySelector('.cal-grid, .calendar, .cal-month') })`);
    if (!still.boite || still.nom !== 'zq9') faults.push(`Ctrl+2 dans la boîte Contact : boîte ${still.boite ? 'restée' : 'fermée'}, Nom « ${still.nom} »`);
    log(`Ctrl+2 dans la boîte Contact : boîte ${still.boite ? 'restée ouverte' : 'FERMÉE'}, Nom « ${still.nom} »`);
    if (still.boite) await cancel();
  } finally {
    await closeDialogs();
  }
  if (faults.length) throw new Error(`${faults.length} défaut(s) :\n  ${faults.join('\n  ')}`);
}
