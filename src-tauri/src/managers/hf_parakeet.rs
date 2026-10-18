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
//! A folder holds `handy-hf.json` (the repo and its files) so Handy finds it
//! again after a restart; a download in progress lives in
//! `<folder>.partial-dir` until every file is there.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;

pub const MARKER: &str = "handy-hf.json";
pub const STAGING_SUFFIX: &str = ".partial-dir";
const HF: &str = "https://huggingface.co";

/// A file in a Hugging Face repo.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct HfFile {
    pub path: String,
    pub size: u64,
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
}

/// The marker written into a Hugging Face model's folder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Marker {
    pub repo: String,
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
pub fn pick_files(tree: &[HfFile]) -> Result<Vec<HfFile>, String> {
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
    let preprocessor = find("nemo128.onnx").ok_or("no nemo128.onnx (the audio preprocessor)")?;
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

pub fn check_tree(repo: &str, tree: &[HfFile]) -> HfRepoCheck {
    match pick_files(tree) {
        Ok(files) => HfRepoCheck {
            repo: repo.to_string(),
            compatible: true,
            reason: None,
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
        },
    }
}

/// A download URL for one file of a repo.
pub fn file_url(repo: &str, path: &str) -> String {
    format!("{HF}/{repo}/resolve/main/{path}")
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
    valid_repo(&marker.repo).then_some(marker)
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

/// The files of a repo (top level of `main`).
pub async fn tree(repo: &str) -> anyhow::Result<Vec<HfFile>> {
    if !valid_repo(repo) {
        anyhow::bail!("not a Hugging Face repo id (owner/name): {repo}");
    }
    let client = crate::net::client_builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let list: Vec<serde_json::Value> = client
        .get(format!("{HF}/api/models/{repo}/tree/main"))
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
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(path: &str, size: u64) -> HfFile {
        HfFile {
            path: path.into(),
            size,
        }
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
        let names: Vec<String> = pick_files(&tree)
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
        let names: Vec<String> = pick_files(&tree)
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
        let check = check_tree("o/r", &[f("model.onnx", 1)]);
        assert!(!check.compatible);
        assert!(check.reason.unwrap().contains("encoder"));
        assert!(
            pick_files(&[
                f("encoder-model.onnx", 1),
                f("decoder_joint-model.onnx", 1),
                f("vocab.txt", 1)
            ])
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
            file_url("a/b", "vocab.txt"),
            "https://huggingface.co/a/b/resolve/main/vocab.txt"
        );
    }

    #[test]
    fn marker_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let marker = Marker {
            repo: "a/b".into(),
            files: vec![f("vocab.txt", 3)],
        };
        write_marker(dir.path(), &marker).unwrap();
        assert_eq!(read_marker(dir.path()), Some(marker));
        std::fs::write(dir.path().join(MARKER), r#"{"repo":"../x","files":[]}"#).unwrap();
        assert_eq!(read_marker(dir.path()), None);
    }
}
