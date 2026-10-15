use std::path::{Path, PathBuf};
use std::process::Output;
use std::str::FromStr;
use std::time::Duration;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    InitializeRequest, NewSessionRequest, SessionNotification,
};
use agent_client_protocol::{AcpAgent, Agent, ConnectionTo};
use tempfile::TempDir;
use tokio::process::Command;

const LIMIT: Duration = Duration::from_secs(120);

pub struct Isolation {
    root: TempDir,
    cwd: PathBuf,
    opencode: PathBuf,
}

impl Isolation {
    pub fn new() -> Self {
        let opencode = std::env::var_os("UDJAT_TEST_OPENCODE")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                    .map(|dir| dir.join("opencode"))
                    .find(|path| path.is_file())
                    .expect("opencode not found on PATH; set UDJAT_TEST_OPENCODE to the real executable")
            });
        let opencode = std::path::absolute(opencode).expect("absolute opencode path");
        assert!(
            opencode.is_file(),
            "UDJAT_TEST_OPENCODE is not a file: {}",
            opencode.display()
        );
        let root = tempfile::tempdir().unwrap();
        for dir in ["share", "config", "cache", "state", "tmp", "home", "cwd"] {
            std::fs::create_dir(root.path().join(dir)).unwrap();
        }
        let cwd = root.path().join("cwd");
        Self {
            root,
            cwd,
            opencode,
        }
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    // All subprocesses, including the session/new fixture, use this one boundary.
    fn command(&self, executable: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(executable);
        command
            .env_clear()
            .env_remove("OPENCODE_DISABLE_CHANNEL_DB")
            .env_remove("OPENCODE_SESSION_ID")
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("OPENCODE_DB", self.root.path().join("share/oc.db"))
            .env("UDJAT_TEST_OPENCODE", &self.opencode)
            .env("TZ", "UTC")
            .env("OPENCODE_DISABLE_MODELS_FETCH", "1")
            .env("OPENCODE_DISABLE_AUTOUPDATE", "1")
            .env("OPENCODE_DISABLE_DEFAULT_PLUGINS", "1")
            .current_dir(&self.cwd)
            .kill_on_drop(true);
        for (key, dir) in [
            ("XDG_DATA_HOME", "share"),
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_STATE_HOME", "state"),
            ("TMPDIR", "tmp"),
            ("TMP", "tmp"),
            ("TEMP", "tmp"),
            ("HOME", "home"),
        ] {
            command.env(key, self.root.path().join(dir));
        }
        command
    }

    pub async fn list(&self, flags: &[&str]) -> Output {
        let mut command = self.command(env!("CARGO_BIN_EXE_udjat-harness"));
        command
            .args(["list-sessions", "--cwd"])
            .arg(&self.cwd)
            .env("UDJAT_HARNESS_COMMAND", harness_command(&self.opencode))
            .args(flags);
        run(command).await
    }

    pub async fn create_session(&self) -> String {
        let id_file = self.root.path().join("session-id");
        let mut command = self.command(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "lists_a_session_created_through_acp",
                "--nocapture",
            ])
            .env("UDJAT_TEST_CREATE_SESSION", &id_file);
        assert_success(&run(command).await);
        std::fs::read_to_string(id_file).unwrap()
    }
}

fn harness_command(opencode: &Path) -> String {
    format!(
        "'{}' acp",
        opencode
            .to_str()
            .expect("UTF-8 executable path")
            .replace('\'', "'\\''")
    )
}

pub async fn create_session_in_child() {
    let file = PathBuf::from(std::env::var_os("UDJAT_TEST_CREATE_SESSION").unwrap());
    let opencode = PathBuf::from(std::env::var_os("UDJAT_TEST_OPENCODE").unwrap());
    let agent = AcpAgent::from_str(&harness_command(&opencode)).unwrap();
    let id = tokio::time::timeout(
        LIMIT,
        agent_client_protocol::Client
            .builder()
            .on_receive_notification(
                async |_: SessionNotification, _| Ok(()),
                agent_client_protocol::on_receive_notification!(),
            )
            .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
                connection
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                let session = connection
                    .send_request(NewSessionRequest::new(std::env::current_dir().unwrap()))
                    .block_task()
                    .await?;
                Ok(session.session_id.to_string())
            }),
    )
    .await
    .expect("ACP session/new timed out")
    .expect("real isolated ACP session/new failed");
    std::fs::write(file, id).unwrap();
}

async fn run(mut command: Command) -> Output {
    tokio::time::timeout(LIMIT + Duration::from_secs(15), command.output())
        .await
        .expect("isolated subprocess timed out")
        .expect("cannot start isolated subprocess")
}

pub fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "exit: {}; stderr: {}; stdout: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}
