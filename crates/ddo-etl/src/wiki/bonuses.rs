use ddo_model::enums::BonusType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiBonus {
    pub stat: Option<String>,
    pub group: Option<String>,
    pub effect: Option<String>,
    pub bonus_type: String,
    pub value: i64,
    pub value2: Option<i64>,
}

impl WikiBonus {
    pub fn name(&self) -> &str {
        self.stat.as_deref().or(self.group.as_deref()).or(self.effect.as_deref()).expect("validated bonus target")
    }

    pub fn bonus_type(&self) -> BonusType {
        BonusType::parse(&self.bonus_type).expect("validated bonus_type")
    }
}
