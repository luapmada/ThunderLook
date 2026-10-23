//! Types et logique d'affichage partagés entre le cœur (natif) et l'interface (WebAssembly).
//!
//! Le cœur calcule tout ce qui dépend de l'heure locale (groupes de dates, libellés) ;
//! l'interface ne fait qu'afficher.

pub mod calendar;
pub mod contacts;
pub mod format;
pub mod items;
pub mod quicksteps;
pub mod settings;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FolderRole {
    Inbox,
    Drafts,
    Sent,
    Deleted,
    Junk,
    Outbox,
    Archive,
    Virtual,
    Folder,
}

impl FolderRole {
    /// Brouillons, indésirables, boîte d'envoi et dossiers virtuels affichent le **total** entre
    /// crochets ; les autres affichent le nombre de non-lus.
    pub fn shows_total(self) -> bool {
        matches!(self, Self::Drafts | Self::Junk | Self::Outbox | Self::Virtual)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub role: FolderRole,
    pub unread: u32,
    pub total: u32,
    #[serde(default)]
    pub children: Vec<Folder>,
}

impl Folder {
    /// Texte du compteur tel qu'affiché à droite du nom, ou `None` si rien à afficher.
    pub fn badge(&self) -> Option<String> {
        if self.role.shows_total() {
            (self.total > 0).then(|| format!("[{}]", format::thousands(self.total)))
        } else {
            (self.unread > 0).then(|| format::thousands(self.unread))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FolderSection {
    pub id: String,
    pub name: String,
    pub folders: Vec<Folder>,
}

/// Arborescence du courrier envoyée à l'interface.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderTree {
    pub sections: Vec<FolderSection>,
    /// Dossier de données ouvert.
    pub data_dir: String,
    /// Raison pour laquelle il n'a pas pu être ouvert.
    pub error: Option<String>,
}

impl FolderTree {
    /// Recherche d'un dossier par identifiant dans toutes les sections.
    pub fn find_folder(&self, id: &str) -> Option<&Folder> {
        fn walk<'a>(folders: &'a [Folder], id: &str) -> Option<&'a Folder> {
            folders.iter().find_map(|f| if f.id == id { Some(f) } else { walk(&f.children, id) })
        }
        self.sections.iter().find_map(|s| walk(&s.folders, id))
    }

    /// Première « Boîte de réception », sinon le premier dossier.
    pub fn default_folder(&self) -> Option<&Folder> {
        let all = || self.sections.iter().flat_map(|s| s.folders.iter());
        all().find(|f| f.role == FolderRole::Inbox).or_else(|| all().next())
    }
}

/// Champ de tri de la liste.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortField {
    #[default]
    Date,
    From,
    Subject,
    Size,
    /// Un élément figure sous chacune de ses catégories.
    Categories,
    /// Avec suivi, terminé, sans indicateur.
    Flag,
    /// Avec ou sans pièce jointe.
    Attachments,
    /// Premier destinataire (« À »).
    To,
    /// Haute, normale, faible.
    Importance,
}

impl SortField {
    pub const ALL: [SortField; 9] = [
        SortField::Date,
        SortField::From,
        SortField::Subject,
        SortField::Size,
        SortField::Categories,
        SortField::Flag,
        SortField::Attachments,
        SortField::To,
        SortField::Importance,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Date => "Date",
            Self::From => "De",
            Self::Subject => "Objet",
            Self::Size => "Taille",
            Self::Categories => "Catégories",
            Self::Flag => "Indicateur",
            Self::Attachments => "Pièces jointes",
            Self::To => "À",
            Self::Importance => "Importance",
        }
    }

    /// Sens par défaut quand on choisit ce tri : les plus récents et les plus gros d'abord.
    pub fn default_descending(self) -> bool {
        matches!(self, Self::Date | Self::Size)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    pub folder_id: String,
    pub unread_only: bool,
    pub sort: SortField,
    pub descending: bool,
    /// « Afficher en tant que conversations » : pris en compte pour le tri par date d'un dossier réel.
    #[serde(default)]
    pub conversations: bool,
}

/// Groupe contigu de la liste triée (« Aujourd'hui », un expéditeur, une tranche de taille…).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListGroup {
    pub label: String,
    pub start: u32,
    pub count: u32,
}

/// Vue ouverte sur un dossier. Le cœur garde l'ordre des éléments ; l'interface demande
/// les lignes visibles par tranches (`list_rows`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListView {
    pub token: u64,
    /// Éléments de la vue, filtre appliqué.
    pub count: u32,
    /// Compteurs du dossier entier, pour la barre d'état.
    pub folder_total: u32,
    pub folder_unread: u32,
    pub groups: Vec<ListGroup>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlagState {
    #[default]
    None,
    Flagged,
    Done,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Importance {
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryChip {
    pub name: String,
    /// Couleur CSS ; `None` pour une catégorie sans couleur ou sans définition.
    pub color: Option<String>,
}

/// Dernière action faite sur un message reçu (`PidTagLastVerbExecuted`, [MS-OXOMSG] § 2.2.1.24) : Outlook en tire
/// l'icône de la liste (`PidTagIconIndex` 0x105 répondu, 0x106 transféré) et la barre « Vous avez répondu… ».
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LastVerb {
    Replied,
    RepliedAll,
    Forwarded,
}

impl LastVerb {
    /// `NOTEIVERB_REPLYTOSENDER` (102), `NOTEIVERB_REPLYTOALL` (103), `NOTEIVERB_FORWARD` (104).
    pub fn from_code(code: i64) -> Option<Self> {
        match code {
            102 => Some(Self::Replied),
            103 => Some(Self::RepliedAll),
            104 => Some(Self::Forwarded),
            _ => None,
        }
    }

    pub fn code(self) -> i64 {
        match self {
            Self::Replied => 102,
            Self::RepliedAll => 103,
            Self::Forwarded => 104,
        }
    }

    pub fn is_forward(self) -> bool {
        self == Self::Forwarded
    }
}

/// Ligne de la liste des messages, prête à afficher.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageSummary {
    pub id: String,
    /// Position dans la vue.
    pub pos: u32,
    pub from: String,
    pub subject: String,
    pub preview: String,
    /// Libellé court de la liste (« 09:12 », « lun. 17:05 », « 04/10/2026 »).
    pub time_label: String,
    pub size_label: String,
    pub unread: bool,
    pub flag: FlagState,
    pub importance: Importance,
    pub has_attachments: bool,
    pub categories: Vec<CategoryChip>,
    /// Dossier de l'élément, dans les résultats de recherche et pour un élément de conversation venu d'un
    /// autre dossier.
    #[serde(default)]
    pub folder: Option<String>,
    /// En-tête d'une conversation de plusieurs éléments.
    #[serde(default)]
    pub conversation: Option<ConversationInfo>,
    /// Élément d'une conversation dépliée, affiché sous son en-tête.
    #[serde(default)]
    pub child: bool,
    /// Répondu ou transféré (icône de la liste).
    #[serde(default)]
    pub last_verb: Option<LastVerb>,
}

/// Conversation résumée par son en-tête. Les éléments comptés sont ceux du fichier de données, hors
/// « Éléments supprimés » et « Courrier indésirable » (sauf s'ils sont dans le dossier affiché).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationInfo {
    pub count: u32,
    pub unread: u32,
    pub expanded: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    pub index: u32,
    pub name: String,
    pub size_label: String,
    /// Message joint (`message/rfc822`).
    pub message: bool,
    /// Type exécutable : ouverture refusée, enregistrement possible.
    pub blocked: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum BodyView {
    /// Document complet pour un `iframe` isolé (`srcdoc`, sans scripts).
    Html { srcdoc: String },
    Text { text: String },
    Empty,
}

/// Message complet pour le volet de lecture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub id: String,
    pub subject: String,
    pub from: String,
    pub from_address: String,
    pub to: String,
    pub cc: String,
    pub date_label: String,
    pub importance: Importance,
    pub flag: FlagState,
    pub flag_label: Option<String>,
    pub categories: Vec<CategoryChip>,
    pub attachments: Vec<AttachmentInfo>,
    pub body: BodyView,
    /// Contenu distant présent et bloqué (images, feuilles de style).
    pub remote_blocked: bool,
    /// Élément autre qu'un courrier (contact, rendez-vous…), en attendant les modules dédiés.
    pub item_kind: Option<String>,
    /// Répondu ou transféré, et la barre d'information d'Outlook (« Vous avez répondu à ce message le … »).
    #[serde(default)]
    pub last_verb: Option<LastVerb>,
    #[serde(default)]
    pub last_verb_label: Option<String>,
}

/// Étendue d'une recherche (onglet Rechercher).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchScope {
    Folder,
    Subfolders,
    /// Fichier de données du dossier courant (« Boîte aux lettres actuelle »).
    #[default]
    Source,
    All,
}

impl SearchScope {
    pub const ALL: [SearchScope; 4] = [SearchScope::Folder, SearchScope::Subfolders, SearchScope::Source, SearchScope::All];

    pub fn label(self) -> &'static str {
        match self {
            Self::Folder => "Dossier actif",
            Self::Subfolders => "Sous-dossiers",
            Self::Source => "Fichier de données actuel",
            Self::All => "Tous les fichiers de données",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub text: String,
    pub scope: SearchScope,
    /// Dossier courant, point de départ de l'étendue.
    pub folder_id: String,
}

/// Avancement de l'indexation plein texte (tâche de fond).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub done: u64,
    pub total: u64,
    pub running: bool,
}

/// Éléments visés par une opération : plages de positions d'une vue ouverte (`[début, fin[`)
/// et/ou identifiants explicites. Une sélection de 20 000 éléments n'oblige pas à charger leurs lignes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub token: u64,
    pub ranges: Vec<(u32, u32)>,
    pub ids: Vec<String>,
}

impl Selection {
    pub fn ids(ids: Vec<String>) -> Self {
        Self { token: 0, ranges: Vec::new(), ids }
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty() && self.ranges.iter().all(|(a, b)| b <= a)
    }

    pub fn len(&self) -> u32 {
        self.ids.len() as u32 + self.ranges.iter().map(|(a, b)| b.saturating_sub(*a)).sum::<u32>()
    }
}

/// Indicateur de suivi à poser ([MS-OXOFLAG] : début et échéance).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlagAction {
    Today,
    Tomorrow,
    ThisWeek,
    NextWeek,
    NoDate,
    Complete,
    Clear,
    /// Touche Inser : pose « Aujourd'hui », ou marque terminé si déjà marqué.
    Toggle,
}

impl FlagAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Today => "Aujourd'hui",
            Self::Tomorrow => "Demain",
            Self::ThisWeek => "Cette semaine",
            Self::NextWeek => "La semaine prochaine",
            Self::NoDate => "Sans date",
            Self::Complete => "Marquer comme terminé",
            Self::Clear => "Effacer l'indicateur",
            Self::Toggle => "Assurer un suivi",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "op")]
pub enum MessageOp {
    Read { read: bool },
    /// Marquage automatique quand la sélection quitte l'élément : hors historique d'annulation.
    Seen,
    /// Lu si au moins un élément est non lu, sinon non lu (bouton « Lu/Non lu »).
    ToggleRead,
    Flag { action: FlagAction },
    Category { name: String, on: bool },
    ClearCategories,
    Move { folder_id: String },
    Copy { folder_id: String },
    /// Vers « Éléments supprimés » de la source ; définitif si l'élément y est déjà.
    Delete,
    DeletePermanently,
    Archive,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "op")]
pub enum FolderOp {
    Create { parent_id: String, name: String },
    Rename { id: String, name: String },
    Move { id: String, parent_id: String },
    Delete { id: String },
    MarkAllRead { id: String },
    Empty { id: String },
    AddFavorite { id: String },
    RemoveFavorite { id: String },
    /// Enregistre une recherche comme dossier de recherche.
    SaveSearch { name: String, query: String, scope: SearchScope, folder_id: String },
    DeleteSearchFolder { id: String },
}

/// Résultat d'une opération : texte pour la barre d'état, et libellé de ce que Ctrl+Z annulera.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpResult {
    pub affected: u32,
    pub message: String,
    pub undo_label: Option<String>,
    /// Dossier créé, pour le sélectionner.
    pub created_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryInfo {
    pub name: String,
    pub color: Option<String>,
}

/// Nature d'un message en rédaction (J5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComposeKind {
    #[default]
    New,
    Reply,
    ReplyAll,
    Forward,
}

/// Origine d'une pièce jointe de brouillon.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DraftAttachmentSource {
    /// Pièce jointe `index` (numérotation de [`MessageView::attachments`]) d'un message enregistré.
    Message { id: String, index: u32 },
    /// Fichier choisi sur le disque.
    File { path: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftAttachment {
    pub name: String,
    pub size_label: String,
    pub source: DraftAttachmentSource,
}

/// Message en rédaction. Les destinataires sont des listes « Nom <adresse> » séparées par des points-virgules,
/// comme les saisit l'utilisateur. Le corps est le contenu de `<body>`, images intégrées en URI `data:`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    /// Brouillon enregistré (`m…`) ; absent tant qu'il ne l'a pas été.
    pub id: Option<String>,
    pub kind: ComposeKind,
    /// Message auquel on répond, ou que l'on transfère.
    pub source_id: Option<String>,
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub html: String,
    /// Feuilles de style du message cité (`<style>` de son en-tête), gardées pour sa mise en forme.
    pub styles: String,
    pub attachments: Vec<DraftAttachment>,
    /// Compte d'envoi (`s<source>`, champ « De ») ; absent : compte du message d'origine, sinon le premier.
    #[serde(default)]
    pub from_account: Option<String>,
}

/// Signature d'Outlook (`%APPDATA%\Microsoft\Signatures`), images intégrées en URI `data:`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    pub name: String,
    pub html: String,
}

/// Règle côté client, pour la boîte « Règles et alertes » (J6).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSummary {
    /// `r<id>`.
    pub id: String,
    /// Jeu de règles : `pst:<fai>` (flux d'un PST ; `pst:s<source>` pour un PST qui n'en avait pas et où une règle a
    /// été créée), `account:<source>` (compte) ou chemin du `.rwz` importé.
    pub origin: String,
    /// Libellé du jeu (nom du fichier de données ou du `.rwz`).
    pub origin_label: String,
    pub position: u32,
    pub name: String,
    pub active: bool,
    /// Description ligne à ligne, avec les libellés d'Outlook.
    pub description: Vec<String>,
    /// Décodage incomplet : raison (la règle est alors ignorée par « Exécuter maintenant »).
    pub error: Option<String>,
    /// Points à vérifier avant d'exécuter : dossier cible introuvable, ou choisi par le nom parmi des homonymes.
    pub warnings: Vec<String>,
}

/// « Créer une règle » (clic droit sur un message) : valeurs proposées d'après le message.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleDraft {
    /// `m<id>`.
    pub message_id: String,
    pub from_name: String,
    pub from_addr: String,
    pub subject: String,
    /// Premier destinataire de la zone À.
    pub to_name: String,
    pub to_addr: String,
    /// Dossier du message (`f<id>`), pour « Exécuter cette règle maintenant ».
    pub folder_id: String,
    pub folder_name: String,
    /// Jeu de règles qui recevra la règle (nom du compte ou du fichier de données).
    pub rule_set: String,
    /// Remarque affichée dans la boîte (règles à l'arrivée désactivées, fichier de données sans compte).
    pub note: Option<String>,
    /// Section du volet des dossiers (`s<source>`) du compte qui recevra la règle : un dossier de destination hors de
    /// cette section (PST, autre compte) retire les messages du serveur, ce que la boîte signale.
    #[serde(default)]
    pub account_section: Option<String>,
}

/// Règle créée depuis un message : conditions cochées et dossier de destination.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewRule {
    /// `m<id>` : le message d'où viennent l'expéditeur et le destinataire, et dont le compte reçoit la règle.
    pub message_id: String,
    pub name: String,
    /// « De <expéditeur> ».
    pub from: bool,
    /// « L'objet contient » : texte, `None` si la case n'est pas cochée.
    pub subject: Option<String>,
    /// « Envoyé à <destinataire> ».
    pub sent_to: bool,
    /// Dossier de destination (`f<id>`).
    pub folder_id: String,
}

/// Messages concernés par « Exécuter maintenant » (« Appliquer les règles à »).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunFilter {
    #[default]
    All,
    Read,
    Unread,
}

/// Demande « Exécuter maintenant » : règles (`r<id>`, exécutées dans l'ordre de la liste), dossier parcouru.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRunRequest {
    pub rule_ids: Vec<String>,
    pub folder_id: String,
    pub subfolders: bool,
    pub filter: RunFilter,
    /// Faux : simulation seulement, rien n'est modifié.
    pub apply: bool,
    /// Non vide : seulement ces messages (`m<id>`) du dossier (règles à l'arrivée, J7).
    #[serde(default)]
    pub only: Vec<String>,
    /// Au-delà de `RULE_MOVES_LIMIT` déplacements, l'application exige ce nombre exact, lu dans un aperçu.
    #[serde(default)]
    pub confirm_moves: Option<u32>,
}

/// Nombre de déplacements (ou suppressions) au-delà duquel une exécution de règles doit être confirmée.
pub const RULE_MOVES_LIMIT: u32 = 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRunEntry {
    pub rule_id: String,
    pub name: String,
    /// Messages qui remplissent les conditions (avant un éventuel « arrêter » d'une règle précédente : non comptés).
    pub matched: u32,
    /// Premiers messages concernés : « date · expéditeur · objet ».
    pub samples: Vec<String>,
    /// Actions exécutées (libellés d'Outlook).
    pub actions: Vec<String>,
    /// Actions non exécutées, avec la raison.
    pub not_run: Vec<String>,
    pub warnings: Vec<String>,
    /// Règle écartée : raison.
    pub skipped: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRunReport {
    pub scanned: u32,
    /// Messages qui ont au moins une action à exécuter.
    pub changed: u32,
    pub applied: bool,
    pub elapsed_ms: u32,
    pub rules: Vec<RuleRunEntry>,
    pub errors: Vec<String>,
    /// Messages qui changeraient de dossier (déplacés ou supprimés).
    #[serde(default)]
    pub moves: u32,
    /// Application refusée faute de confirmation (plus de `RULE_MOVES_LIMIT` déplacements) : rien n'a été modifié.
    #[serde(default)]
    pub needs_confirmation: bool,
}

/// Compte de messagerie (J7), pour la boîte « Paramètres du compte ».
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    /// `s<source>` : même identifiant que la section du volet des dossiers.
    pub id: String,
    pub address: String,
    pub sender_name: String,
    pub imap: String,
    pub smtp: String,
    /// Dernière relève réussie (libellé), et dernière erreur.
    pub last_sync: Option<String>,
    pub last_error: Option<String>,
    /// Règles actives appliquées aux nouveaux messages de la Boîte de réception (désactivé par défaut).
    #[serde(default)]
    pub rules_on_arrival: bool,
    /// Changements locaux pas encore reportés sur le serveur (file `imap_pending`).
    #[serde(default)]
    pub pending: u32,
}

/// État de l'envoi et de la réception (J7), émis par le cœur (événement « sync ») à chaque début et fin de passage.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEvent {
    pub running: bool,
    /// Texte de la barre d'état (« Envoi et réception… », « Message envoyé », « Tous les dossiers sont à jour »).
    pub message: String,
    pub sent: u32,
    pub received: u32,
    pub error: Option<String>,
}

/// Clic sur une notification de nouveaux messages (événement « open-item ») : dossier à afficher, et message à
/// sélectionner s'il n'y en avait qu'un.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenItem {
    pub folder_id: String,
    pub message_id: Option<String>,
}

/// Réponse automatique locale (J7), enregistrée par le cœur dans `autoreply.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoReply {
    pub enabled: bool,
    /// Période (dates `AAAA-MM-JJ`, incluses) ; vide : sans limite.
    pub start: Option<String>,
    pub end: Option<String>,
    pub text: String,
    /// Expéditeurs qui ont déjà reçu une réponse pendant la période en cours (vidé à chaque activation).
    pub replied: Vec<String>,
}

/// Courrier indésirable : expéditeurs bloqués et approuvés (adresses, ou domaines écrits « @exemple.fr »), comme
/// les listes d'Outlook. Un expéditeur approuvé n'est jamais bloqué.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JunkLists {
    pub blocked: Vec<String>,
    pub safe: Vec<String>,
}

impl JunkLists {
    fn listed(list: &[String], address: &str) -> bool {
        let a = address.trim().to_lowercase();
        let domain = a.rsplit_once('@').map(|(_, d)| format!("@{d}"));
        list.iter().map(|x| x.trim().to_lowercase()).any(|x| x == a || Some(&x) == domain.as_ref())
    }

    pub fn is_blocked(&self, address: &str) -> bool {
        !address.trim().is_empty() && Self::listed(&self.blocked, address) && !Self::listed(&self.safe, address)
    }

    /// Ajoute à une liste (sans doublon) et retire de l'autre.
    pub fn add(&mut self, entry: &str, blocked: bool) {
        let e = entry.trim().to_lowercase();
        if e.is_empty() {
            return;
        }
        let (into, from) = if blocked { (&mut self.blocked, &mut self.safe) } else { (&mut self.safe, &mut self.blocked) };
        from.retain(|x| x.to_lowercase() != e);
        if !into.iter().any(|x| x.to_lowercase() == e) {
            into.push(e);
            into.sort();
        }
    }
}

/// Classement d'expéditeurs depuis un message (menu « Courrier indésirable »).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JunkMark {
    /// « Bloquer l'expéditeur ».
    Block,
    /// « Ne jamais bloquer l'expéditeur ».
    Safe,
    /// « Ne jamais bloquer le domaine de l'expéditeur ».
    SafeDomain,
}

/// Initiales pour la pastille de l'expéditeur.
pub fn initials(name: &str) -> String {
    let mut words = name.split_whitespace().filter_map(|w| w.chars().find(|c| c.is_alphanumeric()));
    let s: String = words.by_ref().take(2).flat_map(char::to_uppercase).collect();
    if s.is_empty() { "?".into() } else { s }
}

#[cfg(test)]
mod tests {
    #[test]
    fn junk_lists_block_by_address_or_domain_and_safe_wins() {
        let mut l = JunkLists::default();
        l.add("Spam@Exemple.fr", true);
        l.add("@pub.example", true);
        assert!(l.is_blocked("spam@exemple.fr"));
        assert!(l.is_blocked("qui@pub.example"));
        assert!(!l.is_blocked("autre@exemple.fr"));
        assert!(!l.is_blocked(""));
        l.add("ami@pub.example", false);
        assert!(!l.is_blocked("ami@pub.example"), "approuvé : jamais bloqué");
        l.add("spam@exemple.fr", false);
        assert!(l.blocked.iter().all(|x| x != "spam@exemple.fr"), "passé dans les approuvés");
    }

    use super::*;

    #[test]
    fn initials_cases() {
        assert_eq!(initials("Camille Martin"), "CM");
        assert_eq!(initials("Équipe ThunderLook"), "ÉT");
        assert_eq!(initials("bibliotheque"), "B");
        assert_eq!(initials("   "), "?");
    }

    #[test]
    fn badge_unread_vs_total() {
        let mut f = Folder { id: "x".into(), name: "x".into(), role: FolderRole::Inbox, unread: 1234, total: 2000, children: vec![] };
        assert_eq!(f.badge().as_deref(), Some("1\u{202f}234"));
        f.role = FolderRole::Drafts;
        assert_eq!(f.badge().as_deref(), Some("[2\u{202f}000]"));
        f.total = 0;
        assert_eq!(f.badge(), None);
    }
}
