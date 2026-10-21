//! Appels au cœur Rust via l'IPC de Tauri (`window.__TAURI__.core.invoke`).

use serde::{Serialize, de::DeserializeOwned};
use thunderlook_model::{
    CategoryInfo, ComposeKind, Draft, DraftAttachment, FolderOp, FolderTree, IndexStatus, ListQuery, ListView, MessageOp,
    AccountSummary, MessageSummary, MessageView, OpResult, SyncEvent, RuleRunReport, RuleRunRequest, RuleSummary, SearchQuery, Selection, Signature, SortField,
    quicksteps::QuickStep,
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
}

#[derive(Serialize)]
struct DraftArg<'a> {
    draft: &'a Draft,
}

#[derive(Serialize)]
struct IdArg<'a> {
    id: &'a str,
}

pub async fn compose(kind: ComposeKind, source_id: Option<&str>) -> Result<Draft, String> {
    call("compose", &ComposeArg { kind, source_id }).await
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
}

pub async fn add_account(address: &str, name: &str, password: &str, imap_host: &str, smtp_host: &str) -> Result<Vec<AccountSummary>, String> {
    call("add_account", &AddAccountArg { address, name, password, imap_host, smtp_host }).await
}

pub async fn remove_account(id: &str) -> Result<Vec<AccountSummary>, String> {
    call("remove_account", &IdArg { id }).await
}

pub async fn send_draft(draft: &Draft) -> Result<String, String> {
    call("send_draft", &DraftArg { draft }).await
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

/// Mesure de performance, journalisée par le cœur si `THUNDERLOOK_PERF` est défini.
pub async fn perf_mark(label: &str, ms: f64) {
    let _ = call::<_, ()>("perf_mark", &PerfArg { label, ms }).await;
}
