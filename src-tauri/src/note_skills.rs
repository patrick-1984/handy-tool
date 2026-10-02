//! Skills for the "Make note" feature.
//!
//! A skill is a set of instructions for turning a transcript into a note. It
//! can be imported from a single Markdown/text file, a folder, or a `.zip` /
//! `.skill` archive (Claude-style: a folder with `SKILL.md`, optional YAML
//! frontmatter with `name`/`description`, plus reference files). Imported
//! skills are copied into `<app_data_dir>/skills/<id>/` so they persist; the
//! list of skills is derived from that directory (portable-aware: it sits in
//! `data\skills` in portable mode).

use log::{debug, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use tauri::AppHandle;

/// Error codes returned to the frontend, which shows them translated.
pub const ERR_SKILL_MISSING: &str = "skill_missing";
pub const ERR_SKILL_EMPTY: &str = "skill_empty";
pub const ERR_SKILL_UNSUPPORTED: &str = "skill_unsupported";
pub const ERR_SKILL_TOO_LARGE: &str = "skill_too_large";
pub const ERR_SKILL_UNSAFE_PATH: &str = "skill_unsafe_path";

const SKILLS_DIR: &str = "skills";
/// Hidden metadata file written next to the imported skill files.
const META_FILE: &str = ".handy-skill.json";
/// Upper bound for the instructions sent to the model.
const MAX_INSTRUCTIONS_BYTES: usize = 200 * 1024;
/// Upper bound for the files copied or extracted by one import.
const MAX_IMPORT_BYTES: u64 = 50 * 1024 * 1024;
/// Files that are read as instructions; everything else is skipped.
const TEXT_EXTENSIONS: &[&str] = &["md", "markdown", "txt", "json", "yaml", "yml", "csv"];
/// Extensions accepted for single-file imports.
const SINGLE_FILE_EXTENSIONS: &[&str] = &["md", "markdown", "txt"];
const ARCHIVE_EXTENSIONS: &[&str] = &["zip", "skill"];

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct NoteSkill {
    pub id: String,
    /// Frontmatter `name`, else the imported file or folder name.
    pub name: String,
    pub description: Option<String>,
    /// Number of readable text files that make up the instructions.
    pub file_count: u32,
    /// True when the instructions were cut to fit the size cap.
    pub truncated: bool,
}

/// A skill together with the instructions built from its files.
pub struct LoadedSkill {
    pub skill: NoteSkill,
    pub instructions: String,
}

#[derive(Serialize, Deserialize)]
struct SkillMeta {
    original_name: String,
}

#[derive(Default, Debug, PartialEq)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
}

pub fn skills_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = crate::portable::resolve_app_data_dir(app)?.join(SKILLS_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create skills directory: {}", e))?;
    Ok(dir)
}

/// All imported skills, sorted by name.
pub fn list_skills(skills_dir: &Path) -> Vec<NoteSkill> {
    let Ok(read_dir) = fs::read_dir(skills_dir) else {
        return Vec::new();
    };

    let mut skills: Vec<NoteSkill> = read_dir
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().to_string();
            if !is_valid_id(&id) {
                return None;
            }
            match build_skill(&entry.path(), &id) {
                Ok(loaded) => Some(loaded.skill),
                Err(e) => {
                    warn!("Skipping skill '{}': {}", id, e);
                    None
                }
            }
        })
        .collect();

    skills.sort_by_key(|skill| skill.name.to_lowercase());
    skills
}

pub fn load_skill(skills_dir: &Path, id: &str) -> Result<LoadedSkill, String> {
    if !is_valid_id(id) {
        return Err(format!("Invalid skill id: {}", id));
    }
    let root = skills_dir.join(id);
    if !root.is_dir() {
        return Err(ERR_SKILL_MISSING.to_string());
    }
    let loaded = build_skill(&root, id)?;
    if loaded.instructions.is_empty() {
        return Err(ERR_SKILL_EMPTY.to_string());
    }
    Ok(loaded)
}

pub fn delete_skill(skills_dir: &Path, id: &str) -> Result<(), String> {
    if !is_valid_id(id) {
        return Err(format!("Invalid skill id: {}", id));
    }
    let root = skills_dir.join(id);
    if root.is_dir() {
        fs::remove_dir_all(&root).map_err(|e| format!("Failed to remove skill: {}", e))?;
    }
    Ok(())
}

/// Import a skill from a Markdown/text file, a folder, or a `.zip`/`.skill`
/// archive.
pub fn import_skill(skills_dir: &Path, source: &Path) -> Result<NoteSkill, String> {
    let id = new_skill_id(skills_dir);
    let root = skills_dir.join(&id);
    fs::create_dir_all(&root).map_err(|e| format!("Failed to create skill folder: {}", e))?;

    let result = import_into(&root, source).and_then(|original_name| {
        let meta = serde_json::to_string(&SkillMeta { original_name })
            .map_err(|e| format!("Failed to write skill metadata: {}", e))?;
        fs::write(root.join(META_FILE), meta)
            .map_err(|e| format!("Failed to write skill metadata: {}", e))?;
        let loaded = build_skill(&root, &id)?;
        if loaded.instructions.is_empty() {
            return Err(ERR_SKILL_EMPTY.to_string());
        }
        Ok(loaded.skill)
    });

    if result.is_err() {
        if let Err(e) = fs::remove_dir_all(&root) {
            warn!("Failed to clean up partial skill import: {}", e);
        }
    }

    result
}

/// Copy or extract `source` into `root`. Returns the name to fall back on
/// when the skill has no frontmatter name.
fn import_into(root: &Path, source: &Path) -> Result<String, String> {
    let file_name = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let stem = source
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.clone());

    if source.is_dir() {
        let mut budget = MAX_IMPORT_BYTES;
        copy_dir(source, root, &mut budget)?;
        debug!("Imported skill folder {:?}", source);
        return Ok(file_name);
    }

    if !source.is_file() {
        return Err("The selected file does not exist.".to_string());
    }

    let ext = extension_of(source);
    if SINGLE_FILE_EXTENSIONS.contains(&ext.as_str()) {
        let size = fs::metadata(source)
            .map_err(|e| format!("Failed to read skill file: {}", e))?
            .len();
        if size > MAX_IMPORT_BYTES {
            return Err(ERR_SKILL_TOO_LARGE.to_string());
        }
        fs::copy(source, root.join(&file_name))
            .map_err(|e| format!("Failed to copy skill file: {}", e))?;
        debug!("Imported skill file {:?}", source);
        Ok(stem)
    } else if ARCHIVE_EXTENSIONS.contains(&ext.as_str()) {
        extract_zip(source, root)?;
        debug!("Imported skill archive {:?}", source);
        Ok(stem)
    } else {
        Err(ERR_SKILL_UNSUPPORTED.to_string())
    }
}

fn new_skill_id(skills_dir: &Path) -> String {
    let base = format!("skill_{}", chrono::Utc::now().timestamp_millis());
    let mut id = base.clone();
    let mut n = 1;
    while skills_dir.join(&id).exists() {
        id = format!("{}_{}", base, n);
        n += 1;
    }
    id
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn is_text_file(path: &Path) -> bool {
    TEXT_EXTENSIONS.contains(&extension_of(path).as_str())
}

/// Hidden files/folders and macOS archive metadata are never imported.
fn is_ignored_name(name: &str) -> bool {
    name.starts_with('.') || name == "__MACOSX"
}

/// Recursively copy the text files of `source` into `dest`. Symlinks are
/// skipped so a link can't pull in files from outside the folder.
fn copy_dir(source: &Path, dest: &Path, budget: &mut u64) -> Result<(), String> {
    let entries = fs::read_dir(source).map_err(|e| format!("Failed to read folder: {}", e))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if is_ignored_name(&name) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            copy_dir(&path, &dest.join(&name), budget)?;
        } else if file_type.is_file() && is_text_file(&path) {
            let size = entry
                .metadata()
                .map_err(|e| format!("Failed to read {}: {}", name, e))?
                .len();
            if size > *budget {
                return Err(ERR_SKILL_TOO_LARGE.to_string());
            }
            *budget -= size;
            fs::create_dir_all(dest).map_err(|e| format!("Failed to create folder: {}", e))?;
            fs::copy(&path, dest.join(&name))
                .map_err(|e| format!("Failed to copy {}: {}", name, e))?;
        }
    }
    Ok(())
}

/// Extract the text files of a zip archive into `dest`.
///
/// Entry paths go through `enclosed_name` and are rebuilt from their normal
/// components only, so an archive with absolute paths or `..` components is
/// rejected and `./` prefixes are dropped. The total uncompressed size is capped by
/// counting the bytes actually written, not the sizes the archive claims.
/// When every file sits inside one top-level folder, that folder becomes the
/// skill root.
fn extract_zip(source: &Path, dest: &Path) -> Result<(), String> {
    let file = fs::File::open(source).map_err(|e| format!("Failed to open archive: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Failed to read archive: {}", e))?;

    let mut files: Vec<(usize, PathBuf)> = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("Failed to read archive: {}", e))?;
        let Some(path) = entry.enclosed_name() else {
            return Err(ERR_SKILL_UNSAFE_PATH.to_string());
        };
        if entry.is_dir() || entry.is_symlink() {
            continue;
        }
        let Some(path) = normalize_archive_path(&path)? else {
            continue;
        };
        if !is_text_file(&path) {
            continue;
        }
        files.push((index, path));
    }

    let strip = single_top_level_dir(files.iter().map(|(_, path)| path.as_path()));

    let mut budget = MAX_IMPORT_BYTES;
    for (index, path) in files {
        let relative = match &strip {
            Some(prefix) => path.strip_prefix(prefix).unwrap_or(&path).to_path_buf(),
            None => path,
        };
        let target = dest.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create folder: {}", e))?;
        }
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("Failed to read archive: {}", e))?;
        let mut out = fs::File::create(&target)
            .map_err(|e| format!("Failed to extract {}: {}", relative.display(), e))?;
        let written = io::copy(&mut (&mut entry).take(budget + 1), &mut out)
            .map_err(|e| format!("Failed to extract {}: {}", relative.display(), e))?;
        if written > budget {
            return Err(ERR_SKILL_TOO_LARGE.to_string());
        }
        budget -= written;
    }

    Ok(())
}

/// Rebuild an archive path from its normal components. `.` components are
/// dropped and any `..` is rejected, so stripping a top-level folder later can
/// never produce a path that leaves the skill root. Returns `None` for hidden
/// files/folders and macOS metadata, which are skipped.
fn normalize_archive_path(path: &Path) -> Result<Option<PathBuf>, String> {
    let mut normalized = PathBuf::new();
    let mut ignored = false;
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) => {
                ignored |= is_ignored_name(&name.to_string_lossy());
                normalized.push(name);
            }
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ERR_SKILL_UNSAFE_PATH.to_string());
            }
        }
    }
    if ignored || normalized.as_os_str().is_empty() {
        return Ok(None);
    }
    Ok(Some(normalized))
}

/// The shared first path component, if every path has one plus at least one
/// more component below it.
fn single_top_level_dir<'a>(paths: impl Iterator<Item = &'a Path>) -> Option<PathBuf> {
    let mut common: Option<PathBuf> = None;
    for path in paths {
        let mut components = path.components();
        let first = PathBuf::from(components.next()?.as_os_str());
        components.next()?;
        match &common {
            Some(existing) if *existing != first => return None,
            Some(_) => {}
            None => common = Some(first),
        }
    }
    common
}

/// Text files under `root` as `/`-separated relative paths, sorted.
fn collect_text_files(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if is_ignored_name(&name) {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", prefix, name)
            };
            if file_type.is_dir() {
                walk(&entry.path(), &relative, out);
            } else if file_type.is_file() && is_text_file(&entry.path()) {
                out.push(relative);
            }
        }
    }

    let mut files = Vec::new();
    walk(root, "", &mut files);
    files.sort();
    files
}

/// SKILL.md (any case) at the top level, else the only .md/.txt file, else
/// the first top-level .md file.
fn pick_main_file(files: &[String]) -> Option<String> {
    let top_level = |f: &&String| !f.contains('/');

    if let Some(skill_md) = files
        .iter()
        .filter(top_level)
        .find(|f| f.eq_ignore_ascii_case("SKILL.md"))
    {
        return Some(skill_md.clone());
    }

    let prose: Vec<&String> = files
        .iter()
        .filter(|f| {
            let ext = extension_of(Path::new(f.as_str()));
            SINGLE_FILE_EXTENSIONS.contains(&ext.as_str())
        })
        .collect();
    if prose.len() == 1 {
        return Some(prose[0].clone());
    }

    files
        .iter()
        .filter(top_level)
        .find(|f| {
            matches!(
                extension_of(Path::new(f.as_str())).as_str(),
                "md" | "markdown"
            )
        })
        .cloned()
}

/// Read a text file, reading at most `limit` bytes. Files containing NUL
/// bytes are treated as binary and skipped.
fn read_text(path: &Path, limit: usize) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Append `text` to `out` without going over the instructions cap. Returns
/// false when the text had to be cut.
fn push_capped(out: &mut String, text: &str) -> bool {
    let remaining = MAX_INSTRUCTIONS_BYTES.saturating_sub(out.len());
    if text.len() <= remaining {
        out.push_str(text);
        return true;
    }
    let mut end = remaining;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    out.push_str(&text[..end]);
    false
}

fn build_skill(root: &Path, id: &str) -> Result<LoadedSkill, String> {
    let files = collect_text_files(root);
    let main = pick_main_file(&files);

    let mut instructions = String::new();
    let mut truncated = false;
    let mut frontmatter = Frontmatter::default();
    // Only files that add text count; empty or binary files are skipped.
    let mut file_count: u32 = 0;

    if let Some(main) = &main {
        if let Some(text) = read_text(&root.join(main), MAX_INSTRUCTIONS_BYTES) {
            let (parsed, body) = split_frontmatter(&text);
            frontmatter = parsed;
            let body = body.trim();
            if !body.is_empty() {
                file_count += 1;
                truncated = !push_capped(&mut instructions, body);
            }
        }
    }

    for file in files.iter().filter(|f| Some(*f) != main.as_ref()) {
        if truncated {
            break;
        }
        let Some(text) = read_text(&root.join(file), MAX_INSTRUCTIONS_BYTES) else {
            continue;
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        file_count += 1;
        let section = format!("\n\n---\n\nReference file: {}\n\n{}", file, text);
        truncated = !push_capped(&mut instructions, &section);
    }

    let fallback_name = fs::read_to_string(root.join(META_FILE))
        .ok()
        .and_then(|meta| serde_json::from_str::<SkillMeta>(&meta).ok())
        .map(|meta| meta.original_name)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| id.to_string());

    let skill = NoteSkill {
        id: id.to_string(),
        name: frontmatter.name.unwrap_or(fallback_name),
        description: frontmatter.description,
        file_count,
        truncated,
    };

    Ok(LoadedSkill {
        skill,
        instructions: instructions.trim().to_string(),
    })
}

/// Split a leading `---` YAML frontmatter block off a Markdown document.
fn split_frontmatter(text: &str) -> (Frontmatter, &str) {
    let text = text.trim_start_matches('\u{feff}');
    let Some(rest) = text.strip_prefix("---") else {
        return (Frontmatter::default(), text);
    };
    let Some(rest) = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
    else {
        return (Frontmatter::default(), text);
    };

    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if matches!(line.trim_end(), "---" | "...") {
            let yaml = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return (parse_frontmatter(yaml), body);
        }
        offset += line.len();
    }

    (Frontmatter::default(), text)
}

/// Minimal reader for the `name` and `description` keys. Handles plain,
/// quoted, and block (`|` / `>`) values; other keys are ignored.
fn parse_frontmatter(yaml: &str) -> Frontmatter {
    let lines: Vec<&str> = yaml.lines().collect();
    let mut frontmatter = Frontmatter::default();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key != "name" && key != "description" {
            continue;
        }

        let mut value = value.trim().to_string();
        if value.is_empty() || value.starts_with('|') || value.starts_with('>') {
            let mut parts = Vec::new();
            while i < lines.len()
                && (lines[i].starts_with(char::is_whitespace) || lines[i].trim().is_empty())
            {
                let part = lines[i].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                i += 1;
            }
            value = parts.join(" ");
        } else if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            value = value[1..value.len() - 1].to_string();
        }

        let value = value.trim().to_string();
        if value.is_empty() {
            continue;
        }
        if key == "name" {
            frontmatter.name = Some(value);
        } else {
            frontmatter.description = Some(value);
        }
    }

    frontmatter
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, content).expect("write file");
    }

    fn make_zip(path: &Path, entries: &[(&str, &str)]) {
        let file = fs::File::create(path).expect("create zip");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, content) in entries {
            zip.start_file(*name, options).expect("start zip entry");
            zip.write_all(content.as_bytes()).expect("write zip entry");
        }
        zip.finish().expect("finish zip");
    }

    #[test]
    fn frontmatter_name_and_description_are_parsed() {
        let text = "---\nname: Meeting notes\ndescription: >\n  Summarise a\n  meeting.\nlicense: MIT\n---\n# Body\n";
        let (fm, body) = split_frontmatter(text);
        assert_eq!(fm.name.as_deref(), Some("Meeting notes"));
        assert_eq!(fm.description.as_deref(), Some("Summarise a meeting."));
        assert_eq!(body, "# Body\n");
    }

    #[test]
    fn quoted_frontmatter_values_are_unquoted() {
        let (fm, _) = split_frontmatter("---\nname: \"Quoted: name\"\n---\nbody");
        assert_eq!(fm.name.as_deref(), Some("Quoted: name"));
    }

    #[test]
    fn text_without_frontmatter_is_unchanged() {
        let (fm, body) = split_frontmatter("# Title\nText");
        assert_eq!(fm, Frontmatter::default());
        assert_eq!(body, "# Title\nText");
    }

    #[test]
    fn main_file_prefers_skill_md_then_single_prose_file() {
        let files = vec![
            "README.md".to_string(),
            "skill.md".to_string(),
            "refs/a.md".to_string(),
        ];
        assert_eq!(pick_main_file(&files).as_deref(), Some("skill.md"));

        let files = vec!["data.json".to_string(), "refs/guide.txt".to_string()];
        assert_eq!(pick_main_file(&files).as_deref(), Some("refs/guide.txt"));

        let files = vec!["b.md".to_string(), "a.md".to_string()];
        let mut sorted = files.clone();
        sorted.sort();
        assert_eq!(pick_main_file(&sorted).as_deref(), Some("a.md"));
    }

    #[test]
    fn import_single_file_uses_file_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("My Notes.md");
        write(&source, "Write short notes.");
        let skills = tmp.path().join("skills");
        fs::create_dir_all(&skills).expect("skills dir");

        let skill = import_skill(&skills, &source).expect("import");
        assert_eq!(skill.name, "My Notes");
        assert_eq!(skill.file_count, 1);

        let loaded = load_skill(&skills, &skill.id).expect("load");
        assert_eq!(loaded.instructions, "Write short notes.");
    }

    #[test]
    fn import_folder_builds_instructions_with_references() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("notes-skill");
        write(
            &source.join("SKILL.md"),
            "---\nname: Notes\ndescription: Makes notes\n---\nMain instructions",
        );
        write(&source.join("reference/style.md"), "Style guide");
        write(&source.join(".hidden.md"), "secret");
        write(&source.join("image.png"), "not text");
        let skills = tmp.path().join("skills");
        fs::create_dir_all(&skills).expect("skills dir");

        let skill = import_skill(&skills, &source).expect("import");
        assert_eq!(skill.name, "Notes");
        assert_eq!(skill.description.as_deref(), Some("Makes notes"));
        assert_eq!(skill.file_count, 2);
        assert!(!skill.truncated);

        let loaded = load_skill(&skills, &skill.id).expect("load");
        assert!(loaded.instructions.starts_with("Main instructions"));
        assert!(
            loaded
                .instructions
                .contains("Reference file: reference/style.md\n\nStyle guide")
        );
        assert!(!loaded.instructions.contains("secret"));
        assert!(!loaded.instructions.contains("name: Notes"));
    }

    #[test]
    fn import_zip_strips_single_top_level_folder() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let archive = tmp.path().join("pack.skill");
        make_zip(
            &archive,
            &[
                ("pack/SKILL.md", "---\nname: Packed\n---\nDo it"),
                ("pack/refs/a.txt", "Ref A"),
                ("__MACOSX/pack/._SKILL.md", "junk"),
            ],
        );
        let skills = tmp.path().join("skills");
        fs::create_dir_all(&skills).expect("skills dir");

        let skill = import_skill(&skills, &archive).expect("import");
        assert_eq!(skill.name, "Packed");
        assert!(skills.join(&skill.id).join("SKILL.md").is_file());
        assert!(skills.join(&skill.id).join("refs/a.txt").is_file());
        assert!(!skills.join(&skill.id).join("__MACOSX").exists());
    }

    #[test]
    fn import_zip_rejects_path_traversal() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let archive = tmp.path().join("evil.zip");
        make_zip(&archive, &[("../evil.md", "bad"), ("ok.md", "fine")]);
        let skills = tmp.path().join("skills");
        fs::create_dir_all(&skills).expect("skills dir");

        assert!(import_skill(&skills, &archive).is_err());
        assert!(!tmp.path().join("evil.md").exists());
        assert!(list_skills(&skills).is_empty());
    }

    #[test]
    fn import_zip_handles_dot_prefix_and_rejects_parent_dirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let skills = tmp.path().join("skills");
        fs::create_dir_all(&skills).expect("skills dir");

        let archive = tmp.path().join("dotted.zip");
        make_zip(
            &archive,
            &[
                ("./pack/SKILL.md", "Dotted instructions"),
                ("./pack/refs/a.md", "Ref"),
            ],
        );
        let skill = import_skill(&skills, &archive).expect("import ./ archive");
        assert!(skills.join(&skill.id).join("SKILL.md").is_file());
        assert!(skills.join(&skill.id).join("refs/a.md").is_file());

        let archive = tmp.path().join("climb.zip");
        make_zip(
            &archive,
            &[
                ("pack/SKILL.md", "Main"),
                ("pack/../other/SKILL.md", "Escape"),
            ],
        );
        assert!(import_skill(&skills, &archive).is_err());
        assert!(!skills.join("other").exists());
        assert_eq!(list_skills(&skills).len(), 1);
    }

    #[test]
    fn import_without_readable_instructions_fails() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let skills = tmp.path().join("skills");
        fs::create_dir_all(&skills).expect("skills dir");

        let frontmatter_only = tmp.path().join("fm");
        write(
            &frontmatter_only.join("SKILL.md"),
            "---\nname: Empty\ndescription: Nothing\n---\n",
        );
        assert_eq!(
            import_skill(&skills, &frontmatter_only).err().as_deref(),
            Some(ERR_SKILL_EMPTY)
        );

        let binary = tmp.path().join("bin");
        fs::create_dir_all(&binary).expect("bin dir");
        fs::write(binary.join("notes.md"), b"a\0b").expect("write binary");
        assert!(import_skill(&skills, &binary).is_err());
        assert_eq!(fs::read_dir(&skills).expect("read skills").count(), 0);
    }

    #[test]
    fn file_count_skips_unreadable_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("skill_1");
        write(&root.join("SKILL.md"), "Main");
        write(&root.join("empty.md"), "  ");
        fs::write(root.join("binary.txt"), b"x\0y").expect("write binary");

        let loaded = build_skill(&root, "skill_1").expect("build");
        assert_eq!(loaded.skill.file_count, 1);
    }

    #[test]
    fn import_without_text_files_fails_and_cleans_up() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("empty");
        write(&source.join("picture.png"), "binary");
        let skills = tmp.path().join("skills");
        fs::create_dir_all(&skills).expect("skills dir");

        assert!(import_skill(&skills, &source).is_err());
        assert_eq!(fs::read_dir(&skills).expect("read skills").count(), 0);
    }

    #[test]
    fn instructions_are_capped() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("skill_1");
        write(&root.join("SKILL.md"), "Main");
        write(&root.join("big.txt"), &"é".repeat(MAX_INSTRUCTIONS_BYTES));

        let loaded = build_skill(&root, "skill_1").expect("build");
        assert!(loaded.skill.truncated);
        assert!(loaded.instructions.len() <= MAX_INSTRUCTIONS_BYTES);
    }

    #[test]
    fn delete_skill_rejects_invalid_ids() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(delete_skill(tmp.path(), "../x").is_err());
        assert!(load_skill(tmp.path(), "a/b").is_err());
    }
}
