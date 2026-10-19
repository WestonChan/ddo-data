#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiVendor {
    pub name: String,
    pub wiki_url: String,
    pub location: Option<String>,
    pub pack: Option<String>,
    pub items: Vec<VendorItem>,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VendorItem {
    Named(String),
    Priced(PricedVendorItem),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricedVendorItem {
    pub name: String,
    pub cost: Option<String>,
}

impl VendorItem {
    pub fn name(&self) -> &str {
        match self {
            Self::Named(name) => name,
            Self::Priced(item) => &item.name,
        }
    }

    pub fn cost(&self) -> Option<&str> {
        match self {
            Self::Named(_) => None,
            Self::Priced(item) => item.cost.as_deref(),
        }
    }
}
