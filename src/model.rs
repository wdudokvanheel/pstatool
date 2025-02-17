use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct ClocHeader {
    pub cloc_url: Option<String>,
    pub cloc_version: Option<String>,
    pub elapsed_seconds: Option<f64>,
    pub n_files: Option<u64>,
    pub n_lines: Option<u64>,
    pub files_per_second: Option<f64>,
    pub lines_per_second: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LanguageStats {
    #[serde(rename = "nFiles")]
    pub n_files: u64,
    pub blank: u64,
    pub comment: u64,
    pub code: u64,
}

impl LanguageStats {
    pub fn total_lines(&self) -> u64 {
        self.blank + self.comment + self.code
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClocData {
    pub header: ClocHeader,
    #[serde(flatten)]
    pub languages: HashMap<String, LanguageStats>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct Project {
    pub github_user: String,
    pub project_name: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct Language {
    pub color: Option<String>,
}
