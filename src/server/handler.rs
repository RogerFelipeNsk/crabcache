//! Request handler implementation

use crate::protocol::{Command, Response};

/// Request handler
pub struct RequestHandler;

impl RequestHandler {
    /// Handle a command
    pub async fn handle_command(_command: Command) -> crate::Result<Response> {
        // TODO: Implement command handling
        Ok(Response::Pong)
    }
}
