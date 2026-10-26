//! Inline text from the captured artifact, never a separately supplied display file.
use super::*;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiniteApplicationPresentation {
    pub review: FiniteApplicationReview,
    pub diff: String,
}

pub(super) fn text_diff(bytes: Vec<u8>) -> Result<String> {
    if bytes.is_empty() || bytes.len() > 131072 {
        return Err(RuntimeError::Limit);
    }
    if bytes.contains(&0)
        || bytes
            .split(|b| *b == b'\n')
            .any(|l| l == b"GIT binary patch")
    {
        return Err(RuntimeError::Invalid("binary finite review diff"));
    }
    String::from_utf8(bytes).map_err(|_| RuntimeError::Invalid("non-UTF8 finite review diff"))
}

impl FiniteClientApplication<'_> {
    pub(super) fn captured_diff(&self, review: &FiniteApplicationReview) -> Result<String> {
        text_diff(workspace::read_artifact(
            &self.runtime.store.root,
            &self.run_id,
            &review.artifact_sha256,
        )?)
    }
    /// Reconstruct the candidate, then expose the exact captured text whose
    /// digest is bound into the review. JSON consumers must render it as data.
    pub async fn presentation(&self) -> Result<FiniteApplicationPresentation> {
        let review = self.review().await?;
        let diff = self.captured_diff(&review)?;
        Ok(FiniteApplicationPresentation { review, diff })
    }
    /// Validate the displayed payload at read time. This grants no authority.
    pub async fn validate_presentation(
        &self,
        packet: &FiniteApplicationPresentation,
    ) -> Result<()> {
        if packet.diff.len() > 131072
            || content_digest(packet.diff.as_bytes()) != packet.review.artifact_sha256
            || &self.presentation().await? != packet
        {
            return Err(RuntimeError::Integrity("finite presentation changed"));
        }
        Ok(())
    }
    /// Validate the displayed text again before the existing explicit approval.
    pub async fn accept_presented(
        &self,
        packet: &FiniteApplicationPresentation,
        approved: &str,
    ) -> Result<()> {
        self.validate_presentation(packet).await?;
        self.accept_and_apply(&packet.review, approved).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_refuses_binary_non_utf8_and_oversize_without_lossy_conversion() {
        for bytes in [
            vec![],
            vec![0],
            vec![255],
            b"diff\nGIT binary patch\n".to_vec(),
            vec![b'a'; 131073],
        ] {
            assert!(text_diff(bytes).is_err());
        }
        let bytes = b"diff\n+\tquote \" and \x1b[31m\n".to_vec();
        let text = text_diff(bytes.clone()).unwrap();
        let json = serde_json::to_string(&text).unwrap();
        assert!(!json.contains('\x1b'));
        assert_eq!(
            serde_json::from_str::<String>(&json).unwrap().as_bytes(),
            bytes
        );
        assert_eq!(text_diff(vec![b'a'; 131072]).unwrap().len(), 131072);
    }
}
