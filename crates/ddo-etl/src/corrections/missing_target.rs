use super::CorrectionValue;

const NO_SUCH_BONUS: &str = "no such bonus";

impl CorrectionValue {
    pub fn missing_target() -> Self {
        Self::Text(NO_SUCH_BONUS.into())
    }

    pub(crate) fn has_matching_target(&self) -> bool {
        self.as_text() != Some(NO_SUCH_BONUS)
    }

    pub(crate) fn has_missing_target(&self, found: &[Self]) -> bool {
        self.has_matching_target() && found.iter().any(|value| !value.has_matching_target())
    }
}
