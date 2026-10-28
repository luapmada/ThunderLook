// Cas L-saisie : dans chaque boîte de création, taper caractère par caractère dans CHAQUE champ texte ; le focus doit
// rester dans le champ et la valeur doit être complète (défaut du 28/10 : le focus repartait sur le champ photo du contact
// à chaque frappe). Chaque boîte est annulée, la rédaction fermée sans enregistrer : rien n'est écrit, pas de base de test.
import { js, until, press, send, sleep, clickTab, closeDialogs } from './lib.mjs';

export const name = 'L-saisie : frappe dans tous les champs des boîtes (focus et valeur)';

const q = JSON.stringify;
// [module Ctrl+n, commande du ruban (Accueil), conteneur des champs]
const BOXES = [
  [3, 'Nouveau contact', '.dialog'],
  [3, 'Nouvelle liste', '.dialog'],
  [2, 'Nouveau rendez-vous', '.dialog'],
  [4, 'Nouvelle tâche', '.dialog'],
  [5, 'Nouvelle note', '.dialog'],
  [1, 'Nouveau message', '.compose'],
];
const TEXT = 'zq9';

/** Champs saisissables visibles du conteneur, repérés par leur rang. */
const fieldsOf = (scope) => js(`[...document.querySelectorAll(${q(scope)} + ' input, ' + ${q(scope)} + ' textarea')]
  .map((e, i) => ({ i, e })).filter(({ e }) => !e.disabled && !e.readOnly && e.offsetParent !== null
    && (e.tagName === 'TEXTAREA' || ['text', 'email', 'search', 'tel', 'url', ''].includes(e.type)))
  .map(({ i, e }) => ({ i, label: e.getAttribute('aria-label') || e.placeholder || e.name || ('#' + i) }))`);
const el = (scope, i) => `document.querySelectorAll(${q(scope)} + ' input, ' + ${q(scope)} + ' textarea')[${i}]`;

export async function run(log) {
  const faults = [];
  try {
    for (const [module, command, scope] of BOXES) {
      await closeDialogs();
      await press(String(module), { ctrl: true });
      await sleep(2500);
      await clickTab('Accueil');
      // Ruban d'abord, puis tout bouton de la page (« Nouvelle liste » est dans le volet des contacts).
      const opened = await js(`(() => { const b = [...document.querySelectorAll('.cmd'), ...document.querySelectorAll('button')].find(b => b.textContent.trim() === ${q(command)} && b.getAttribute('aria-disabled') !== 'true'); if (!b) return false; b.click(); return true; })()`);
      if (!opened) {
        log(`« ${command} » : commande introuvable ou grisée (sautée)`);
        continue;
      }
      await until(`!!document.querySelector(${q(scope)})`, `ouverture de « ${command} »`, 8000);
      await sleep(600);
      const fields = await fieldsOf(scope);
      let ok = 0;
      for (const f of fields) {
        await js(`(() => { const e = ${el(scope, f.i)}; e.focus(); e.select?.(); })()`);
        for (const ch of TEXT) {
          await send('Input.insertText', { text: ch });
          await sleep(60);
        }
        await sleep(250);
        const r = await js(`(() => { const e = ${el(scope, f.i)}; return { focus: document.activeElement === e, active: document.activeElement?.getAttribute('aria-label') ?? document.activeElement?.tagName, value: e?.value ?? null }; })()`);
        if (!r.focus || !String(r.value).includes(TEXT)) faults.push(`« ${command} » › « ${f.label} » : focus ${r.focus ? 'gardé' : 'parti vers « ' + r.active + ' »'}, valeur « ${r.value} »`);
        else ok++;
      }
      log(`« ${command} » : ${ok}/${fields.length} champs corrects`);
      if (scope === '.compose') {
        await js(`document.querySelector('.compose button[title="Fermer la rédaction"]')?.click()`);
        await until(`!document.querySelector('.compose')`, 'fermeture de la rédaction');
      } else {
        await js(`[...document.querySelectorAll('.dialog button')].find(b => b.textContent.trim() === 'Annuler')?.click()`);
        await until(`!document.querySelector('.dialog')`, `fermeture de « ${command} » par Annuler`);
      }
    }
  } finally {
    await closeDialogs();
    await js(`document.querySelector('.compose button[title="Fermer la rédaction"]')?.click()`).catch(() => {});
  }
  if (faults.length) throw new Error(`${faults.length} champ(s) en défaut :\n  ${faults.join('\n  ')}`);
}
