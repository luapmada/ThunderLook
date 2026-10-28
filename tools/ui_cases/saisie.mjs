// Cas L-saisie : dans chaque boîte de création, taper caractère par caractère dans CHAQUE champ texte ; le focus doit
// rester dans le champ et la valeur doit être complète (défaut du 28/10 : le focus repartait sur le champ photo du contact
// à chaque frappe). Chaque boîte est annulée, la rédaction fermée sans enregistrer : rien n'est écrit, pas de base de test.
import { js, until, press, send, sleep, clickTab, closeDialogs } from './lib.mjs';

export const name = 'L-saisie : frappe dans tous les champs des boîtes (focus et valeur)';

const q = JSON.stringify;
// [module Ctrl+n, onglet du ruban, commande, conteneur des champs]. Aucune de ces boîtes n'écrit pendant la frappe :
// tout part à « Enregistrer » / « OK », jamais cliqués ici.
const BOXES = [
  [3, 'Accueil', 'Nouveau contact', '.dialog'],
  [3, 'Accueil', 'Nouvelle liste', '.dialog'],
  [2, 'Accueil', 'Nouveau rendez-vous', '.dialog'],
  [4, 'Accueil', 'Nouvelle tâche', '.dialog'],
  [5, 'Accueil', 'Nouvelle note', '.dialog'],
  [1, 'Accueil', 'Nouveau message', '.compose'],
  [1, 'Envoyer/Recevoir', 'Réponses automatiques', '.dialog'],
  [1, 'Envoyer/Recevoir', 'Paramètres du compte', '.dialog'],
  [1, 'Dossier', 'Nouveau dossier', '.dialog'],
  [1, 'Accueil', "Réglages de l'affichage", '.dialog'],
];
const CLOSE = ['Annuler', 'Fermer', 'Close'];
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
    for (const [module, tab, command, scope] of BOXES) {
      await closeDialogs();
      await press(String(module), { ctrl: true });
      await sleep(2500);
      await clickTab(tab);
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
        // Bouton de fermeture SANS enregistrement ; s'il n'y en a pas, Échap.
        const closed = await js(`(() => { const b = [...document.querySelectorAll('.dialog .dialog-buttons button, .dialog button')].find(b => ${q(CLOSE)}.includes(b.textContent.trim())); if (!b) return false; b.click(); return true; })()`);
        if (!closed) await closeDialogs();
        await until(`!document.querySelector('.dialog')`, `fermeture de « ${command} » sans enregistrer`);
      }
    }
    // Carnet d'adresses : ouvert par « À… » dans la rédaction (la commande du ruban n'est pas encore branchée). La recherche
    // recharge la liste à chaque frappe : le champ doit garder le focus. Annulé, rédaction fermée sans enregistrer.
    {
      await closeDialogs();
      await press('1', { ctrl: true });
      await sleep(1500);
      await clickTab('Accueil');
      await js(`[...document.querySelectorAll('.cmd')].find(b => b.textContent.trim() === 'Nouveau message')?.click()`);
      await until(`!!document.querySelector('.compose')`, 'rédaction', 8000);
      await sleep(1500);
      await js(`[...document.querySelectorAll('.compose button')].find(b => b.textContent.trim() === 'À…')?.click()`);
      await until(`!!document.querySelector('.dialog')`, 'carnet d\'adresses', 8000);
      await sleep(1500);
      const fields = await fieldsOf('.dialog');
      let ok = 0;
      for (const f of fields) {
        await js(`(() => { const e = ${el('.dialog', f.i)}; e.focus(); e.select?.(); })()`);
        for (const ch of TEXT) {
          await send('Input.insertText', { text: ch });
          await sleep(150);
        }
        await sleep(800);
        const r = await js(`(() => { const e = ${el('.dialog', f.i)}; return { focus: document.activeElement === e, active: document.activeElement?.getAttribute('aria-label') ?? document.activeElement?.tagName, value: e?.value ?? null }; })()`);
        if (!r.focus || !String(r.value).includes(TEXT)) faults.push(`carnet d'adresses › « ${f.label} » : focus ${r.focus ? 'gardé' : 'parti vers « ' + r.active + ' »'}, valeur « ${r.value} »`);
        else ok++;
      }
      log(`carnet d'adresses (depuis « À… ») : ${ok}/${fields.length} champs corrects`);
      await js(`[...document.querySelectorAll('.dialog button')].find(b => ${q(CLOSE)}.includes(b.textContent.trim()))?.click()`);
      await sleep(500);
      await closeDialogs();
      await js(`document.querySelector('.compose button[title="Fermer la rédaction"]')?.click()`);
      await sleep(500);
    }

    // Éditeur d'actions rapides : les paramètres d'une action (« Catégorie », « À ») n'existent qu'une fois la sorte choisie ;
    // ces lignes étaient recréées à chaque frappe (focus perdu). « Annuler » : rien n'est enregistré.
    for (const kind of [/^Classer le message$/, /^Transférer$/, /^Nouveau message$/]) {
      await closeDialogs();
      await press('1', { ctrl: true });
      await sleep(1500);
      await clickTab('Accueil');
      await js(`[...document.querySelectorAll('.cmd')].find(b => b.textContent.trim() === 'Actions rapides')?.click()`);
      await sleep(400);
      const menu = await js(`(() => { const m = [...document.querySelectorAll('[role=menuitem], .menu-item, .menu button, .menu li')].find(e => e.textContent.trim().startsWith('Créer une action rapide')); if (!m) return false; m.click(); return true; })()`);
      if (!menu) {
        log('actions rapides : entrée « Créer une action rapide… » introuvable (sautée)');
        break;
      }
      await until(`!!document.querySelector('.dialog select[aria-label="Action 1"]')`, 'éditeur d\'action rapide', 8000);
      const chosen = await js(`(() => { const s = document.querySelector('.dialog select[aria-label="Action 1"]'); const o = [...s.options].find(o => ${kind}.test(o.textContent.trim())); if (!o) return null; s.value = o.value; s.dispatchEvent(new Event('change', { bubbles: true })); return o.textContent.trim(); })()`);
      await sleep(700);
      const fields = await fieldsOf('.dialog');
      let ok = 0;
      for (const f of fields) {
        await js(`(() => { const e = ${el('.dialog', f.i)}; e.focus(); e.select?.(); })()`);
        for (const ch of TEXT) {
          await send('Input.insertText', { text: ch });
          await sleep(60);
        }
        await sleep(250);
        const r = await js(`(() => { const e = ${el('.dialog', f.i)}; return { focus: document.activeElement === e, active: document.activeElement?.getAttribute('aria-label') ?? document.activeElement?.tagName, value: e?.value ?? null }; })()`);
        if (!r.focus || !String(r.value).includes(TEXT)) faults.push(`action rapide « ${chosen} » › « ${f.label} » : focus ${r.focus ? 'gardé' : 'parti vers « ' + r.active + ' »'}, valeur « ${r.value} »`);
        else ok++;
      }
      log(`action rapide « ${chosen ?? kind} » : ${ok}/${fields.length} champs corrects`);
      await js(`[...document.querySelectorAll('.dialog button')].find(b => b.textContent.trim() === 'Annuler')?.click()`);
      await sleep(500);
      await closeDialogs();
    }
  } finally {
    await closeDialogs();
    await js(`document.querySelector('.compose button[title="Fermer la rédaction"]')?.click()`).catch(() => {});
  }
  if (faults.length) throw new Error(`${faults.length} champ(s) en défaut :\n  ${faults.join('\n  ')}`);
}
