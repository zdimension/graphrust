use crate::http::{download_bytes, download_text};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyFileList {
    manifest: Manifest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    file_refs: Vec<FileRef>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRef {
    filename: String,
    url: String,
}

pub fn download_font(family: &str, filename: &str) -> anyhow::Result<Vec<u8>> {
    let json = download_text(&format!("https://fonts.google.com/download/list?family={family}"))?;
    let file_info: FamilyFileList = serde_json::from_str(&json[5..]).map_err(|e| anyhow::anyhow!(e))?;
    let url =
        &file_info.manifest.file_refs.iter()
            .find(|f| f.filename.ends_with(filename))
            .ok_or_else(|| anyhow::anyhow!("Failed to find font file"))?.url;
    download_bytes(url)
}