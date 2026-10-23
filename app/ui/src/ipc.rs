//! Appels au cœur Rust via l'IPC de Tauri (`window.__TAURI__.core.invoke`).

use serde::{Serialize, de::DeserializeOwned};
use thunderlook_model::{
    CategoryInfo, ComposeKind, Draft, DraftAttachment, FolderOp, FolderTree, IndexStatus, ListQuery, ListView, MessageOp,
    AccountSummary, AutoReply, JunkLists, JunkMark, MessageSummary, MessageView, NewRule, OpResult, OpenItem, RuleDraft, SyncEvent, RuleRunReport, RuleRunRequest, RuleSummary, SearchQuery, Selection, Signature, SortField,
    quicksteps::QuickStep,
    calendar::{AppointmentForm, CalendarQuery, CalendarView},
    contacts::{ContactCard, ContactFolder, ContactList},
    settings::Settings,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

async fn call<A: Serialize, T: DeserializeOwned>(cmd: &str, args: &A) -> Result<T, String> {
    // Les u64 (jetons de vue) passent en nombres JS : ils restent très en dessous de 2^53.
    let ser = serde_wasm_bindgen::Serializer::json_compatible();
    let args = args.serialize(&ser).map_err(|e| e.to_string())?;
    let out = invoke(cmd, args)
        .await
        .map_err(|e| e.as_string().unwrap_or_else(|| format!("{cmd} : {e:?}")))?;
    serde_wasm_bindgen::from_value(out).map_err(|e| format!("{cmd} : {e}"))
}

#[derive(Serialize)]
struct NoArgs {}

#[derive(Serialize)]
struct SettingsArg {
    settings: Settings,
}

#[derive(Serialize)]
struct QueryArg<'a> {
    query: &'a ListQuery,
}

#[derive(Serialize)]
struct PositionArg<'a> {
    token: u64,
    id: &'a str,
}

/// Position d'un message dans une vue ouverte.
pub async fn message_position(token: u64, id: &str) -> Result<Option<u32>, String> {
    call("message_position", &PositionArg { token, id }).await
}

#[derive(Serialize)]
struct RowsArg {
    token: u64,
    start: u32,
    count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewArg<'a> {
    id: &'a str,
    allow_remote: bool,
}

#[derive(Serialize)]
struct AttachmentArg<'a> {
    id: &'a str,
    index: u32,
}

#[derive(Serialize)]
struct PerfArg<'a> {
    label: &'a str,
    ms: f64,
}

pub async fn folder_tree() -> Result<FolderTree, String> {
    call("folder_tree", &NoArgs {}).await
}

pub async fn open_list(query: &ListQuery) -> Result<ListView, String> {
    call("open_list", &QueryArg { query }).await
}

pub async fn list_rows(token: u64, start: u32, count: u32) -> Result<Vec<MessageSummary>, String> {
    call("list_rows", &RowsArg { token, start, count }).await
}

#[derive(Serialize)]
struct ToggleArg {
    token: u64,
    pos: u32,
    expand: Option<bool>,
}

pub async fn toggle_conversation(token: u64, pos: u32, expand: Option<bool>) -> Result<ListView, String> {
    call("toggle_conversation", &ToggleArg { token, pos, expand }).await
}

pub async fn message_view(id: &str, allow_remote: bool) -> Result<MessageView, String> {
    call("message_view", &ViewArg { id, allow_remote }).await
}

#[derive(Serialize)]
struct OpArg<'a> {
    selection: &'a Selection,
    op: &'a MessageOp,
}

#[derive(Serialize)]
struct FolderOpArg<'a> {
    op: &'a FolderOp,
}

pub async fn apply_op(selection: &Selection, op: &MessageOp) -> Result<OpResult, String> {
    call("apply_op", &OpArg { selection, op }).await
}

pub async fn folder_op(op: &FolderOp) -> Result<OpResult, String> {
    call("folder_op", &FolderOpArg { op }).await
}

#[derive(Serialize)]
struct SearchArg<'a> {
    query: &'a SearchQuery,
    sort: SortField,
    descending: bool,
}

pub async fn search(query: &SearchQuery, sort: SortField, descending: bool) -> Result<ListView, String> {
    call("search", &SearchArg { query, sort, descending }).await
}

pub async fn index_status() -> Result<IndexStatus, String> {
    call("index_status", &NoArgs {}).await
}

pub async fn undo() -> Result<Option<String>, String> {
    call("undo", &NoArgs {}).await
}

#[derive(Serialize)]
struct CalendarArg<'a> {
    query: &'a CalendarQuery,
}

pub async fn calendar_view(query: &CalendarQuery) -> Result<CalendarView, String> {
    call("calendar_view", &CalendarArg { query }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FormArg<'a> {
    id: Option<&'a str>,
    calendar_id: Option<&'a str>,
    start: Option<&'a str>,
}

/// Boîte « Rendez-vous » : rendez-vous existant, ou nouveau.
pub async fn appointment_form(id: Option<&str>, calendar_id: Option<&str>, start: Option<&str>) -> Result<AppointmentForm, String> {
    call("appointment_form", &FormArg { id, calendar_id, start }).await
}

#[derive(Serialize)]
struct SaveArg<'a> {
    form: &'a AppointmentForm,
}

pub async fn appointment_save(form: &AppointmentForm) -> Result<String, String> {
    call("appointment_save", &SaveArg { form }).await
}

#[derive(Serialize)]
struct DeleteArg<'a> {
    id: &'a str,
    series: bool,
}

pub async fn appointment_delete(id: &str, series: bool) -> Result<String, String> {
    call("appointment_delete", &DeleteArg { id, series }).await
}

/// Synchronisation du calendrier commun avec Outlook (le résultat arrive par « calendar-changed »).
pub async fn outlook_sync() -> Result<(), String> {
    call("outlook_sync", &NoArgs {}).await
}

/// Suit l'événement « calendar-changed » du cœur (calendrier commun relu).
pub fn on_calendar_changed(f: impl Fn() + 'static) {
    let handler = Closure::<dyn FnMut(JsValue)>::new(move |_: JsValue| f());
    wasm_bindgen_futures::spawn_local(async move {
        let _ = tauri_listen("calendar-changed", &handler).await;
        handler.forget();
    });
}

pub fn on_contacts_changed(f: impl Fn() + 'static) {
    let handler = Closure::<dyn FnMut(JsValue)>::new(move |_: JsValue| f());
    wasm_bindgen_futures::spawn_local(async move {
        let _ = tauri_listen("contacts-changed", &handler).await;
        handler.forget();
    });
}

pub async fn contact_folders() -> Result<Vec<ContactFolder>, String> {
    call("contact_folders", &NoArgs {}).await
}

#[derive(Serialize)]
struct FolderArg<'a> {
    folder: &'a str,
}

pub async fn contact_list(folder: &str) -> Result<ContactList, String> {
    call("contact_list", &FolderArg { folder }).await
}

pub async fn contact_card(id: &str) -> Result<ContactCard, String> {
    call("contact_card", &IdArg { id }).await
}

#[derive(Serialize)]
struct ShownArg<'a> {
    id: &'a str,
    shown: bool,
}

pub async fn set_calendar_shown(id: &str, shown: bool) -> Result<(), String> {
    call("set_calendar_shown", &ShownArg { id, shown }).await
}

pub async fn junk_lists() -> Result<JunkLists, String> {
    call("junk_lists", &NoArgs {}).await
}

#[derive(Serialize)]
struct JunkListsArg<'a> {
    lists: &'a JunkLists,
}

pub async fn set_junk_lists(lists: &JunkLists) -> Result<(), String> {
    call("set_junk_lists", &JunkListsArg { lists }).await
}

#[derive(Serialize)]
struct JunkMarkArg<'a> {
    selection: &'a Selection,
    mark: JunkMark,
}

pub async fn junk_mark(selection: &Selection, mark: JunkMark) -> Result<String, String> {
    call("junk_mark", &JunkMarkArg { selection, mark }).await
}

pub async fn calendar_import() -> Result<Option<String>, String> {
    call("calendar_import", &NoArgs {}).await
}

#[derive(Serialize)]
struct ExportArg<'a> {
    format: &'a str,
    query: &'a CalendarQuery,
}

pub async fn calendar_export(format: &str, query: &CalendarQuery) -> Result<Option<String>, String> {
    call("calendar_export", &ExportArg { format, query }).await
}

pub async fn calendar_remove(id: &str) -> Result<(), String> {
    call("calendar_remove", &IdArg { id }).await
}

pub async fn categories() -> Result<Vec<CategoryInfo>, String> {
    call("categories", &NoArgs {}).await
}

pub async fn open_attachment(id: &str, index: u32) -> Result<(), String> {
    call("open_attachment", &AttachmentArg { id, index }).await
}

pub async fn save_attachment(id: &str, index: u32) -> Result<bool, String> {
    call("save_attachment", &AttachmentArg { id, index }).await
}

pub async fn load_settings() -> Result<Settings, String> {
    call("load_settings", &NoArgs {}).await
}

pub async fn save_settings(settings: Settings) -> Result<(), String> {
    call("save_settings", &SettingsArg { settings }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposeArg<'a> {
    kind: ComposeKind,
    source_id: Option<&'a str>,
    folder_id: Option<&'a str>,
}

#[derive(Serialize)]
struct DraftArg<'a> {
    draft: &'a Draft,
}

#[derive(Serialize)]
struct IdArg<'a> {
    id: &'a str,
}

pub async fn compose(kind: ComposeKind, source_id: Option<&str>, folder_id: Option<&str>) -> Result<Draft, String> {
    call("compose", &ComposeArg { kind, source_id, folder_id }).await
}

pub async fn save_draft(draft: &Draft) -> Result<Draft, String> {
    call("save_draft", &DraftArg { draft }).await
}

pub async fn open_draft(id: &str) -> Result<Option<Draft>, String> {
    call("open_draft", &IdArg { id }).await
}

pub async fn compose_from_template() -> Result<Option<Draft>, String> {
    call("compose_from_template", &NoArgs {}).await
}

pub async fn signatures() -> Result<Vec<Signature>, String> {
    call("signatures", &NoArgs {}).await
}

pub async fn pick_attachments() -> Result<Vec<DraftAttachment>, String> {
    call("pick_attachments", &NoArgs {}).await
}

pub async fn rules() -> Result<Vec<RuleSummary>, String> {
    call("rules", &NoArgs {}).await
}

#[derive(Serialize)]
struct RuleActiveArg<'a> {
    id: &'a str,
    active: bool,
}

pub async fn set_rule_active(id: &str, active: bool) -> Result<(), String> {
    call("set_rule_active", &RuleActiveArg { id, active }).await
}

pub async fn rule_draft(id: &str) -> Result<RuleDraft, String> {
    call("rule_draft", &IdArg { id }).await
}

#[derive(Serialize)]
struct NewRuleArg<'a> {
    rule: &'a NewRule,
}

/// Renvoie l'identifiant (`r<id>`) de la règle créée.
pub async fn create_rule(rule: &NewRule) -> Result<String, String> {
    call("create_rule", &NewRuleArg { rule }).await
}

pub async fn import_rwz() -> Result<Option<usize>, String> {
    call("import_rwz", &NoArgs {}).await
}

#[derive(Serialize)]
struct RunRulesArg<'a> {
    request: &'a RuleRunRequest,
}

pub async fn run_rules(request: &RuleRunRequest) -> Result<RuleRunReport, String> {
    call("run_rules", &RunRulesArg { request }).await
}

pub async fn load_quick_steps() -> Result<Vec<QuickStep>, String> {
    call("load_quick_steps", &NoArgs {}).await
}

#[derive(Serialize)]
struct StepsArg<'a> {
    steps: &'a [QuickStep],
}

pub async fn save_quick_steps(steps: &[QuickStep]) -> Result<(), String> {
    call("save_quick_steps", &StepsArg { steps }).await
}

pub async fn accounts() -> Result<Vec<AccountSummary>, String> {
    call("accounts", &NoArgs {}).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AddAccountArg<'a> {
    address: &'a str,
    name: &'a str,
    password: &'a str,
    imap_host: &'a str,
    smtp_host: &'a str,
    imap_port: u16,
    imap_security: &'a str,
    smtp_port: u16,
    smtp_security: &'a str,
}

/// Serveurs d'un compte à ajouter : hôte, port et chiffrement (« tls » ou « starttls »).
pub struct AccountServers {
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_security: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: String,
}

pub async fn add_account(address: &str, name: &str, password: &str, s: &AccountServers) -> Result<Vec<AccountSummary>, String> {
    call(
        "add_account",
        &AddAccountArg {
            address,
            name,
            password,
            imap_host: &s.imap_host,
            smtp_host: &s.smtp_host,
            imap_port: s.imap_port,
            imap_security: &s.imap_security,
            smtp_port: s.smtp_port,
            smtp_security: &s.smtp_security,
        },
    )
    .await
}

#[derive(Serialize)]
struct AccountRulesArg<'a> {
    id: &'a str,
    on: bool,
}

pub async fn set_account_rules(id: &str, on: bool) -> Result<Vec<AccountSummary>, String> {
    call("set_account_rules", &AccountRulesArg { id, on }).await
}

#[derive(Serialize)]
struct CopyRulesArg<'a> {
    origin: &'a str,
    account: &'a str,
}

pub async fn copy_rules(origin: &str, account: &str) -> Result<String, String> {
    call("copy_rules", &CopyRulesArg { origin, account }).await
}

pub async fn remove_account(id: &str) -> Result<Vec<AccountSummary>, String> {
    call("remove_account", &IdArg { id }).await
}

pub async fn send_draft(draft: &Draft) -> Result<String, String> {
    call("send_draft", &DraftArg { draft }).await
}

pub async fn get_auto_reply() -> Result<AutoReply, String> {
    call("get_auto_reply", &NoArgs {}).await
}

#[derive(Serialize)]
struct AutoReplyArg<'a> {
    config: &'a AutoReply,
}

pub async fn set_auto_reply(config: &AutoReply) -> Result<(), String> {
    call("set_auto_reply", &AutoReplyArg { config }).await
}

#[derive(Serialize)]
struct OfflineArg {
    offline: bool,
}

pub async fn set_offline(offline: bool) -> Result<bool, String> {
    call("set_offline", &OfflineArg { offline }).await
}

pub async fn sync_now() -> Result<(), String> {
    call("sync_now", &NoArgs {}).await
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = listen, catch)]
    async fn tauri_listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> Result<JsValue, JsValue>;
}

#[derive(serde::Deserialize)]
struct EventEnvelope {
    payload: SyncEvent,
}

/// Suit l'événement « sync » du cœur (envoi et réception) pendant toute la vie de l'interface.
pub fn on_sync(f: impl Fn(SyncEvent) + 'static) {
    let handler = Closure::<dyn FnMut(JsValue)>::new(move |v: JsValue| {
        if let Ok(e) = serde_wasm_bindgen::from_value::<EventEnvelope>(v) {
            f(e.payload);
        }
    });
    wasm_bindgen_futures::spawn_local(async move {
        let _ = tauri_listen("sync", &handler).await;
        handler.forget();
    });
}

#[derive(serde::Deserialize)]
struct OpenItemEnvelope {
    payload: OpenItem,
}

/// Suit l'événement « open-item » du cœur (clic sur une notification de nouveaux messages).
pub fn on_open_item(f: impl Fn(OpenItem) + 'static) {
    let handler = Closure::<dyn FnMut(JsValue)>::new(move |v: JsValue| {
        if let Ok(e) = serde_wasm_bindgen::from_value::<OpenItemEnvelope>(v) {
            f(e.payload);
        }
    });
    wasm_bindgen_futures::spawn_local(async move {
        let _ = tauri_listen("open-item", &handler).await;
        handler.forget();
    });
}

/// Mesure de performance, journalisée par le cœur si `THUNDERLOOK_PERF` est défini.
pub async fn perf_mark(label: &str, ms: f64) {
    let _ = call::<_, ()>("perf_mark", &PerfArg { label, ms }).await;
}
