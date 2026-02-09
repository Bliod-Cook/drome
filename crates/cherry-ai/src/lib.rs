mod openai;
mod provider;

pub use openai::OpenAiCompatibleClient;
pub use provider::{
    ChatCompletionRequest, ChatCompletionResponse, ChatTurn, ProviderRegistry,
    RuntimeModelSelection,
};
