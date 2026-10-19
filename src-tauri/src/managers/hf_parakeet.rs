//! Parakeet models from Hugging Face: any ONNX export laid out like the one
//! Handy ships (istupakov/parakeet-tdt-0.6b-v3-onnx) - newer or fine-tuned
//! weights (e.g. the "ultra" exports, other languages) - downloaded file by
//! file into `{models}/hf-<owner>--<name>/` and then read by Handy's own
//! Parakeet engine.
//!
//! Handy's engine needs `encoder-model[.int8].onnx`,
//! `decoder_joint-model[.int8].onnx`, `nemo128.onnx` and `vocab.txt`; the
//! int8 pair is preferred (smaller, what Handy ships), else the full-precision
//! pair with the encoder's `.onnx.data` weights when the repo has them.
//! A folder holds `handy-hf.json` (the repo, the commit and its files) so
//! Handy finds it again after a restart; a download in progress lives in
//! `{models}/.hf-partial/<id>/` until every file is there. Everything is
//! pinned to the commit seen when the model was added, and each large (LFS)
//! file is checked against its SHA-256, so a repo changing meanwhile can
//! never mix two versions.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;

pub const MARKER: &str = "handy-hf.json";

/// The audio preprocessor (128-band mel spectrogram) of the Parakeet TDT
/// 0.6B family, from the repo Handy's own Parakeet comes from. Newer exports
/// in the onnx-asr layout (e.g. the "ultra" ones) leave it out because the
/// onnx-asr library ships it; Handy's engine needs it in the model folder, so
/// for that family it is taken from here, pinned and checked.
pub const BASE_PREPROCESSOR_REPO: &str = "istupakov/parakeet-tdt-0.6b-v3-onnx";
pub const BASE_PREPROCESSOR_REVISION: &str = "8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce";
pub const BASE_PREPROCESSOR_SHA256: &str =
    "a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f";
pub const BASE_PREPROCESSOR_SIZE: u64 = 139_764;

/// Models known to share that preprocessor: the repo id or a declared base
/// model names the Parakeet TDT 0.6B family (v2, v3, ultra).
pub fn uses_base_preprocessor(repo: &str, base_models: &[String]) -> bool {
    std::iter::once(repo)
        .chain(base_models.iter().map(String::as_str))
        .map(str::to_ascii_lowercase)
        .any(|name| name.contains("parakeet-tdt-0.6b") || name.contains("parakeet-ultra"))
}

fn base_preprocessor() -> HfFile {
    HfFile {
        path: "nemo128.onnx".to_string(),
        size: BASE_PREPROCESSOR_SIZE,
        sha256: Some(BASE_PREPROCESSOR_SHA256.to_string()),
        source: Some(format!(
            "{BASE_PREPROCESSOR_REPO}@{BASE_PREPROCESSOR_REVISION}"
        )),
    }
}
/// Where downloads in progress live, inside the models folder.
pub const STAGING_DIR: &str = ".hf-partial";
const HF: &str = "https://huggingface.co";

/// A file in a Hugging Face repo.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct HfFile {
    pub path: String,
    pub size: u64,
    /// SHA-256 of a large (LFS) file, as Hugging Face lists it.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Taken from another repo (`owner/name@commit`) - the shared
    /// preprocessor. `None` = from the model's own repo.
    #[serde(default)]
    pub source: Option<String>,
}

/// One search hit.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct HfModelSummary {
    pub repo: String,
    pub downloads: u64,
}

/// What a repo offers Handy.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct HfRepoCheck {
    pub repo: String,
    pub compatible: bool,
    /// Why not, when it is not.
    pub reason: Option<String>,
    /// The files Handy would download, with their sizes.
    pub files: Vec<HfFile>,
    pub total_mb: u64,
    /// The id the model gets in Handy.
    pub model_id: String,
    /// The audio preprocessor comes from Handy's own Parakeet repo (the
    /// model's repo leaves it out).
    pub preprocessor_from_base: bool,
}

/// The marker written into a Hugging Face model's folder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Marker {
    pub repo: String,
    /// The commit the files come from.
    #[serde(default)]
    pub revision: String,
    pub files: Vec<HfFile>,
}

/// `owner/name` with safe characters only.
pub fn valid_repo(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let ok = |p: &str| {
        !p.is_empty()
            && p.len() <= 96
            && p != "."
            && p != ".."
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    matches!((parts.next(), parts.next(), parts.next()), (Some(a), Some(b), None) if ok(a) && ok(b))
}

/// The model id (and folder name) of a repo: `hf-owner--name`.
pub fn model_id_for(repo: &str) -> String {
    format!("hf-{}", repo.replace('/', "--"))
}

/// The files Handy needs from a repo's file list, or why it cannot use it.
/// `base_preprocessor`: the repo belongs to the Parakeet TDT 0.6B family, so a
/// missing `nemo128.onnx` is taken from Handy's own Parakeet repo.
pub fn pick_files(tree: &[HfFile], base_preprocessor: bool) -> Result<Vec<HfFile>, String> {
    let find = |name: &str| tree.iter().find(|f| f.path == name).cloned();
    let (encoder, decoder) = match (
        find("encoder-model.int8.onnx"),
        find("decoder_joint-model.int8.onnx"),
    ) {
        (Some(e), Some(d)) => (vec![e], d),
        _ => match (find("encoder-model.onnx"), find("decoder_joint-model.onnx")) {
            (Some(e), Some(d)) => {
                let mut encoder = vec![e];
                if let Some(data) = find("encoder-model.onnx.data") {
                    encoder.push(data);
                }
                (encoder, d)
            }
            _ => return Err(
                "no Parakeet TDT encoder/decoder ONNX files (encoder-model, decoder_joint-model)"
                    .to_string(),
            ),
        },
    };
    let preprocessor = match find("nemo128.onnx") {
        Some(own) => own,
        None if base_preprocessor => self::base_preprocessor(),
        None => {
            return Err(
                "no nemo128.onnx (the audio preprocessor), and it is not a Parakeet TDT 0.6B model whose preprocessor Handy has"
                    .to_string(),
            );
        }
    };
    let vocab = find("vocab.txt").ok_or("no vocab.txt")?;
    let mut files = encoder;
    files.push(decoder);
    files.push(preprocessor);
    files.push(vocab);
    if let Some(config) = find("config.json") {
        files.push(config);
    }
    Ok(files)
}

pub fn check_tree(repo: &str, tree: &[HfFile], base_preprocessor: bool) -> HfRepoCheck {
    match pick_files(tree, base_preprocessor) {
        Ok(files) => HfRepoCheck {
            repo: repo.to_string(),
            compatible: true,
            reason: None,
            preprocessor_from_base: files.iter().any(|f| f.source.is_some()),
            total_mb: files
                .iter()
                .map(|f| f.size)
                .sum::<u64>()
                .div_ceil(1024 * 1024),
            files,
            model_id: model_id_for(repo),
        },
        Err(reason) => HfRepoCheck {
            repo: repo.to_string(),
            compatible: false,
            reason: Some(reason),
            files: Vec::new(),
            total_mb: 0,
            model_id: model_id_for(repo),
            preprocessor_from_base: false,
        },
    }
}

/// A download URL for one file of a repo at a commit.
pub fn file_url(repo: &str, revision: &str, path: &str) -> String {
    let revision = if valid_revision(revision) {
        revision
    } else {
        "main"
    };
    format!("{HF}/{repo}/resolve/{revision}/{path}")
}

/// Where one file of a model comes from: its `source` (`owner/name@commit`)
/// when it is borrowed, else the model's repo at the model's commit.
pub fn file_url_for(repo: &str, revision: &str, file: &HfFile) -> Option<String> {
    match &file.source {
        Some(source) => {
            let (src_repo, src_rev) = source.split_once('@')?;
            (valid_repo(src_repo) && valid_revision(src_rev))
                .then(|| file_url(src_repo, src_rev, &file.path))
        }
        None => Some(file_url(repo, revision, &file.path)),
    }
}

/// A commit id (hex).
pub fn valid_revision(revision: &str) -> bool {
    (7..=64).contains(&revision.len()) && revision.chars().all(|c| c.is_ascii_hexdigit())
}

/// The ModelInfo `url` of a Hugging Face model: `hf://owner/name`.
pub fn model_url(repo: &str) -> String {
    format!("hf://{repo}")
}

pub fn repo_from_url(url: &str) -> Option<&str> {
    url.strip_prefix("hf://").filter(|r| valid_repo(r))
}

/// A display name for a repo: its name part.
pub fn display_name(repo: &str) -> String {
    let name = repo.rsplit('/').next().unwrap_or(repo);
    format!("{name} (Hugging Face)")
}

pub fn read_marker(dir: &Path) -> Option<Marker> {
    let text = std::fs::read_to_string(dir.join(MARKER)).ok()?;
    let marker: Marker = serde_json::from_str(&text).ok()?;
    let files_ok = marker
        .files
        .iter()
        .all(|f| !f.path.is_empty() && !f.path.contains(['/', '\\']) && !f.path.contains(".."));
    (valid_repo(&marker.repo) && files_ok).then_some(marker)
}

/// SHA-256 of a file, lowercase hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// A downloaded file is right: its size, and its SHA-256 when listed.
pub fn file_matches(path: &Path, file: &HfFile) -> std::io::Result<bool> {
    if std::fs::metadata(path)?.len() != file.size {
        return Ok(false);
    }
    match &file.sha256 {
        Some(expected) => Ok(sha256_file(path)?.eq_ignore_ascii_case(expected)),
        None => Ok(true),
    }
}

pub fn write_marker(dir: &Path, marker: &Marker) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(marker).map_err(std::io::Error::other)?;
    std::fs::write(dir.join(MARKER), text)
}

/// Parakeet ONNX repos on Hugging Face matching `query` (most downloaded
/// first).
pub async fn search(query: &str) -> anyhow::Result<Vec<HfModelSummary>> {
    let search = format!("parakeet {}", query.trim()).trim().to_string();
    let client = crate::net::client_builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let list: Vec<serde_json::Value> = client
        .get(format!("{HF}/api/models"))
        .query(&[
            ("search", search.as_str()),
            ("filter", "onnx"),
            ("sort", "downloads"),
            ("limit", "40"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(list
        .iter()
        .filter_map(|m| {
            let repo = m.get("id")?.as_str()?.to_string();
            valid_repo(&repo).then(|| HfModelSummary {
                repo,
                downloads: m.get("downloads").and_then(|d| d.as_u64()).unwrap_or(0),
            })
        })
        .collect())
}

/// What `tree` learns about a repo.
pub struct RepoListing {
    pub revision: String,
    pub files: Vec<HfFile>,
    /// Declared base models (model card `base_model`, `base_model:` tags).
    pub base_models: Vec<String>,
}

/// The base models a repo declares.
fn base_models_of(info: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    match info.pointer("/cardData/base_model") {
        Some(serde_json::Value::String(s)) => out.push(s.clone()),
        Some(serde_json::Value::Array(list)) => {
            out.extend(list.iter().filter_map(|v| v.as_str().map(str::to_string)))
        }
        _ => {}
    }
    if let Some(tags) = info.get("tags").and_then(|t| t.as_array()) {
        for tag in tags.iter().filter_map(|t| t.as_str()) {
            if let Some(rest) = tag.strip_prefix("base_model:") {
                // "base_model:finetune:owner/name" or "base_model:owner/name"
                out.push(rest.rsplit(':').next().unwrap_or(rest).to_string());
            }
        }
    }
    out
}

/// The repo's current commit and its files (top level) at that commit.
pub async fn tree(repo: &str) -> anyhow::Result<RepoListing> {
    if !valid_repo(repo) {
        anyhow::bail!("not a Hugging Face repo id (owner/name): {repo}");
    }
    let client = crate::net::client_builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let info: serde_json::Value = client
        .get(format!("{HF}/api/models/{repo}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let revision = info
        .get("sha")
        .and_then(|s| s.as_str())
        .filter(|s| valid_revision(s))
        .ok_or_else(|| anyhow::anyhow!("Hugging Face did not say which commit {repo} is at"))?
        .to_string();
    let list: Vec<serde_json::Value> = client
        .get(format!("{HF}/api/models/{repo}/tree/{revision}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(list
        .iter()
        .filter(|f| f.get("type").and_then(|t| t.as_str()) != Some("directory"))
        .filter_map(|f| {
            Some(HfFile {
                path: f.get("path")?.as_str()?.to_string(),
                size: f.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
                sha256: f
                    .get("lfs")
                    .and_then(|l| l.get("oid"))
                    .and_then(|o| o.as_str())
                    .filter(|o| o.len() == 64 && o.chars().all(|c| c.is_ascii_hexdigit()))
                    .map(str::to_lowercase),
                source: None,
            })
        })
        .collect::<Vec<_>>())
    .map(|files| RepoListing {
        revision,
        files,
        base_models: base_models_of(&info),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(path: &str, size: u64) -> HfFile {
        HfFile {
            path: path.into(),
            size,
            sha256: None,
            source: None,
        }
    }

    #[test]
    fn an_ultra_export_without_its_preprocessor_takes_handys() {
        // Masterx/parakeet-tdt-0.6b-ultra-onnx: onnx-asr layout, no nemo128.onnx.
        let tree = vec![
            f("decoder_joint-model.int8.onnx", 17),
            f("encoder-model.int8.onnx", 621),
            f("vocab.txt", 1),
        ];
        let bases = vec!["moondream/parakeet-ultra".to_string()];
        assert!(uses_base_preprocessor(
            "Masterx/parakeet-tdt-0.6b-ultra-onnx",
            &[]
        ));
        assert!(uses_base_preprocessor("someone/model", &bases));
        assert!(!uses_base_preprocessor("someone/parakeet-ctc-110m", &[]));
        let check = check_tree("Masterx/parakeet-tdt-0.6b-ultra-onnx", &tree, true);
        assert!(check.compatible && check.preprocessor_from_base);
        let pre = check
            .files
            .iter()
            .find(|f| f.path == "nemo128.onnx")
            .unwrap();
        assert_eq!(pre.size, BASE_PREPROCESSOR_SIZE);
        assert_eq!(
            file_url_for("Masterx/x", "abcdef1", pre).unwrap(),
            format!(
                "https://huggingface.co/{BASE_PREPROCESSOR_REPO}/resolve/{BASE_PREPROCESSOR_REVISION}/nemo128.onnx"
            )
        );
        let enc = check
            .files
            .iter()
            .find(|f| f.path == "encoder-model.int8.onnx")
            .unwrap();
        assert_eq!(
            file_url_for("Masterx/x", "abcdef1", enc).unwrap(),
            "https://huggingface.co/Masterx/x/resolve/abcdef1/encoder-model.int8.onnx"
        );
        // Outside the family it is still refused.
        assert!(!check_tree("someone/other", &tree, false).compatible);
        // A bad source never becomes a URL.
        let mut bad = pre.clone();
        bad.source = Some("../x@zz".into());
        assert_eq!(file_url_for("a/b", "abcdef1", &bad), None);
    }

    #[test]
    fn base_models_come_from_the_card_and_tags() {
        let info = serde_json::json!({
            "cardData": {"base_model": ["moondream/parakeet-ultra"]},
            "tags": ["onnx", "base_model:finetune:nvidia/parakeet-tdt-0.6b-v3"]
        });
        assert_eq!(
            base_models_of(&info),
            vec!["moondream/parakeet-ultra", "nvidia/parakeet-tdt-0.6b-v3"]
        );
    }

    #[test]
    fn files_are_checked_by_size_and_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("vocab.txt");
        std::fs::write(&p, b"abc").unwrap();
        let sha = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_file(&p).unwrap(), sha);
        let with = |s: &str| HfFile {
            path: "vocab.txt".into(),
            size: 3,
            sha256: Some(s.into()),
            source: None,
        };
        assert!(file_matches(&p, &with(sha)).unwrap());
        assert!(!file_matches(&p, &with(&"00".repeat(32))).unwrap());
        assert!(!file_matches(&p, &f("vocab.txt", 4)).unwrap());
        assert!(file_matches(&p, &f("vocab.txt", 3)).unwrap());
    }

    #[test]
    fn int8_files_are_preferred() {
        let tree = vec![
            f("README.md", 1),
            f("config.json", 97),
            f("decoder_joint-model.int8.onnx", 18),
            f("decoder_joint-model.onnx", 72),
            f("encoder-model.int8.onnx", 652),
            f("encoder-model.onnx", 41),
            f("encoder-model.onnx.data", 2435),
            f("nemo128.onnx", 1),
            f("vocab.txt", 1),
        ];
        let names: Vec<String> = pick_files(&tree, false)
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert_eq!(
            names,
            [
                "encoder-model.int8.onnx",
                "decoder_joint-model.int8.onnx",
                "nemo128.onnx",
                "vocab.txt",
                "config.json"
            ]
        );
    }

    #[test]
    fn full_precision_brings_its_weights_and_missing_files_are_explained() {
        let tree = vec![
            f("decoder_joint-model.onnx", 72),
            f("encoder-model.onnx", 41),
            f("encoder-model.onnx.data", 2435),
            f("nemo128.onnx", 1),
            f("vocab.txt", 1),
        ];
        let names: Vec<String> = pick_files(&tree, false)
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert_eq!(
            names,
            [
                "encoder-model.onnx",
                "encoder-model.onnx.data",
                "decoder_joint-model.onnx",
                "nemo128.onnx",
                "vocab.txt"
            ]
        );
        let check = check_tree("o/r", &[f("model.onnx", 1)], false);
        assert!(!check.compatible);
        assert!(check.reason.unwrap().contains("encoder"));
        assert!(
            pick_files(
                &[
                    f("encoder-model.onnx", 1),
                    f("decoder_joint-model.onnx", 1),
                    f("vocab.txt", 1)
                ],
                false
            )
            .unwrap_err()
            .contains("nemo128")
        );
    }

    #[test]
    fn repo_ids_are_strict_and_map_to_safe_folder_names() {
        assert!(valid_repo("istupakov/parakeet-tdt-0.6b-v3-onnx"));
        for bad in ["", "a", "a/b/c", "../x", "a/..", "a/b c", "a\\b", "a/b?x"] {
            assert!(!valid_repo(bad), "{bad}");
        }
        assert_eq!(
            model_id_for("Olicorne/parakeet-ultra"),
            "hf-Olicorne--parakeet-ultra"
        );
        assert_eq!(repo_from_url("hf://a/b"), Some("a/b"));
        assert_eq!(repo_from_url("hf://../b"), None);
        assert_eq!(repo_from_url("https://x"), None);
        assert_eq!(
            file_url("a/b", "0123abcd", "vocab.txt"),
            "https://huggingface.co/a/b/resolve/0123abcd/vocab.txt"
        );
        assert_eq!(
            file_url("a/b", "../x", "vocab.txt"),
            "https://huggingface.co/a/b/resolve/main/vocab.txt"
        );
    }

    #[test]
    fn marker_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let marker = Marker {
            repo: "a/b".into(),
            revision: "abc1234".into(),
            files: vec![f("vocab.txt", 3)],
        };
        write_marker(dir.path(), &marker).unwrap();
        assert_eq!(read_marker(dir.path()), Some(marker));
        std::fs::write(dir.path().join(MARKER), r#"{"repo":"../x","files":[]}"#).unwrap();
        assert_eq!(read_marker(dir.path()), None);
        std::fs::write(
            dir.path().join(MARKER),
            r#"{"repo":"a/b","files":[{"path":"../evil","size":1}]}"#,
        )
        .unwrap();
        assert_eq!(read_marker(dir.path()), None);
        std::fs::write(
            dir.path().join(MARKER),
            r#"{"repo":"a/b","files":[{"path":"../evil","size":1}]}"#,
        )
        .unwrap();
        assert_eq!(read_marker(dir.path()), None);
    }
}
