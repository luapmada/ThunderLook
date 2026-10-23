// Cas L-contacts : créer, modifier (société), rouvrir, photo (si le champ fichier existe), supprimer.
// Écrit dans la base : exige THUNDERLOOK_DATA (copie de test). Tout ce qui est créé porte « ZZ-L… » et est supprimé à la fin.
import fs from 'node:fs';
import { js, until, sleep, setValue, setFile, mustClick, requireTestData, tag, tinyPng } from './lib.mjs';
import { openContacts, createContact, selectContact, deleteContacts, saveDialog } from './data.mjs';

export const name = 'L-contacts : créer, modifier (société), rouvrir, photo, supprimer';

export async function run(log) {
  if (!requireTestData(log)) return;
  const surname = tag('c');
  const png = tinyPng();
  try {
    await openContacts();
    const folder = await js(`document.querySelector('.contact-nav .cal-item.selected')?.textContent.trim()`);
    await createContact({ surname, company: 'ZZ Société A', email: 'zz-l@example.invalid' });
    log(`créé dans « ${folder} » : ZZ ${surname}`);

    await selectContact(surname);
    const card = await js(`document.querySelector('.contact-card')?.textContent ?? ''`);
    if (!card.includes('ZZ Société A')) throw new Error(`société « ZZ Société A » absente de la fiche : ${card.slice(0, 120)}`);
    if (!card.includes('zz-l@example.invalid')) throw new Error('courrier absent de la fiche');
    log('fiche : société et courrier présents');

    // Modifier la société, enregistrer.
    await mustClick('.contact-card', 'Modifier');
    await until(`document.querySelector('.dialog input[aria-label="Société"]')?.value === 'ZZ Société A'`, 'boîte Contact pré-remplie');
    await setValue('.dialog input[aria-label="Société"]', 'ZZ Société B');
    await saveDialog('modification du contact');
    await until(`(document.querySelector('.contact-card')?.textContent ?? '').includes('ZZ Société B')`, 'fiche mise à jour', 8000);
    log('société modifiée : A → B dans la fiche');

    // Rouvrir : la valeur enregistrée revient, puis Annuler ne change rien.
    await mustClick('.contact-card', 'Modifier');
    await until(`document.querySelector('.dialog input[aria-label="Société"]')?.value === 'ZZ Société B'`, 'réouverture avec la société B');
    await mustClick('.dialog .dialog-buttons', 'Annuler');
    await until(`!document.querySelector('.dialog')`, 'fermeture par Annuler');
    log('réouverture : société B conservée');

    // Photo, seulement si le champ fichier existe.
    await mustClick('.contact-card', 'Modifier');
    await until(`!!document.querySelector('.dialog')`, 'boîte Contact');
    if (await js(`!!document.querySelector('.dialog input[type=file]')`)) {
      await setFile('.dialog input[type=file]', png);
      await until(`!!document.querySelector('.dialog img.avatar.large')`, 'aperçu de la photo', 15000);
      await saveDialog('enregistrement de la photo');
      await until(`!!document.querySelector('.contact-card img.avatar.large')`, 'photo dans la fiche', 8000);
      log('photo : choisie, enregistrée, visible dans la fiche');
      await mustClick('.contact-card', 'Modifier');
      await until(`!!document.querySelector('.dialog')`, 'boîte Contact');
      await mustClick('.dialog', 'Retirer la photo');
      await saveDialog('retrait de la photo');
      await until(`!document.querySelector('.contact-card img.avatar.large')`, 'photo retirée de la fiche', 8000);
      log('photo : retirée');
    } else {
      log('photo : pas de champ fichier dans la boîte Contact (ignorée)');
      await mustClick('.dialog .dialog-buttons', 'Annuler');
    }

    // Suppression.
    await deleteContacts(surname);
    log('supprimé : plus dans la liste');
  } finally {
    await deleteContacts(surname).catch((e) => log(`NETTOYAGE contact « ${surname} » : ${e.message}`));
    fs.rmSync(png, { force: true });
    await sleep(200);
  }
}
