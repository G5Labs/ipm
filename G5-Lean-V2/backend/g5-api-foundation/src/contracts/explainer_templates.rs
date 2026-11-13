//! Explainer-template values shared by routes and workspace services.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExplainerTemplateSource {
    Builtin,
    Saved,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainerTemplateDTO {
    pub name: String,
    pub slug: String,
    pub prompt: String,
    pub source: ExplainerTemplateSource,
}

#[derive(Debug, Clone)]
pub struct SavedExplainerTemplate {
    pub slug: String,
    pub name: String,
    pub prompt: String,
}

#[derive(Debug, Clone)]
pub struct ExplainerTemplateInput {
    pub name: String,
    pub slug: String,
    pub prompt: String,
    pub expected_head_commit: Option<String>,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitExplainerTemplateResult {
    pub head_commit: String,
    pub template: ExplainerTemplateDTO,
}

impl From<SavedExplainerTemplate> for ExplainerTemplateDTO {
    fn from(saved: SavedExplainerTemplate) -> Self {
        Self {
            name: saved.name,
            slug: saved.slug,
            prompt: saved.prompt,
            source: ExplainerTemplateSource::Saved,
        }
    }
}
