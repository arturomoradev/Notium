use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone, Debug)]
pub struct VaultInfoDto {
    pub root: String,
    pub name: String,
    pub notes: usize,
    pub attachments: usize,
    pub folders: usize,
    pub opened: bool,
    pub is_first_run: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct VaultProfileDto {
    pub name: String,
    pub path: String,
    pub active: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VaultBackupEntryDto {
    pub rel_path: String,
    #[serde(with = "bytes_base64")]
    pub bytes: Vec<u8>,
}

/// Backup entry bytes are written as a base64 string. Bundles exported by
/// older versions used a JSON array of numbers, which is still accepted.
mod bytes_base64 {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Encoded {
        Base64(String),
        Legacy(Vec<u8>),
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        match Encoded::deserialize(d)? {
            Encoded::Base64(text) => STANDARD.decode(text).map_err(serde::de::Error::custom),
            Encoded::Legacy(bytes) => Ok(bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_entry_bytes_roundtrip_as_base64() {
        let entry = VaultBackupEntryDto {
            rel_path: "attachments/a.bin".into(),
            bytes: vec![0, 1, 2, 250, 255],
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert_eq!(
            json,
            r#"{"rel_path":"attachments/a.bin","bytes":"AAEC+v8="}"#
        );
        let back: VaultBackupEntryDto = serde_json::from_str(&json).unwrap();
        assert_eq!(back.bytes, entry.bytes);
    }

    #[test]
    fn backup_entry_accepts_legacy_number_array() {
        let legacy = r#"{"rel_path":"notes/tiptap/A.json","bytes":[35,32,65]}"#;
        let entry: VaultBackupEntryDto = serde_json::from_str(legacy).unwrap();
        assert_eq!(entry.bytes, b"# A");
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VaultBackupBundleDto {
    pub vault_name: String,
    pub exported_at: String,
    pub entries: Vec<VaultBackupEntryDto>,
}

#[derive(Serialize, Clone, Debug)]
pub struct FolderDto {
    pub path: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct NoteSummaryDto {
    pub note_id: String,
    pub title: String,
    pub format: String,
    pub path: String,
    pub updated_at: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct NoteContentDto {
    pub note: NoteSummaryDto,
    pub content: String,
    pub rev: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct SaveResultDto {
    pub rev: u64,
    pub note: NoteSummaryDto,
}

#[derive(Serialize, Clone, Debug)]
pub struct BacklinkDto {
    pub source_id: String,
    pub source_title: String,
    pub link_text: String,
    pub kind: String,
    pub snippet: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct OutlinkDto {
    pub target_raw: String,
    pub link_text: String,
    pub kind: String,
    pub is_attachment: bool,
    pub target_id: Option<String>,
    pub target_title: Option<String>,
    pub target_path: Option<String>,
    pub is_resolved: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct LinkReferenceDto {
    pub note_id: String,
    pub title: String,
    pub path: String,
    pub count: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct LinkRepairResultDto {
    pub affected_notes_count: usize,
    pub updated_links_count: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct SearchHitDto {
    pub note_id: String,
    pub title: String,
    pub path: String,
    pub format: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct TitleSuggestionDto {
    pub note_id: String,
    pub title: String,
    pub format: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct TrashItemDto {
    pub trash_id: String,
    pub note_id: Option<String>,
    pub title: String,
    pub deleted_at: u64,
    pub days_remaining: u32,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SettingsDto {
    pub source_mode: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_font_size")]
    pub font_size: u16,
    #[serde(default = "notium_core::vault::default_image_quality")]
    pub image_quality: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct RebuildReportDto {
    pub scanned: usize,
    pub adopted: usize,
    pub updated: usize,
    pub tombstoned: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct AttachmentNoteRefDto {
    pub note_id: String,
    pub title: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct AttachmentItemDto {
    pub att_id: String,
    pub path: String,
    pub filename: String,
    pub file_size: u64,
    pub content_hash: String,
    pub is_orphan: bool,
    pub referencing_notes: Vec<AttachmentNoteRefDto>,
}

fn default_theme() -> String {
    "dark".to_string()
}

fn default_font_size() -> u16 {
    15
}
