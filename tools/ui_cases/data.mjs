// Aides partagées par les cas qui créent des contacts ou des rendez-vous « ZZ… » (voir lib.mjs : PREFIX, requireTestData).
import { sleep, js, until, setValue, setSelect, setFile, mustClick, ribbon, openModule, closeDialogs, PREFIX } from './lib.mjs';

const q = JSON.stringify;

/** Attend la fin d'un enregistrement : boîte fermée, ou message d'erreur de la boîte (lève alors l'erreur). */
export async function saveDialog(what) {
  await mustClick('.dialog .dialog-buttons', 'Enregistrer et fermer');
  const done = `!document.querySelector('.dialog') || !!document.querySelector('.dialog .dialog-note[role=alert]')`;
  await until(done, `${what} : enregistrement`, 15000);
  const alert = await js(`document.querySelector('.dialog .dialog-note[role=alert]')?.textContent ?? null`);
  if (alert) throw new Error(`${what} : ${alert}`);
  await sleep(300);
}
/** Erreur affichée dans la barre d'état (par exemple « Aucun calendrier commun »), sinon null. */
export const statusError = () => js(`document.querySelector('.statusbar .status-error')?.textContent ?? null`);

// --- Contacts -------------------------------------------------------------------------------------
export async function openContacts() {
  await openModule(3, `document.querySelectorAll('.contact-nav .cal-item').length > 0 && !!document.querySelector('.contact-rows') && !/Chargement/.test(document.querySelector('.contact-rows').textContent)`);
  // Un filtre laissé par une vérification précédente cacherait les lignes.
  await setValue('.contact-search input', '');
  await sleep(200);
}
const contactRow = (token) => `[...document.querySelectorAll('.contact-row')].find(r => r.textContent.includes(${q(token)}))`;
export async function createContact({ given = 'ZZ', surname, company = '', email = '', photo = null }) {
  await ribbon('Nouveau contact');
  await until(`!!document.querySelector('.dialog input[aria-label="Prénom"]')`, 'boîte Nouveau contact');
  await setValue('.dialog input[aria-label="Prénom"]', given);
  await setValue('.dialog input[aria-label="Nom"]', surname);
  if (company) await setValue('.dialog input[aria-label="Société"]', company);
  if (email) await setValue('.dialog input[aria-label="Courrier 1"]', email);
  if (photo) {
    await setFile('.dialog input[type=file]', photo);
    await until(`!!document.querySelector('.dialog img.avatar.large')`, 'aperçu de la photo choisie', 15000);
  }
  await saveDialog('contact');
  await until(`!!${contactRow(surname)}`, `contact « ${surname} » dans la liste`, 10000);
}
export async function selectContact(token) {
  await js(`${contactRow(token)}.click()`);
  await until(`document.querySelector('.contact-card h2')?.textContent.includes(${q(token)})`, `fiche de « ${token} »`);
}
/** Supprime les contacts du dossier affiché dont la ligne contient `token` (au plus 20). */
export async function deleteContacts(token) {
  await closeDialogs();
  await openContacts();
  for (let i = 0; i < 20 && (await js(`!!${contactRow(token)}`)); i++) {
    await js(`${contactRow(token)}.click()`);
    await sleep(300);
    await ribbon('Supprimer');
    await sleep(800);
  }
  if (await js(`!!${contactRow(token)}`)) throw new Error(`contact « ${token} » encore présent après suppression`);
}

// --- Calendrier -----------------------------------------------------------------------------------
export async function openCalendar() {
  await openModule(2, `!!document.querySelector('.cal-title')?.textContent && !document.querySelector('.pane.calendar .placeholder')`);
}
export async function setKind(label) {
  await js(`[...document.querySelectorAll('.cal-kind')].find(b => b.textContent.trim() === ${q(label)}).click()`);
  await until(`document.querySelector('.cal-kind[aria-checked=true]')?.textContent.trim() === ${q(label)}`, `vue ${label}`);
  await sleep(500);
}
/** Vue affichée (Jour, Semaine de travail, Semaine, Mois) : réglage partagé, à remettre en fin de cas. */
export const currentKind = () => js(`document.querySelector('.cal-kind[aria-checked=true]')?.textContent.trim() ?? null`);
const title = () => js(`document.querySelector('.cal-title')?.textContent ?? ''`);
/** « Aujourd'hui », puis `n` périodes plus tard (en vue Semaine : `n` semaines). */
export async function weekAhead(n) {
  await js(`[...document.querySelectorAll('.cal-head button')].find(b => b.textContent.trim() === "Aujourd'hui").click()`);
  await sleep(500);
  for (let i = 0; i < n; i++) await nextPeriod();
}
export async function nextPeriod() {
  const before = await title();
  await js(`document.querySelector('button[aria-label="Période suivante"]').click()`);
  await until(`document.querySelector('.cal-title')?.textContent !== ${q(before)}`, 'période suivante');
  await sleep(500);
}
/** Libellés (« objet, quand, lieu ») des rendez-vous affichés dont l'étiquette commence par `prefix`. */
export const chips = (prefix) => js(`[...document.querySelectorAll('.cal-ev')].map(e => e.getAttribute('aria-label')).filter(l => l.startsWith(${q(prefix)}))`);
export async function selectChip(prefix, index = 0) {
  const ok = await js(`(() => { const e = [...document.querySelectorAll('.cal-ev')].filter(e => e.getAttribute('aria-label').startsWith(${q(prefix)}))[${index}]; if (!e) return false; e.click(); return true; })()`);
  if (!ok) throw new Error(`rendez-vous « ${prefix} » absent de la période affichée`);
  await until(`!!document.querySelector('.cal-ev.selected')`, 'rendez-vous sélectionné');
}
export async function openSelected() {
  await js(`document.querySelector('.cal-ev.selected').dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))`);
  await until(`!!document.querySelector('.dialog input[aria-label="Objet"]')`, 'boîte Rendez-vous');
}
/** Supprime le rendez-vous sélectionné : `series` vrai = « Toute la série », sinon cette occurrence / le rendez-vous. */
export async function deleteSelected(series = false) {
  await js(`document.querySelector('.cal-ev.selected').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 300, clientY: 300 }))`);
  await sleep(300);
  const label = series ? 'Supprimer toute la série' : 'Supprimer';
  const ok = await js(`(() => { const b = [...document.querySelectorAll('[role=menuitem]')].find(b => b.textContent.trim().startsWith(${q(label)})); if (!b) return false; b.click(); return true; })()`);
  if (!ok) throw new Error(`entrée « ${label} » absente du menu du rendez-vous`);
  await sleep(800);
}
/**
 * Crée un rendez-vous par le ruban. `date` = AAAA-MM-JJ ; `from` / `to` = HH:MM ;
 * `repeat` = '' | 'weekly' | … ; `count` = nombre d'occurrences ; `reminder` = minutes (chaîne) ou null.
 */
export async function createAppointment({ subject, date, from, to, location = '', repeat = '', count = 0, reminder = null }) {
  await ribbon('Nouveau rendez-vous');
  await until(`!!document.querySelector('.dialog input[aria-label="Objet"]')`, 'boîte Nouveau rendez-vous');
  await setValue('.dialog input[aria-label="Objet"]', subject);
  if (location) await setValue('.dialog input[aria-label="Emplacement"]', location);
  await setValue('.dialog input[aria-label="Début (date)"]', date);
  await setValue('.dialog input[aria-label="Fin (date)"]', date);
  await setValue('.dialog input[aria-label="Début (heure)"]', from);
  await setValue('.dialog input[aria-label="Fin (heure)"]', to);
  if (repeat) {
    await setSelect('.dialog select[aria-label="Répéter"]', repeat);
    await until(`!!document.querySelector('.dialog select[aria-label="Fin de la répétition"]')`, 'champs de répétition');
    await setSelect('.dialog select[aria-label="Fin de la répétition"]', 'count');
    const nField = '.dialog input[aria-label="Nombre d\'occurrences"]';
    await until(`!!document.querySelector(${q(nField)})`, 'champ du nombre d\'occurrences');
    await setValue(nField, count);
  }
  if (reminder !== null) await setSelect('.dialog select[aria-label="Rappel"]', reminder);
  await saveDialog(`rendez-vous « ${subject} »`);
  const err = await statusError();
  if (err) throw new Error(`rendez-vous « ${subject} » : ${err}`);
}
/**
 * Supprime tous les rendez-vous « ZZ-L… » des `weeks` semaines à venir (série entière quand c'est une série).
 * Appelée en fin de cas, y compris après un échec : elle ne lève jamais.
 */
export async function sweepAppointments(weeks = 9) {
  try {
    await closeDialogs();
    await openCalendar();
    await setKind('Semaine');
    await weekAhead(0);
    for (let w = 0; w < weeks; w++) {
      for (let i = 0; i < 12 && (await chips(PREFIX)).length > 0; i++) {
        await selectChip(PREFIX, 0);
        const series = await js(`!!document.querySelector('.cal-ev.selected .cal-flag')`);
        await deleteSelected(series);
      }
      await nextPeriod();
    }
  } catch { /* nettoyage au mieux */ }
}
