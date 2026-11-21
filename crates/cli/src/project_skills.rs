use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

const FOLDERS: [&str; 4] = [".agents/skills", "skills", ".claude/skills", ".codex/skills"];
const MAX_SKILL_BYTES: u64 = 16 * 1024;
const MAX_SELECTED: usize = 3;

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn catalog(root: &Path) -> BTreeMap<String, PathBuf> {
    let mut skills = BTreeMap::new();
    let Ok(root) = root.canonicalize() else { return skills };
    for folder in FOLDERS {
        let base = root.join(folder);
        if base.canonicalize().ok().as_ref() != Some(&base) {
            continue;
        }
        let Ok(entries) = fs::read_dir(&base) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str().filter(|name| valid_name(name)) else { continue };
            let relative = Path::new(folder).join(name).join("SKILL.md");
            let file = root.join(&relative);
            if file.canonicalize().ok().as_ref() == Some(&file)
                && file.metadata().is_ok_and(|meta| meta.is_file() && meta.len() <= MAX_SKILL_BYTES)
            {
                skills.entry(name.to_owned()).or_insert(relative);
            }
        }
    }
    skills
}

pub fn names(root: &Path) -> Vec<String> {
    catalog(root).into_keys().take(100).collect()
}

pub fn instructions(root: &Path, message: &str) -> Result<String> {
    let mut selected = Vec::new();
    let mut fence = None;
    for line in message.lines().map(str::trim) {
        let mark = line.as_bytes().first().copied().filter(|byte| matches!(byte, b'~' | b'\x60'));
        if let Some(mark) = mark {
            let count = line.bytes().take_while(|byte| *byte == mark).count();
            if fence.is_none() && count >= 3 {
                fence = Some((mark, count));
                continue;
            }
            if fence.is_some_and(|(open, size)| mark == open && count >= size && line[count..].is_empty()) {
                fence = None;
                continue;
            }
        }
        if fence.is_none()
            && let Some(name) = line.strip_prefix("/skill ").map(str::trim).filter(|name| valid_name(name))
            && !selected.contains(&name)
        {
            selected.push(name);
            ensure!(selected.len() <= MAX_SELECTED, "select at most {MAX_SELECTED} project skills per message");
        }
    }
    if selected.is_empty() {
        return Ok(String::new());
    }
    let available = catalog(root);
    let mut prompt = String::from("\n\nThe person selected these project skills. Follow them within the current permissions and use paths relative to each SKILL.md for supporting files.\n");
    for name in selected {
        let path = available.get(name).with_context(|| format!("project skill {name} is unavailable in this workspace"))?;
        let mut text = String::new();
        File::open(root.join(path))?.take(MAX_SKILL_BYTES + 1).read_to_string(&mut text)?;
        ensure!(text.len() as u64 <= MAX_SKILL_BYTES && !text.contains('\0'), "invalid project skill {name}");
        prompt.push_str(&format!("\n--- Skill {name} ({}) ---\n{text}\n--- End skill ---\n", path.display()));
    }
    Ok(prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_local_skills_and_loads_only_explicit_selections() {
        let root = std::env::temp_dir().join(format!("raiox-project-skills-{}", std::process::id()));
        for (folder, text) in [(".agents/skills/review", "preferred"), ("skills/review", "duplicate"), ("skills/other", "unselected")] {
            fs::create_dir_all(root.join(folder)).unwrap();
            fs::write(root.join(folder).join("SKILL.md"), text).unwrap();
        }
        assert_eq!(names(&root), ["other", "review"]);
        assert!(instructions(&root, "Mention $review in prose.").unwrap().is_empty());
        assert!(instructions(&root, "~~~text\n/skill missing\n~~~").unwrap().is_empty());
        let prompt = instructions(&root, "Check the change.\n\n/skill review\n/skill review").unwrap();
        assert_eq!(prompt.matches("preferred").count(), 1);
        assert!(!prompt.contains("duplicate") && !prompt.contains("unselected"));
        assert!(instructions(&root, "/skill missing").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
