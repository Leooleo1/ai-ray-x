//! Who answers a channel message that @mentions nobody: chosen here, for
//! free, without asking any AI. Each member is scored on what the message
//! talks about: its role (with the words that go with it: "tela" and "css"
//! for a frontend role), its name, its project's name and its project's own
//! file names. The best match answers; with no clear match, the lead does,
//! as before. Every member not called is a whole turn (and its tokens) saved.

use std::collections::BTreeSet;
use std::path::Path;

/// Words that go with a role, in Portuguese and English; a role naming the key brings them.
const ROLE_WORDS: [(&str, &[&str]); 10] = [
    ("front", &["tela", "telas", "botao", "botoes", "css", "layout", "interface", "pagina", "componente", "visual", "responsivo", "mobile", "screen", "button", "page", "component", "ui", "ux", "react", "html"]),
    ("back", &["api", "banco", "servidor", "endpoint", "rota", "rotas", "sql", "migracao", "query", "server", "database", "route", "auth", "login", "webhook"]),
    ("design", &["cor", "cores", "fonte", "tipografia", "icone", "icones", "logo", "visual", "espacamento", "color", "font", "icon", "spacing", "figma"]),
    ("qa", &["teste", "testes", "bug", "erro", "quebrou", "falha", "regressao", "test", "tests", "error", "broken", "failing"]),
    ("test", &["teste", "testes", "bug", "erro", "quebrou", "falha", "test", "tests", "error", "broken", "failing"]),
    ("devops", &["deploy", "publicar", "ci", "pipeline", "docker", "servidor", "infra", "release", "build"]),
    ("produto", &["prioridade", "roadmap", "escopo", "cliente", "requisito", "requisitos", "priority", "scope", "customer", "requirement"]),
    ("product", &["prioridade", "roadmap", "escopo", "cliente", "requisito", "priority", "scope", "customer", "requirement", "spec"]),
    ("segur", &["seguranca", "senha", "token", "vulnerabilidade", "permissao", "security", "password", "vulnerability", "permission", "secret"]),
    ("doc", &["documentacao", "readme", "guia", "tutorial", "docs", "documentation", "guide"]),
];
/// Words too common to tell members apart.
const STOPWORDS: [&str; 40] = [
    "que", "para", "com", "uma", "como", "isso", "esse", "essa", "este", "esta", "nos", "das", "dos", "por", "mais", "tem", "ser", "voce", "pode", "fazer",
    "the", "and", "for", "with", "this", "that", "what", "how", "can", "you", "are", "from", "have", "make", "into", "all", "not", "our", "your", "why",
];
/// File names read from a project, at most, two folder levels deep.
const MAX_FILES: usize = 400;
const SKIP: [&str; 8] = ["node_modules", "target", "dist", "build", ".git", ".next", "vendor", "coverage"];

/// What a member answers to: its id and the words that point at it, each with a weight.
pub struct Candidate {
    pub id: String,
    pub words: Vec<(String, u32)>,
}

/// Who answers, and why: the words of the message that pointed at each one.
#[derive(Debug, PartialEq)]
pub struct Routed {
    pub ids: Vec<String>,
    pub why: Vec<(String, Vec<String>)>,
    /// No member stood out: the lead answers, as without a router.
    pub by_lead: bool,
}

/// Lowercase, without accents, so "Botão" and "botao" are the same word.
fn plain(word: &str) -> String {
    word.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'ê' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ô' | 'õ' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// The words of a text worth matching: three letters or more, not too common.
fn words(text: &str) -> BTreeSet<String> {
    plain(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3 && !STOPWORDS.contains(w))
        .map(str::to_string)
        .collect()
}

/// A project's file and folder names (without extensions), two levels deep, as words.
pub fn file_words(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let mut folders = vec![(dir.to_path_buf(), 0)];
    while let Some((folder, depth)) = folders.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || SKIP.contains(&name.as_str()) || names.len() >= MAX_FILES {
                continue;
            }
            let stem = name.split('.').next().unwrap_or(&name).to_string();
            names.extend(words(&stem.replace(['-', '_'], " ")));
            if depth < 1 && entry.file_type().is_ok_and(|t| t.is_dir()) {
                folders.push((entry.path(), depth + 1));
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// The words that point at a member, weighted: its name and project count most.
pub fn candidate(id: &str, name: &str, project: &str, role: &str, files: &[String]) -> Candidate {
    let mut weighted: Vec<(String, u32)> = Vec::new();
    weighted.extend(words(name).into_iter().map(|w| (w, 3)));
    weighted.extend(words(&project.replace(['-', '_'], " ")).into_iter().map(|w| (w, 3)));
    let role_plain = plain(role);
    weighted.extend(words(role).into_iter().map(|w| (w, 3)));
    for (key, related) in ROLE_WORDS {
        if role_plain.contains(key) {
            weighted.extend(related.iter().map(|w| (w.to_string(), 2)));
        }
    }
    weighted.extend(files.iter().map(|f| (f.clone(), 2)));
    Candidate { id: id.to_string(), words: weighted }
}

/// Picks who answers a message: the clear best match (or the tied best, two at most), else the lead.
pub fn route(text: &str, candidates: &[Candidate], lead: &str) -> Routed {
    let said = words(text);
    let mut scored: Vec<(u32, &str, Vec<String>)> = candidates
        .iter()
        .map(|c| {
            let mut matched: Vec<String> = Vec::new();
            let mut score = 0;
            for (word, weight) in &c.words {
                if said.contains(word) && !matched.contains(word) {
                    matched.push(word.clone());
                    score += weight;
                }
            }
            (score, c.id.as_str(), matched)
        })
        .collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    let best = scored.first().map_or(0, |s| s.0);
    // One word of a role's vocabulary is enough; nothing at all is not.
    if best < 2 {
        return Routed { ids: vec![lead.to_string()], why: Vec::new(), by_lead: true };
    }
    let top: Vec<&(u32, &str, Vec<String>)> = scored.iter().filter(|s| s.0 == best).take(2).collect();
    Routed { ids: top.iter().map(|s| s.1.to_string()).collect(), why: top.iter().map(|s| (s.1.to_string(), s.2.clone())).collect(), by_lead: false }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team() -> Vec<Candidate> {
        vec![
            candidate("lead", "Lia", "loja-api", "Backend", &["pedidos".into(), "pagamento".into()]),
            candidate("front", "Iris", "loja-web", "Frontend", &["checkout".into(), "carrinho".into()]),
            candidate("qa", "Teo", "loja-testes", "QA", &[]),
        ]
    }

    #[test]
    fn a_message_goes_to_the_member_it_is_about() {
        let routed = route("O botão do checkout está desalinhado na tela do celular", &team(), "lead");
        assert_eq!(routed.ids, vec!["front"]);
        assert!(routed.why[0].1.contains(&"checkout".to_string()) && routed.why[0].1.contains(&"botao".to_string()));
        assert_eq!(route("A rota de pagamento devolve erro 500 na API", &team(), "lead").ids, vec!["lead"]);
        assert_eq!(route("Escreve os testes para esse bug que quebrou", &team(), "lead").ids, vec!["qa"]);
    }

    #[test]
    fn with_no_clear_match_the_lead_answers() {
        let routed = route("Bom dia, como estão as coisas?", &team(), "lead");
        assert_eq!((routed.ids, routed.by_lead), (vec!["lead".to_string()], true));
    }

    #[test]
    fn a_tie_calls_the_tied_members_only() {
        let routed = route("Iris e Teo, olhem isso", &team(), "lead");
        assert_eq!(routed.ids, vec!["front", "qa"]);
    }
}
