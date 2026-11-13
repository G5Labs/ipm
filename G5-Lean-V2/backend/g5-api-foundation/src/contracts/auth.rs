//! SCM response contracts shared by routes and services.

use serde::Serialize;

/// SCM organization/group projection.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScmOrgDTO {
    pub id: String,
    pub name: String,
    pub avatar_url: Option<String>,
}
