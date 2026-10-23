// Cas L-recherche : la recherche « Tout » retrouve un contact créé par le cas. Écrit dans la base : exige THUNDERLOOK_DATA.
import { js, until, sleep, press, setValue, setSelect, key, requireTestData, tag } from './lib.mjs';
import { createContact, deleteContacts, openContacts } from './data.mjs';

export const name = 'L-recherche : taper le nom d\'un contact créé et le retrouver (portée « Tout »)';

export async function run(log) {
  if (!requireTestData(log)) return;
  const surname = tag('g');
  let scopeChanged = false;
  try {
    await openContacts();
    await createContact({ surname, company: 'ZZ Recherche' });
    log(`contact créé : ZZ ${surname}`);

    // Depuis le courrier : la portée « Tout » interroge aussi les contacts.
    await press('1', { ctrl: true });
    await until(`!!document.querySelector('.msg, .treeitem')`, 'module Courrier');
    await setSelect('select.gs-scope', 'all');
    scopeChanged = true;
    await setValue('input[aria-label="Rechercher"]', surname);
    await js(`document.querySelector('input[aria-label="Rechercher"]').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))`);
    await until(`!!document.querySelector('.gs-panel') && [...document.querySelectorAll('.gs-panel .gs-item .gs-title')].some(t => t.textContent.includes(${JSON.stringify(surname)}))`,
      `contact « ${surname} » dans le panneau de résultats`, 30000);
    const section = await js(`[...document.querySelectorAll('.gs-panel section')].find(s => s.textContent.includes(${JSON.stringify(surname)}))?.getAttribute('aria-label')`);
    log(`trouvé dans la section « ${section} »`);

    // Ouvrir le résultat : on arrive sur la fiche du contact.
    await js(`[...document.querySelectorAll('.gs-panel .gs-item')].find(b => b.textContent.includes(${JSON.stringify(surname)})).click()`);
    await until(`document.querySelector('.contact-card h2')?.textContent.includes(${JSON.stringify(surname)})`, 'fiche du contact ouverte depuis le résultat', 10000);
    log('clic sur le résultat : fiche du contact affichée');
  } finally {
    try {
      if (scopeChanged) await setSelect('select.gs-scope', 'module');
      await key('Escape');
      await js(`(() => { const i = document.querySelector('input[aria-label="Rechercher"]'); if (i) { i.value = ''; i.dispatchEvent(new Event('input', { bubbles: true })); } })()`);
    } catch { /* réglages d'interface seulement */ }
    await deleteContacts(surname).catch((e) => log(`NETTOYAGE contact « ${surname} » : ${e.message}`));
    await press('1', { ctrl: true });
    await sleep(200);
  }
}
