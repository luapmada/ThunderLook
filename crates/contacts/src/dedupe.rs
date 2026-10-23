//! Doublons de contacts : détection, fusion champ par champ, application. Sans lecture ni écriture hors de `apply_merge`
//! (qui passe par `write::update` puis `write::delete`) ; la liste « Ce ne sont pas des doublons » est tenue par l'appelant.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::Path,
};

use thunderlook_model::contacts::ContactForm;
use thunderlook_store::rusqlite::Connection;

use crate::{Contact, Result, load, write};

/// Séparateur entre deux jeux de notes (et avant les informations reportées).
pub const SEPARATOR: &str = "----------";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Même adresse électronique (normalisée).
    SameEmail(String),
    /// Même nom (casse, accents et ordre prénom/nom ignorés) et même mobile.
    SameNameAndMobile(String),
}

impl Reason {
    pub fn label(&self) -> String {
        match self {
            Self::SameEmail(a) => format!("Même adresse électronique : {a}"),
            Self::SameNameAndMobile(m) => format!("Même nom et même mobile : {m}"),
        }
    }
}

/// Contacts qui désignent probablement la même personne. Les membres se ressemblent tous deux à deux.
#[derive(Clone, Debug)]
pub struct Group {
    /// Identifiants des contacts (messages), croissants.
    pub ids: Vec<i64>,
    pub reason: Reason,
    /// 100 : même courriel et même nom ; 85 : même nom et même mobile ; 70 : même courriel, noms différents.
    pub score: u8,
    /// Même courriel ET même nom pour toutes les paires : fusion groupée possible.
    pub safe: bool,
    /// Le groupe n'est qu'une partie d'une chaîne A~B, B~C sans A~C : à regarder à la main.
    pub chain: Option<String>,
}

/// Adresse normalisée : espaces et « mailto: » retirés, minuscules.
pub fn norm_email(s: &str) -> String {
    let s = s.trim();
    let s = s.get(..7).filter(|p| p.eq_ignore_ascii_case("mailto:")).map_or(s, |_| &s[7..]);
    s.trim().to_lowercase()
}

/// Lettre sans accent (minuscule) ; `None` pour ce qui n'est ni lettre ni chiffre.
fn fold(c: char, out: &mut String) {
    let lower: Vec<char> = c.to_lowercase().collect();
    for c in lower {
        match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => out.push('a'),
            'ç' | 'ć' | 'č' => out.push('c'),
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => out.push('e'),
            'ì' | 'í' | 'î' | 'ï' | 'ī' => out.push('i'),
            'ñ' | 'ń' => out.push('n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => out.push('o'),
            'ù' | 'ú' | 'û' | 'ü' | 'ū' => out.push('u'),
            'ý' | 'ÿ' => out.push('y'),
            'š' | 'ś' => out.push('s'),
            'ž' | 'ź' | 'ż' => out.push('z'),
            'ł' => out.push('l'),
            'œ' => out.push_str("oe"),
            'æ' => out.push_str("ae"),
            'ß' => out.push_str("ss"),
            c if c.is_alphanumeric() => out.push(c),
            _ => out.push(' '),
        }
    }
}

/// Clé de nom : mots sans accent ni casse, triés (« Dupont, Jean » = « jean DUPONT »). Vide si aucun nom de personne.
pub fn name_key(given: &str, surname: &str, display: &str) -> String {
    let mut s = String::new();
    for part in [given, surname] {
        fold_str(part, &mut s);
        s.push(' ');
    }
    if s.trim().is_empty() {
        fold_str(display, &mut s);
    }
    let mut words: Vec<&str> = s.split_whitespace().collect();
    words.sort_unstable();
    words.join(" ")
}

fn fold_str(s: &str, out: &mut String) {
    for c in s.chars() {
        fold(c, out);
    }
}

/// Numéro réduit à ses chiffres, « +33 6… », « 0033 6… » et « 06… » étant équivalents. Vide si moins de 6 chiffres.
pub fn norm_phone(s: &str) -> String {
    let plus = s.trim_start().starts_with('+');
    let d: String = s.chars().filter(char::is_ascii_digit).collect();
    let national = if let Some(r) = d.strip_prefix("0033") {
        Some(r)
    } else if plus || d.len() == 11 {
        d.strip_prefix("33")
    } else {
        None
    };
    let d = match national {
        Some(r) if r.len() == 9 => format!("0{r}"),
        _ => d,
    };
    if d.len() < 6 { String::new() } else { d }
}

fn mobile(c: &Contact) -> String {
    c.phones.iter().find(|(l, _)| *l == "Mobile").map(|(_, v)| norm_phone(v)).unwrap_or_default()
}

fn emails(c: &Contact) -> BTreeSet<String> {
    c.emails.iter().map(|e| norm_email(&e.address)).filter(|e| e.contains('@')).collect()
}

/// Lien entre deux contacts (indices dans la liste) : raison, score, « sûr ».
struct Link {
    reason: Reason,
    score: u8,
    safe: bool,
}

/// Regroupe les contacts qui désignent probablement la même personne : même adresse électronique, sinon même nom ET même
/// mobile. Pas de transitivité abusive : si A~B et B~C sans A~C, les paires sont des groupes séparés, signalés « chaîne ».
pub fn find_duplicates(contacts: &[Contact]) -> Vec<Group> {
    let names: Vec<String> = contacts.iter().map(|c| name_key(&c.given_name, &c.surname, &c.display_name)).collect();
    let mails: Vec<BTreeSet<String>> = contacts.iter().map(emails).collect();
    let mobiles: Vec<String> = contacts.iter().map(mobile).collect();

    // Candidats par seaux : on ne compare que ce qui partage une adresse, ou un nom.
    let mut pairs: HashSet<(usize, usize)> = HashSet::new();
    let mut by_mail: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for i in 0..contacts.len() {
        for m in &mails[i] {
            by_mail.entry(m).or_default().push(i);
        }
        if !names[i].is_empty() {
            by_name.entry(&names[i]).or_default().push(i);
        }
    }
    for bucket in by_mail.values().chain(by_name.values()) {
        for (k, &a) in bucket.iter().enumerate() {
            for &b in &bucket[k + 1..] {
                pairs.insert((a.min(b), a.max(b)));
            }
        }
    }

    let mut links: HashMap<(usize, usize), Link> = HashMap::new();
    for (a, b) in pairs {
        let same_name = !names[a].is_empty() && names[a] == names[b];
        let shared = mails[a].intersection(&mails[b]).next();
        let link = if let Some(m) = shared {
            Link { reason: Reason::SameEmail(m.clone()), score: if same_name { 100 } else { 70 }, safe: same_name }
        } else if same_name && !mobiles[a].is_empty() && mobiles[a] == mobiles[b] {
            Link { reason: Reason::SameNameAndMobile(mobiles[a].clone()), score: 85, safe: false }
        } else {
            continue;
        };
        links.insert((a, b), link);
    }

    // Composantes connexes ; une composante qui est une clique donne un groupe, sinon des cliques maximales (gloutonnes).
    let mut adj: HashMap<usize, BTreeSet<usize>> = HashMap::new();
    for &(a, b) in links.keys() {
        adj.entry(a).or_default().insert(b);
        adj.entry(b).or_default().insert(a);
    }
    let mut seen: HashSet<usize> = HashSet::new();
    let mut starts: Vec<usize> = adj.keys().copied().collect();
    starts.sort_unstable();
    let mut groups = Vec::new();
    for s in starts {
        if !seen.insert(s) {
            continue;
        }
        let mut comp = vec![s];
        let mut i = 0;
        while i < comp.len() {
            for &n in &adj[&comp[i]] {
                if seen.insert(n) {
                    comp.push(n);
                }
            }
            i += 1;
        }
        comp.sort_unstable();
        let linked = |a: usize, b: usize| links.contains_key(&(a.min(b), a.max(b)));
        let clique = comp.iter().all(|&a| comp.iter().all(|&b| a == b || linked(a, b)));
        let parts: Vec<Vec<usize>> = if clique { vec![comp.clone()] } else { cliques(&comp, &linked) };
        for part in parts {
            let pairs: Vec<&Link> = part.iter().enumerate().flat_map(|(k, &a)| part[k + 1..].iter().map(move |&b| (a, b))).filter_map(|(a, b)| links.get(&(a, b))).collect();
            let best = pairs.iter().max_by_key(|l| l.score).expect("une clique a au moins une paire");
            let chain = (!clique).then(|| {
                let others: Vec<String> = comp.iter().filter(|x| !part.contains(x)).map(|&x| format!("p{}", contacts[x].id)).collect();
                format!("Chaîne : ces fiches sont aussi liées à {} sans l'être toutes entre elles.", others.join(", "))
            });
            groups.push(Group {
                ids: part.iter().map(|&x| contacts[x].id).collect(),
                reason: best.reason.clone(),
                score: pairs.iter().map(|l| l.score).min().unwrap_or(0),
                safe: chain.is_none() && pairs.iter().all(|l| l.safe),
                chain,
            });
        }
    }
    groups.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.ids.cmp(&b.ids)));
    groups
}

/// Cliques maximales (gloutonnes) qui couvrent chaque lien d'une composante qui n'est pas une clique.
fn cliques(comp: &[usize], linked: &dyn Fn(usize, usize) -> bool) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = Vec::new();
    let covered = |out: &[Vec<usize>], a: usize, b: usize| out.iter().any(|c| c.contains(&a) && c.contains(&b));
    for (k, &a) in comp.iter().enumerate() {
        for &b in &comp[k + 1..] {
            if !linked(a, b) || covered(&out, a, b) {
                continue;
            }
            let mut c = vec![a, b];
            for &x in comp {
                if !c.contains(&x) && c.iter().all(|&y| linked(x, y)) {
                    c.push(x);
                }
            }
            c.sort_unstable();
            out.push(c);
        }
    }
    out
}

/// Retire les groupes dont toutes les paires ont été déclarées « pas des doublons ». `ignored` : paires `(a, b)` avec `a < b`.
pub fn without_ignored(groups: Vec<Group>, ignored: &HashSet<(i64, i64)>) -> Vec<Group> {
    groups.into_iter().filter(|g| !all_pairs(&g.ids).iter().all(|p| ignored.contains(p))).collect()
}

/// Toutes les paires `(a, b)`, `a < b`, d'un groupe (ce que « Ce ne sont pas des doublons » mémorise).
pub fn all_pairs(ids: &[i64]) -> Vec<(i64, i64)> {
    let mut v = Vec::new();
    for (k, &a) in ids.iter().enumerate() {
        for &b in &ids[k + 1..] {
            v.push((a.min(b), a.max(b)));
        }
    }
    v
}

/// Champs dont la fusion se choisit contact par contact ; les adresses postales forment chacune un bloc.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    GivenName,
    Surname,
    Company,
    JobTitle,
    Department,
    WorkAddress,
    HomeAddress,
    WebPage,
    Birthday,
    Photo,
}

pub const FIELDS: [Field; 10] = [
    Field::GivenName,
    Field::Surname,
    Field::Company,
    Field::JobTitle,
    Field::Department,
    Field::WorkAddress,
    Field::HomeAddress,
    Field::WebPage,
    Field::Birthday,
    Field::Photo,
];

impl Field {
    pub fn key(self) -> &'static str {
        match self {
            Self::GivenName => "given_name",
            Self::Surname => "surname",
            Self::Company => "company",
            Self::JobTitle => "job_title",
            Self::Department => "department",
            Self::WorkAddress => "work_address",
            Self::HomeAddress => "home_address",
            Self::WebPage => "web_page",
            Self::Birthday => "birthday",
            Self::Photo => "photo",
        }
    }

    pub fn from_key(k: &str) -> Option<Self> {
        FIELDS.into_iter().find(|f| f.key() == k)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::GivenName => "Prénom",
            Self::Surname => "Nom",
            Self::Company => "Société",
            Self::JobTitle => "Fonction",
            Self::Department => "Service",
            Self::WorkAddress => "Adresse professionnelle",
            Self::HomeAddress => "Adresse personnelle",
            Self::WebPage => "Page web",
            Self::Birthday => "Anniversaire",
            Self::Photo => "Photo",
        }
    }

    /// Valeur du champ dans un formulaire, sur une ligne (les adresses sont réunies) ; vide si absente.
    pub fn value(self, f: &ContactForm) -> String {
        let join = |p: [&str; 4]| p.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" | ");
        match self {
            Self::GivenName => f.given_name.trim().into(),
            Self::Surname => f.surname.trim().into(),
            Self::Company => f.company.trim().into(),
            Self::JobTitle => f.job_title.trim().into(),
            Self::Department => f.department.trim().into(),
            Self::WorkAddress => join([&f.work_street, &f.work_postal, &f.work_city, &f.work_country]),
            Self::HomeAddress => join([&f.home_street, &f.home_postal, &f.home_city, &f.home_country]),
            Self::WebPage => f.web_page.trim().into(),
            Self::Birthday => f.birthday.trim().into(),
            Self::Photo => f.photo.clone().unwrap_or_default(),
        }
    }

    fn copy(self, from: &ContactForm, to: &mut ContactForm) {
        match self {
            Self::GivenName => to.given_name = from.given_name.clone(),
            Self::Surname => to.surname = from.surname.clone(),
            Self::Company => to.company = from.company.clone(),
            Self::JobTitle => to.job_title = from.job_title.clone(),
            Self::Department => to.department = from.department.clone(),
            Self::WorkAddress => {
                to.work_street = from.work_street.clone();
                to.work_postal = from.work_postal.clone();
                to.work_city = from.work_city.clone();
                to.work_country = from.work_country.clone();
            }
            Self::HomeAddress => {
                to.home_street = from.home_street.clone();
                to.home_postal = from.home_postal.clone();
                to.home_city = from.home_city.clone();
                to.home_country = from.home_country.clone();
            }
            Self::WebPage => to.web_page = from.web_page.clone(),
            Self::Birthday => to.birthday = from.birthday.clone(),
            Self::Photo => to.photo = from.photo.clone(),
        }
    }
}

/// Un contact du groupe tel que la fusion le voit : son formulaire, plus ce que le formulaire ne porte pas.
#[derive(Clone, Debug, Default)]
pub struct Candidate {
    pub id: i64,
    pub form: ContactForm,
    /// Téléphones hors Mobile, Bureau et Domicile (« Autre », « Société »…) : libellé, numéro.
    pub other_phones: Vec<(String, String)>,
    /// Autres informations (nom d'usage, deuxième prénom, adresse « autre », messagerie…) : libellé, valeur.
    pub extra: Vec<(String, String)>,
}

impl Candidate {
    pub fn from_contact(c: &Contact, data_dir: &Path) -> Self {
        let other_phones = c.phones.iter().filter(|(l, _)| !["Mobile", "Bureau", "Domicile"].contains(l)).map(|(l, v)| (l.to_string(), v.clone())).collect();
        let mut extra: Vec<(String, String)> = Vec::new();
        let mut add = |label: &str, v: &str| {
            if !v.trim().is_empty() {
                extra.push((label.into(), v.trim().into()));
            }
        };
        add("Titre", &c.prefix);
        add("Deuxième prénom", &c.middle_name);
        add("Suffixe", &c.suffix);
        add("Surnom", &c.nickname);
        add("Bureau", &c.office);
        add("Messagerie instantanée", &c.im_address);
        add("Conjoint(e)", &c.spouse);
        add("Assistant(e)", &c.assistant);
        add("Responsable", &c.manager);
        if !c.other.is_empty() {
            add("Autre adresse", &c.other.lines().join(", "));
        }
        if let Some(a) = c.anniversary_ms {
            let (y, m, d) = crate::day(a);
            add("Anniversaire de mariage", &format!("{d:02}/{m:02}/{y:04}"));
        }
        Self { id: c.id, form: write::form_from(c, data_dir), other_phones, extra }
    }
}

/// Valeurs choisies, champ par champ : l'identifiant du contact dont on reprend la valeur.
pub type Choices = HashMap<Field, i64>;

/// Contact « le plus complet » pour un champ : valeur la plus longue ; à égalité, le conservé puis l'ordre du groupe.
pub fn default_source(field: Field, cands: &[Candidate], keep_id: i64) -> Option<i64> {
    let len = |c: &Candidate| field.value(&c.form).chars().count();
    let mut best: Option<(&Candidate, usize)> = None;
    for c in cands.iter().filter(|c| len(c) > 0) {
        let l = len(c);
        let better = match best {
            None => true,
            Some((b, bl)) => l > bl || (l == bl && c.id == keep_id && b.id != keep_id),
        };
        if better {
            best = Some((c, l));
        }
    }
    best.map(|(c, _)| c.id)
}

fn contains_ci(list: &[String], v: &str) -> bool {
    list.iter().any(|x| x.eq_ignore_ascii_case(v))
}

/// Formulaire du contact conservé après fusion : champs choisis (par défaut les plus complets), courriels et téléphones
/// distincts ajoutés dans les emplacements libres, catégories réunies, notes concaténées après une ligne de séparation.
/// Ce qui ne tient pas dans les emplacements du formulaire passe à la fin des notes : rien n'est perdu.
pub fn merge(cands: &[Candidate], keep_id: i64, choices: &Choices) -> Result<ContactForm> {
    let keep = cands.iter().find(|c| c.id == keep_id).ok_or("le contact à conserver n'est pas dans le groupe")?;
    let others: Vec<&Candidate> = cands.iter().filter(|c| c.id != keep_id).collect();
    let mut out = keep.form.clone();

    for field in FIELDS {
        let src = choices.get(&field).copied().or_else(|| default_source(field, cands, keep_id));
        if let Some(c) = src.and_then(|id| cands.iter().find(|c| c.id == id)) {
            field.copy(&c.form, &mut out);
        }
    }
    // Sans choix de photo et sans photo nulle part : la photo existante est conservée (`None`).
    if out.photo.as_deref() == Some("") {
        out.photo = None;
    }

    // Reports qui ne tiennent pas dans le formulaire.
    let mut carried: Vec<String> = Vec::new();

    // Courriels : ceux du conservé gardent leur place, les autres complètent.
    let mut mails: Vec<String> = [&keep.form.email1, &keep.form.email2, &keep.form.email3].iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let mut spare_mails: Vec<String> = Vec::new();
    for c in &others {
        for m in [&c.form.email1, &c.form.email2, &c.form.email3] {
            let m = m.trim();
            if !m.is_empty() && !mails.iter().any(|x| norm_email(x) == norm_email(m)) && !spare_mails.iter().any(|x| norm_email(x) == norm_email(m)) {
                if mails.len() < 3 {
                    mails.push(m.to_string());
                } else {
                    spare_mails.push(m.to_string());
                }
            }
        }
    }
    mails.resize(3, String::new());
    (out.email1, out.email2, out.email3) = (mails[0].clone(), mails[1].clone(), mails[2].clone());
    if !spare_mails.is_empty() {
        carried.push(format!("Autres adresses électroniques : {}", spare_mails.join(", ")));
    }

    // Téléphones : un numéro va dans l'emplacement de son libellé s'il est libre, sinon dans un emplacement libre.
    let mut slots: [(&str, String); 3] = [("Mobile", keep.form.mobile.trim().into()), ("Bureau", keep.form.business.trim().into()), ("Domicile", keep.form.home_phone.trim().into())];
    let mut known: Vec<String> = slots.iter().map(|(_, v)| norm_phone(v)).chain(keep.other_phones.iter().map(|(_, v)| norm_phone(v))).filter(|v| !v.is_empty()).collect();
    let mut spare_phones: Vec<String> = Vec::new();
    for c in &others {
        let own = [("Mobile", &c.form.mobile), ("Bureau", &c.form.business), ("Domicile", &c.form.home_phone)];
        let all = own.iter().map(|(l, v)| (l.to_string(), v.trim().to_string())).chain(c.other_phones.iter().map(|(l, v)| (l.clone(), v.trim().to_string())));
        for (label, v) in all {
            let n = norm_phone(&v);
            if v.is_empty() || (!n.is_empty() && known.contains(&n)) {
                continue;
            }
            known.push(n);
            let free = slots.iter().position(|(l, s)| *l == label && s.is_empty()).or_else(|| slots.iter().position(|(_, s)| s.is_empty()));
            if let Some(i) = free {
                slots[i].1 = v;
            } else {
                spare_phones.push(format!("{label} : {v}"));
            }
        }
    }
    (out.mobile, out.business, out.home_phone) = (slots[0].1.clone(), slots[1].1.clone(), slots[2].1.clone());
    if !spare_phones.is_empty() {
        carried.push(format!("Autres numéros : {}", spare_phones.join(" ; ")));
    }

    // Informations que le formulaire ne porte pas et que le conservé n'a pas.
    for c in &others {
        let lost: Vec<String> = c
            .extra
            .iter()
            .filter(|(l, v)| !keep.extra.iter().any(|(kl, kv)| kl == l && kv.eq_ignore_ascii_case(v)))
            .map(|(l, v)| format!("{l} : {v}"))
            .collect();
        if !lost.is_empty() {
            carried.push(format!("Reporté de la fiche fusionnée « {} » : {}", display(&c.form), lost.join(" ; ")));
        }
    }
    // Valeurs de champs choisis, perdues quand elles diffèrent de celle retenue : on ne les jette pas non plus.
    for field in FIELDS.into_iter().filter(|f| *f != Field::Photo) {
        let kept = field.value(&out);
        let mut lost: Vec<String> = Vec::new();
        for c in cands {
            let v = field.value(&c.form);
            if !v.is_empty() && !v.eq_ignore_ascii_case(&kept) && !contains_ci(&lost, &v) {
                lost.push(v);
            }
        }
        if !lost.is_empty() {
            carried.push(format!("{} (autre valeur) : {}", field.label(), lost.join(" ; ")));
        }
    }

    // Catégories réunies.
    let mut cats: Vec<String> = Vec::new();
    for c in cands {
        for k in c.form.categories.split(',').map(str::trim).filter(|k| !k.is_empty()) {
            if !contains_ci(&cats, k) {
                cats.push(k.to_string());
            }
        }
    }
    out.categories = cats.join(", ");

    // Notes : celles du conservé, puis celles des autres (sauf si déjà présentes), puis les reports.
    let mut blocks: Vec<String> = Vec::new();
    for c in std::iter::once(keep).chain(others.iter().copied()) {
        let n = c.form.notes.trim();
        if !n.is_empty() && !blocks.iter().any(|b| b.contains(n)) {
            blocks.push(n.to_string());
        }
    }
    if !carried.is_empty() {
        blocks.push(carried.join("\n"));
    }
    out.notes = blocks.join(&format!("\n{SEPARATOR}\n"));
    Ok(out)
}

fn display(f: &ContactForm) -> String {
    [f.given_name.trim(), f.surname.trim()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ")
}

/// Contact à conserver par défaut : celui dont la fiche est la plus riche (champs, courriels, téléphones, notes) ; à égalité, le plus ancien.
pub fn default_keep(cands: &[Candidate]) -> Option<i64> {
    let weight = |c: &Candidate| -> usize {
        let f = &c.form;
        let text = [&f.given_name, &f.surname, &f.company, &f.job_title, &f.department, &f.email1, &f.email2, &f.email3, &f.mobile, &f.business, &f.home_phone, &f.web_page, &f.birthday, &f.notes];
        text.iter().map(|s| s.trim().chars().count()).sum::<usize>() + FIELDS.iter().map(|k| k.value(f).chars().count()).sum::<usize>() + c.other_phones.len() * 10 + c.extra.len() * 5
    };
    cands.iter().max_by(|a, b| weight(a).cmp(&weight(b)).then(b.id.cmp(&a.id))).map(|c| c.id)
}

/// Charge les membres d'un groupe sous forme de candidats.
pub fn candidates(conn: &Connection, data_dir: &Path, ids: &[i64]) -> Result<Vec<Candidate>> {
    ids.iter()
        .map(|&id| {
            let all = load(conn, None, Some(id))?;
            Ok(Candidate::from_contact(all.contacts.first().ok_or_else(|| format!("contact p{id} introuvable"))?, data_dir))
        })
        .collect()
}

/// Résultat d'une fusion.
#[derive(Clone, Debug)]
pub struct Merged {
    pub kept: i64,
    pub deleted: Vec<i64>,
}

/// Écrit `form` dans le contact conservé, puis supprime les autres. Si l'écriture échoue, rien n'est supprimé.
pub fn commit(conn: &mut Connection, data_dir: &Path, keep_id: i64, form: &ContactForm, delete: &[i64]) -> Result<Merged> {
    thunderlook_safety::snapshot_edit(conn, data_dir, keep_id)?;
    let form = ContactForm { id: Some(format!("p{keep_id}")), ..form.clone() };
    write::update(conn, data_dir, keep_id, &form)?;
    let mut deleted = Vec::new();
    for &id in delete.iter().filter(|&&id| id != keep_id) {
        write::delete(conn, id).map_err(|e| format!("fusion incomplète (supprimés : {deleted:?}) : {e}"))?;
        deleted.push(id);
    }
    Ok(Merged { kept: keep_id, deleted })
}

/// Fusionne un groupe : fiche fusionnée écrite dans `keep_id`, puis les autres membres supprimés.
pub fn apply_merge(conn: &mut Connection, data_dir: &Path, ids: &[i64], keep_id: i64, choices: &Choices) -> Result<Merged> {
    let cands = candidates(conn, data_dir, ids)?;
    let form = merge(&cands, keep_id, choices)?;
    commit(conn, data_dir, keep_id, &form, ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Email;

    fn c(id: i64, given: &str, surname: &str, mails: &[&str], mobile: &str) -> Contact {
        Contact {
            id,
            given_name: given.into(),
            surname: surname.into(),
            display_name: format!("{given} {surname}"),
            emails: mails.iter().map(|m| Email { address: (*m).into(), display: String::new() }).collect(),
            phones: if mobile.is_empty() { vec![] } else { vec![("Mobile", mobile.into())] },
            ..Default::default()
        }
    }

    fn ids(g: &[Group]) -> Vec<Vec<i64>> {
        g.iter().map(|g| g.ids.clone()).collect()
    }

    #[test]
    fn courriel_en_majuscules_et_espaces() {
        let g = find_duplicates(&[c(1, "Jean", "Dupont", &["Jean.Dupont@Exemple.FR"], ""), c(2, "J.", "Dupont", &[" mailto:jean.dupont@exemple.fr "], ""), c(3, "Autre", "Personne", &["x@y.fr"], "")]);
        assert_eq!(ids(&g), [[1, 2]]);
        assert_eq!(g[0].reason, Reason::SameEmail("jean.dupont@exemple.fr".into()));
        assert!(!g[0].safe && g[0].score == 70, "noms différents : pas sûr");
    }

    #[test]
    fn nom_inverse_accents_et_mobile_33() {
        let g = find_duplicates(&[c(1, "Hélène", "Müller-Éluard", &[], "+33 6 12 34 56 78"), c(2, "MULLER-ELUARD", "helene", &[], "06.12.34.56.78"), c(3, "Hélène", "Muller-Eluard", &[], "06 99 99 99 99")]);
        assert_eq!(ids(&g), [[1, 2]]);
        assert_eq!(g[0].reason, Reason::SameNameAndMobile("0612345678".into()));
        // Même nom sans même mobile, ou même mobile sans même nom : rien.
        assert!(find_duplicates(&[c(1, "Léa", "Martin", &[], "0611111111"), c(2, "Paul", "Martin", &[], "0611111111"), c(3, "Léa", "Martin", &[], "")]).is_empty());
    }

    #[test]
    fn numeros_equivalents() {
        for n in ["+33 6 12 34 56 78", "0033612345678", "06 12 34 56 78", "+33(0)6…"] {
            if n.contains("(0)") {
                continue;
            }
            assert_eq!(norm_phone(n), "0612345678", "{n}");
        }
        assert_eq!(norm_phone("+44 20 7946 0958"), "442079460958");
        assert_eq!(norm_phone("12"), "");
    }

    #[test]
    fn groupes_non_transitifs_separes_et_signales() {
        // A~B (courriel), B~C (courriel), A et C n'ont rien en commun.
        let g = find_duplicates(&[c(1, "Ana", "Roy", &["a@x.fr", "ab@x.fr"], ""), c(2, "Ana", "Roy", &["ab@x.fr", "bc@x.fr"], ""), c(3, "Anne", "Roy", &["bc@x.fr"], "")]);
        assert_eq!(ids(&g), [vec![1, 2], vec![2, 3]]);
        assert!(g.iter().all(|g| g.chain.is_some() && !g.safe));
        assert!(g[0].chain.as_deref().unwrap().contains("p3"));
        // Une vraie clique de trois reste un seul groupe, sûr.
        let g = find_duplicates(&[c(1, "Ana", "Roy", &["a@x.fr"], ""), c(2, "Roy", "Ana", &["A@x.fr"], ""), c(3, "ana", "roy", &["a@x.fr"], "")]);
        assert_eq!(ids(&g), [[1, 2, 3]]);
        assert!(g[0].safe && g[0].chain.is_none() && g[0].score == 100);
    }

    #[test]
    fn paires_ignorees() {
        let g = find_duplicates(&[c(1, "Ana", "Roy", &["a@x.fr"], ""), c(2, "Ana", "Roy", &["a@x.fr"], ""), c(3, "Ana", "Roy", &["a@x.fr"], "")]);
        let mut ignored = HashSet::new();
        ignored.insert((1, 2));
        assert_eq!(without_ignored(g.clone(), &ignored).len(), 1);
        ignored.extend(all_pairs(&[1, 2, 3]));
        assert!(without_ignored(g, &ignored).is_empty());
    }

    fn cand(id: i64, f: ContactForm) -> Candidate {
        Candidate { id, form: f, ..Default::default() }
    }

    #[test]
    fn fusion_sans_perte() {
        let a = ContactForm { given_name: "Jean".into(), surname: "Dupont".into(), email1: "jean@a.fr".into(), mobile: "0611111111".into(), notes: "Note A".into(), categories: "Clients".into(), ..Default::default() };
        let b = ContactForm {
            given_name: "Jean-Pierre".into(),
            surname: "Dupont".into(),
            company: "ADI".into(),
            email1: "JEAN@a.fr".into(),
            email2: "jp@b.fr".into(),
            mobile: "+33 6 11 11 11 11".into(),
            business: "0545000000".into(),
            work_city: "Fenioux".into(),
            notes: "Note B".into(),
            categories: "clients, VIP".into(),
            ..Default::default()
        };
        let mut cb = cand(2, b);
        cb.other_phones = vec![("Autre".into(), "0555555555".into())];
        cb.extra = vec![("Surnom".into(), "JP".into())];
        let m = merge(&[cand(1, a), cb], 1, &Choices::new()).unwrap();
        // Le plus complet : « Jean-Pierre », société, ville ; courriel identique (casse) non dupliqué ; mobile identique (+33) non dupliqué.
        assert_eq!((m.given_name.as_str(), m.company.as_str(), m.work_city.as_str()), ("Jean-Pierre", "ADI", "Fenioux"));
        assert_eq!((m.email1.as_str(), m.email2.as_str(), m.email3.as_str()), ("jean@a.fr", "jp@b.fr", ""));
        assert_eq!((m.mobile.as_str(), m.business.as_str(), m.home_phone.as_str()), ("0611111111", "0545000000", "0555555555"));
        assert_eq!(m.categories, "Clients, VIP");
        // Notes concaténées avec séparateur ; « Jean » (valeur écartée) et le surnom sont reportés.
        let parts: Vec<&str> = m.notes.split(&format!("\n{SEPARATOR}\n")).collect();
        assert_eq!(&parts[..2], ["Note A", "Note B"]);
        assert!(parts[2].contains("Surnom : JP") && parts[2].contains("Prénom (autre valeur) : Jean"), "{}", m.notes);
    }

    #[test]
    fn conserve_par_defaut_le_plus_riche() {
        let a = cand(5, ContactForm { surname: "Roy".into(), ..Default::default() });
        let b = cand(7, ContactForm { surname: "Roy".into(), company: "ADI".into(), ..Default::default() });
        let c = cand(3, ContactForm { surname: "Roy".into(), company: "ADI".into(), ..Default::default() });
        assert_eq!(default_keep(&[a.clone(), b.clone()]), Some(7));
        assert_eq!(default_keep(&[a, b, c]), Some(3));
    }

    #[test]
    fn choix_explicite_et_debordement() {
        let a = ContactForm { given_name: "Jean".into(), email1: "a@x.fr".into(), email2: "b@x.fr".into(), email3: "c@x.fr".into(), mobile: "0611111111".into(), business: "0522222222".into(), home_phone: "0533333333".into(), ..Default::default() };
        let b = ContactForm { given_name: "Jean-Pierre".into(), email1: "d@x.fr".into(), business: "0544444444".into(), ..Default::default() };
        let choices: Choices = [(Field::GivenName, 1)].into();
        let m = merge(&[cand(1, a), cand(2, b)], 1, &choices).unwrap();
        assert_eq!(m.given_name, "Jean");
        assert!(m.notes.contains("Autres adresses électroniques : d@x.fr") && m.notes.contains("Autres numéros : Bureau : 0544444444"), "{}", m.notes);
        assert!(merge(&[], 9, &Choices::new()).is_err());
    }

    fn fixture(name: &str) -> (std::path::PathBuf, Connection) {
        let dir = std::env::temp_dir().join(format!("tl-dedupe-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conn = thunderlook_store::open(&dir).unwrap();
        // Table de noms avec les trois emplacements de courriel (sans elle, `write` ne saurait pas les écrire).
        let mut names = Vec::new();
        for (k, lid) in [0x8080u32, 0x8082, 0x8083, 0x8084, 0x8090, 0x8092, 0x8093, 0x8094, 0x80A0, 0x80A2, 0x80A3, 0x80A4].into_iter().enumerate() {
            names.extend_from_slice(&(0x8000 + k as u16).to_le_bytes());
            names.extend_from_slice(crate::PSETID_ADDRESS.as_bytes());
            names.push(0);
            names.extend_from_slice(&lid.to_le_bytes());
        }
        conn.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at, props) VALUES (1, 'tl', '', '', 't', 0, 0, ?1)", [names]).unwrap();
        conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position) VALUES (1, 1, 1, 'Contacts', 'folder', 0, 0)", []).unwrap();
        (dir, conn)
    }

    #[test]
    fn apply_merge_ecrit_puis_supprime() {
        let (dir, mut conn) = fixture("apply");
        let a = write::create(&mut conn, &dir, 1, &ContactForm { given_name: "Léa".into(), surname: "Martin".into(), email1: "lea@x.fr".into(), notes: "A".into(), ..Default::default() }).unwrap();
        let b = write::create(&mut conn, &dir, 1, &ContactForm { given_name: "Martin".into(), surname: "Léa".into(), email1: "LEA@x.fr".into(), mobile: "+33 6 12 34 56 78".into(), company: "ADI".into(), notes: "B".into(), ..Default::default() }).unwrap();
        let all = load(&conn, Some(1), None).unwrap().contacts;
        let g = find_duplicates(&all);
        assert_eq!(ids(&g), [[a, b]]);
        let done = apply_merge(&mut conn, &dir, &g[0].ids, a, &Choices::new()).unwrap();
        assert_eq!((done.kept, done.deleted.clone()), (a, vec![b]));
        let left = load(&conn, Some(1), None).unwrap().contacts;
        assert_eq!(left.len(), 1);
        let f = write::form_of(&conn, &dir, a).unwrap();
        assert_eq!((f.company.as_str(), f.mobile.as_str(), f.email1.as_str()), ("ADI", "+33 6 12 34 56 78", "lea@x.fr"));
        assert!(f.notes.starts_with("A\n----------\nB"), "{}", f.notes);
        assert!(find_duplicates(&left).is_empty());
    }

    #[test]
    fn ecriture_en_echec_ne_supprime_rien() {
        let (dir, mut conn) = fixture("echec");
        let a = write::create(&mut conn, &dir, 1, &ContactForm { surname: "Martin".into(), ..Default::default() }).unwrap();
        let b = write::create(&mut conn, &dir, 1, &ContactForm { surname: "Martin".into(), ..Default::default() }).unwrap();
        // Photo qui n'est pas un JPEG : `update` refuse.
        let bad = ContactForm { surname: "Martin".into(), photo: Some("data:image/png;base64,iVBORw0KGgo=".into()), ..Default::default() };
        assert!(commit(&mut conn, &dir, a, &bad, &[a, b]).is_err());
        assert_eq!(load(&conn, Some(1), None).unwrap().contacts.len(), 2);
        // Contact conservé inconnu : idem.
        assert!(apply_merge(&mut conn, &dir, &[a, b, 999], a, &Choices::new()).is_err());
        assert_eq!(load(&conn, Some(1), None).unwrap().contacts.len(), 2);
    }
}
