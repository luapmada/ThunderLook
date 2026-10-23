// Cas L-notes : créer (couleur verte), modifier, supprimer. Écrit dans la base : exige THUNDERLOOK_DATA.
import { js, until, sleep, setValue, setSelect, ribbon, openModule, closeDialogs, requireTestData, tag } from './lib.mjs';
import { saveDialog } from './data.mjs';

export const name = 'L-notes : créer (verte), modifier, supprimer';

const card = (token) => `[...document.querySelectorAll('.note-card')].find(c => c.querySelector('h3')?.textContent.includes(${JSON.stringify(token)}))`;
const openNotes = () => openModule(5, `document.querySelector('.pane.notes') && !/Chargement/.test(document.querySelector('.pane.notes').textContent)`);
const deleteAll = async (token) => {
  await closeDialogs();
  await openNotes();
  for (let i = 0; i < 20 && (await js(`!!${card(token)}`)); i++) {
    await js(`${card(token)}.click()`);
    await sleep(300);
    await ribbon('Supprimer');
    await sleep(800);
  }
  if (await js(`!!${card(token)}`)) throw new Error(`note « ${token} » encore présente après suppression`);
};

export async function run(log) {
  if (!requireTestData(log)) return;
  const title = tag('n');
  try {
    await openNotes();
    await ribbon('Nouvelle note');
    await until(`!!document.querySelector('.dialog textarea[aria-label="Texte de la note"]')`, 'boîte Nouvelle note');
    await setValue('.dialog textarea[aria-label="Texte de la note"]', `${title}\nZZ ligne 2`);
    await setSelect('.dialog select[aria-label="Couleur"]', '1');
    await saveDialog('note');
    await until(`!!${card(title)}`, 'note dans la grille', 8000);
    const bg = await js(`getComputedStyle(${card(title)}).backgroundColor`);
    if (bg !== 'rgb(168, 219, 168)') throw new Error(`couleur de la note : ${bg}, attendu le vert (168, 219, 168)`);
    log(`créée : « ${title} », fond ${bg} (verte)`);

    // Modifier le texte (double clic ouvre la note).
    await js(`${card(title)}.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))`);
    await until(`document.querySelector('.dialog textarea[aria-label="Texte de la note"]')?.value.startsWith(${JSON.stringify(title)})`, 'note rouverte');
    await setValue('.dialog textarea[aria-label="Texte de la note"]', `${title} mod\nZZ ligne 2 modifiée`);
    await saveDialog('modification de la note');
    await until(`!!${card(title)} && ${card(title)}.querySelector('h3').textContent.endsWith(' mod')`, 'titre modifié', 8000);
    if (!(await js(`${card(title)}.querySelector('p').textContent.includes('modifiée')`))) throw new Error('texte de la note non modifié');
    log('modifiée : titre et texte à jour');

    await deleteAll(title);
    log('supprimée : plus dans la grille');
  } finally {
    await deleteAll(title).catch((e) => log(`NETTOYAGE note « ${title} » : ${e.message}`));
  }
}
