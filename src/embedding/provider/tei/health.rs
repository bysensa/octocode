use super::Tei;

impl Tei {
    pub(crate) async fn health(&self) -> bool {
        self.infer.health().await
    }
}
