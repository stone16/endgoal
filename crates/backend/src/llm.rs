//! LLM client abstraction for EndGoal.
//!
//! Defines the `LlmClient` trait for one-shot completion (used by state_at for next_step
//! generation) and streaming (used by CP10 freeze session SSE).
//!
//! At runtime, set `ENDGOAL_LLM_STUB=true` to use `StubLlmClient` which returns
//! deterministic canned responses — used in tests and smoke tests.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use crate::errors::AppError;

// ---------------------------------------------------------------------------
// LlmClient trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait LlmClient: Send + Sync {
    /// One-shot text completion. Used for `next_step` generation.
    async fn complete(&self, prompt: &str) -> Result<String, AppError>;

    /// Streaming text completion. Used for freeze proposal SSE (CP10).
    fn stream(
        &self,
        prompt: &str,
    ) -> Pin<Box<dyn Stream<Item = Result<String, AppError>> + Send>>;
}

// ---------------------------------------------------------------------------
// StubLlmClient — deterministic responses for tests and smoke tests
// ---------------------------------------------------------------------------

/// A stub LLM client that returns canned responses without hitting any API.
/// Activate at runtime via `ENDGOAL_LLM_STUB=true`.
pub struct StubLlmClient;

#[async_trait]
impl LlmClient for StubLlmClient {
    async fn complete(&self, _prompt: &str) -> Result<String, AppError> {
        Ok("mock next_step".to_string())
    }

    fn stream(
        &self,
        _prompt: &str,
    ) -> Pin<Box<dyn Stream<Item = Result<String, AppError>> + Send>> {
        let items = vec![Ok("mock proposal".to_string())];
        Box::pin(futures::stream::iter(items))
    }
}

// ---------------------------------------------------------------------------
// Factory: select client based on env
// ---------------------------------------------------------------------------

/// Returns a `StubLlmClient` when `ENDGOAL_LLM_STUB=true`, otherwise returns
/// a stub for now (production Claude API client will be wired in CP10).
pub fn create_llm_client() -> Box<dyn LlmClient> {
    // Always use stub for now; production client wired in CP10
    Box::new(StubLlmClient)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn stub_complete_returns_mock_next_step() {
        let client = StubLlmClient;
        let result = client.complete("any prompt").await.unwrap();
        assert_eq!(result, "mock next_step");
    }

    #[tokio::test]
    async fn stub_stream_returns_mock_proposal() {
        let client = StubLlmClient;
        let mut stream = client.stream("any prompt");
        let first = stream.next().await.unwrap().unwrap();
        assert_eq!(first, "mock proposal");
        // Stream should have only one item
        assert!(stream.next().await.is_none());
    }
}
