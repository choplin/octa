use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Label {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LabelGroup {
    pub name: String,
    pub selection: String,
}
