//! Icônes Lucide (licence ISC) rendues en SVG en ligne, via `icondata_lu`.

use icondata_core::Icon;
use leptos::prelude::*;

pub use icondata_lu::{
    LuArchive as ARCHIVE, LuArrowDown as ARROW_DOWN, LuArrowDownAZ as SORT_AZ, LuArrowUp as ARROW_UP, LuBan as BAN, LuBold as BOLD, LuItalic as ITALIC, LuList as LIST, LuListOrdered as LIST_ORDERED, LuBookUser as BOOK_USER,
    LuCalendar as CALENDAR, LuCalendarDays as CALENDAR_DAYS, LuCalendarPlus as CALENDAR_PLUS, LuCheck as CHECK,
    LuCheckCheck as CHECK_CHECK, LuChevronDown as CHEVRON_DOWN, LuChevronLeft as CHEVRON_LEFT,
    LuChevronRight as CHEVRON_RIGHT, LuCircleAlert as CIRCLE_ALERT, LuCircleHelp as HELP, LuClock as CLOCK, LuCloudOff as CLOUD_OFF,
    LuDownload as DOWNLOAD, LuEllipsis as ELLIPSIS, LuEraser as ERASER, LuEye as EYE, LuFile as FILE, LuFlag as FLAG,
    LuFolderInput as FOLDER_INPUT, LuFolderPlus as FOLDER_PLUS, LuFolderSearch as FOLDER_SEARCH,
    LuForward as FORWARD, LuFunnel as FUNNEL, LuImageOff as IMAGE_OFF, LuInfo as INFO, LuListChecks as LIST_CHECKS,
    LuListFilter as LIST_FILTER, LuMail as MAIL, LuMailOpen as MAIL_OPEN, LuMailPlus as MAIL_PLUS,
    LuMessageSquareOff as MESSAGE_OFF, LuMinus as MINUS, LuPanelLeftClose as PANEL_LEFT_CLOSE,
    LuPanelLeftOpen as PANEL_LEFT_OPEN, LuPanelRight as PANEL_RIGHT, LuPaperclip as PAPERCLIP, LuPencil as PENCIL,
    LuPlus as PLUS, LuRefreshCw as REFRESH, LuReply as REPLY, LuReplyAll as REPLY_ALL, LuRows2 as ROWS_2, LuSave as SAVE,
    LuRows3 as ROWS_3, LuScissors as SCISSORS, LuSearch as SEARCH, LuSearchX as SEARCH_X, LuSend as SEND, LuSettings2 as SETTINGS, LuShieldAlert as SHIELD_ALERT,
    LuSquareCheck as SQUARE_CHECK, LuStickyNote as STICKY_NOTE, LuStar as STAR, LuTags as TAGS, LuTrash2 as TRASH, LuUndo2 as UNDO, LuUnderline as UNDERLINE, LuX as X,
    LuUsers as USERS, LuZap as ZAP,
};

/// Icône décorative : le libellé accessible est porté par le contrôle parent.
#[component]
pub fn Svg(icon: Icon, #[prop(default = 16)] size: u32) -> impl IntoView {
    view! {
        <svg
            width=size
            height=size
            viewBox=icon.view_box.unwrap_or("0 0 24 24")
            fill=icon.fill.unwrap_or("none")
            stroke=icon.stroke.unwrap_or("currentColor")
            stroke-width=icon.stroke_width.unwrap_or("2")
            stroke-linecap=icon.stroke_linecap.unwrap_or("round")
            stroke-linejoin=icon.stroke_linejoin.unwrap_or("round")
            aria-hidden="true"
            focusable="false"
            inner_html=icon.data
        ></svg>
    }
}
