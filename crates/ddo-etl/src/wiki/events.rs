#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiEvent {
    pub name: String,
    pub wiki_url: String,
    pub items: Vec<String>,
    pub file_name: String,
}
