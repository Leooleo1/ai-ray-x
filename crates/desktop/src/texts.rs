//! The app's own words (menu bar, notifications, startup), in the system's language.

use std::sync::OnceLock;

pub struct Texts {
    pub open: &'static str,
    pub open_browser: &'static str,
    pub start_at_login: &'static str,
    pub quit: &'static str,
    pub starting: &'static str,
    pub idle: &'static str,
    pub working_one: &'static str,
    /// "{n}" is the number of AIs.
    pub working_many: &'static str,
    /// "{e}" is the reason.
    pub failed: &'static str,
    pub proposal: &'static str,
    pub draft: &'static str,
    pub broke: &'static str,
}

const PT: Texts = Texts {
    open: "Abrir o raiox",
    open_browser: "Abrir no navegador",
    start_at_login: "Abrir ao iniciar o computador",
    quit: "Sair do raiox",
    starting: "Abrindo…",
    idle: "Nenhuma IA trabalhando",
    working_one: "1 IA trabalhando",
    working_many: "{n} IAs trabalhando",
    failed: "Não deu para abrir o raiox: {e}",
    proposal: "Esperando sua aprovação",
    draft: "Rascunho pronto para revisar",
    broke: "Não deu certo",
};

const EN: Texts = Texts {
    open: "Open raiox",
    open_browser: "Open in the browser",
    start_at_login: "Open at login",
    quit: "Quit raiox",
    starting: "Opening…",
    idle: "No AI working",
    working_one: "1 AI working",
    working_many: "{n} AIs working",
    failed: "raiox could not open: {e}",
    proposal: "Waiting for your approval",
    draft: "Draft ready to review",
    broke: "It did not work",
};

pub fn texts() -> &'static Texts {
    static CHOSEN: OnceLock<&'static Texts> = OnceLock::new();
    CHOSEN.get_or_init(|| if sys_locale::get_locale().is_some_and(|l| l.to_lowercase().starts_with("pt")) { &PT } else { &EN })
}

/// The menu bar's line about the AIs at work.
pub fn working(n: usize) -> String {
    let t = texts();
    match n {
        0 => t.idle.to_string(),
        1 => t.working_one.to_string(),
        _ => t.working_many.replace("{n}", &n.to_string()),
    }
}
