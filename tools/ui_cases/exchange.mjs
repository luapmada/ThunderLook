// Cas L-échange : l'écran « Échange avec le téléphone » s'ouvre (Ctrl+Maj+T ; l'onglet Fichier est encore grisé) et
// ses boutons sont là. Le choix de dossier passe par une boîte système, impossible à piloter : on ne clique sur rien
// qui exporte ou importe, donc ce cas n'écrit rien et n'exige pas de base de test.
import { js, until, press, sleep, closeDialogs } from './lib.mjs';

export const name = 'L-échange : écran du téléphone (boutons présents)';

export async function run(log) {
  try {
    await closeDialogs();
    const fichier = await js(`(() => { const t = document.querySelector('button.tab.file'); return t ? t.getAttribute('aria-disabled') : null; })()`);
    log(`onglet Fichier : ${fichier === null ? 'absent' : fichier === 'true' ? 'grisé (raccourci utilisé)' : 'actif'}`);
    await press('t', { ctrl: true, shift: true });
    await until(`document.querySelector('.dialog h2')?.textContent.trim() === 'Échange avec le téléphone'`, 'écran « Échange avec le téléphone »');
    const labels = await js(`[...document.querySelectorAll('.dialog button')].map(b => b.textContent.trim())`);
    const missing = ['Choisir le dossier…', 'Exporter maintenant', 'Vérifier les changements', 'Fermer'].filter((l) => !labels.includes(l));
    if (missing.length) throw new Error(`boutons absents : ${missing.join(', ')} (présents : ${labels.join(' | ')})`);
    log(`boutons présents : ${labels.join(' | ')}`);
    const disabled = await js(`[...document.querySelectorAll('.dialog button')].filter(b => b.disabled).map(b => b.textContent.trim())`);
    log(`grisés tant qu'aucun dossier n'est choisi : ${disabled.join(', ') || '(aucun : un dossier est déjà réglé)'}`);
    await js(`[...document.querySelectorAll('.dialog button')].find(b => b.textContent.trim() === 'Fermer').click()`);
    await until(`!document.querySelector('.dialog')`, 'fermeture de l\'écran');
  } finally {
    await closeDialogs();
    await sleep(100);
  }
}
