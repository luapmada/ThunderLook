// Cas L-tâches : créer avec échéance, terminer, filtre « En retard », supprimer. Écrit dans la base : exige THUNDERLOOK_DATA.
import { js, until, sleep, setValue, setSelect, mustClick, ribbon, openModule, closeDialogs, requireTestData, tag, ymd } from './lib.mjs';
import { saveDialog } from './data.mjs';

export const name = 'L-tâches : créer avec échéance, terminer, filtre « En retard », supprimer';

const row = (token) => `[...document.querySelectorAll('.task-row')].find(r => r.textContent.includes(${JSON.stringify(token)}))`;
const openTasks = async () => {
  await openModule(4, `!!document.querySelector('.task-rows') && !/Chargement/.test(document.querySelector('.task-rows').textContent)`);
  await setValue('.pane.tasks .contact-search input', '');
  await setSelect('.task-views select[aria-label="Afficher"]', 'all');
  await sleep(500);
};
const deleteAll = async (token) => {
  await closeDialogs();
  await openTasks();
  for (let i = 0; i < 20 && (await js(`!!${row(token)}`)); i++) {
    await js(`${row(token)}.click()`);
    await sleep(300);
    await ribbon('Supprimer');
    await sleep(800);
  }
  if (await js(`!!${row(token)}`)) throw new Error(`tâche « ${token} » encore présente après suppression`);
};

export async function run(log) {
  if (!requireTestData(log)) return;
  const subject = tag('t');
  try {
    await openTasks();
    await ribbon('Nouvelle tâche');
    await until(`!!document.querySelector('.dialog input[aria-label="Objet"]')`, 'boîte Nouvelle tâche');
    await setValue('.dialog input[aria-label="Objet"]', subject);
    await setValue('.dialog input[aria-label="Échéance"]', ymd(-2));
    await saveDialog('tâche');
    await until(`!!${row(subject)}`, 'tâche dans la liste', 8000);
    const due = await js(`${row(subject)}.querySelector('.task-due')?.textContent ?? ''`);
    if (!due) throw new Error('la tâche créée n\'affiche pas d\'échéance');
    log(`créée avec échéance ${due} (il y a 2 jours)`);

    // En retard : la tâche (non terminée, échue) y figure.
    await setSelect('.task-views select[aria-label="Afficher"]', 'overdue');
    await until(`!!${row(subject)}`, 'tâche dans le filtre « En retard »', 8000);
    if (!(await js(`${row(subject)}.querySelector('.task-due.overdue') !== null`))) throw new Error('échéance non marquée en retard');
    log('filtre « En retard » : présente, échéance marquée en retard');

    // Terminer : elle quitte « En retard » et apparaît dans « Terminées ».
    await js(`${row(subject)}.querySelector('input[type=checkbox]').click()`);
    await until(`!${row(subject)}`, 'tâche terminée absente de « En retard »', 8000);
    log('terminée : disparue de « En retard »');
    await setSelect('.task-views select[aria-label="Afficher"]', 'done');
    await until(`!!${row(subject)} && ${row(subject)}.classList.contains('done')`, 'tâche dans « Terminées »', 8000);
    log('filtre « Terminées » : présente, barrée');

    await deleteAll(subject);
    log('supprimée : plus dans la liste');
  } finally {
    await deleteAll(subject).catch((e) => log(`NETTOYAGE tâche « ${subject} » : ${e.message}`));
  }
}
