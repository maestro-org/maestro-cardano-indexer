use clap::Parser;

/// Maestro Blockchain API
#[derive(Parser)]
pub enum Args {
    /// Generate openapi docs
    Docs,
    /// Start the API server
    Server,
}

impl Default for Args {
    fn default() -> Self {
        Self::parse()
    }
}
