use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    InitializeRequest, ListSessionsRequest, SessionInfo, SessionNotification,
};
use agent_client_protocol::{AcpAgent, Agent, ConnectionTo};

pub async fn list_sessions(command: &str, cwd: PathBuf) -> Result<Vec<SessionInfo>, String> {
    let agent = AcpAgent::from_str(command)
        .map_err(|error| format!("Cannot parse harness command {command:?}: {error}"))?;
    log::debug!("Connecting to harness {command:?}");
    agent_client_protocol::Client
        .builder()
        .on_receive_notification(
            async |_: SessionNotification, _| Ok(()),
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
            let initialized = connection
                .send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;
            if initialized
                .agent_capabilities
                .session_capabilities
                .list
                .is_none()
            {
                return Err(agent_client_protocol::Error::new(
                    -32601,
                    "Harness does not advertise session listing (sessionCapabilities.list)",
                ));
            }
            let mut sessions = Vec::new();
            let mut cursor = None;
            let mut seen = HashSet::new();
            loop {
                let response = connection
                    .send_request(ListSessionsRequest::new().cwd(cwd.clone()).cursor(cursor))
                    .block_task()
                    .await?;
                sessions.extend(response.sessions);
                let Some(next) = response.next_cursor else {
                    break;
                };
                if !seen.insert(next.clone()) {
                    return Err(agent_client_protocol::Error::new(
                        -32603,
                        "Harness returned a repeated session/list pagination cursor",
                    ));
                }
                cursor = Some(next);
            }
            Ok(sessions)
        })
        .await
        .map_err(|error| format!("Harness {command:?}: {error}"))
}
