//! Lets friends join through a playit.gg tunnel: installs or updates the
//! agent with winget, links an account and runs its Windows service.
//!
//! Account and tunnel details come from the service itself, over the local
//! pipe its own CLI uses: JSON messages, one per line. Its secret file is not
//! readable by normal users, and the app never needs it.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const QUICK_TIMEOUT: Duration = Duration::from_secs(10);
const IPC_TIMEOUT: Duration = Duration::from_secs(5);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);
const PLAYIT_SERVICE: &str = "playitd";
const PLAYIT_PIPE: &str = r"\\.\pipe\playitd-system";
const PLAYIT_WINGET_ID: &str = "DevelopedMethods.playit";
const TUNNELS_URL: &str = "https://playit.gg/account/tunnels";
const CLAIM_PAGE: &str = "https://playit.gg/claim/";
const PLAYIT_API: &str = "https://api.playit.gg";
const LINK_TIMEOUT: Duration = Duration::from_secs(600);
// The first playit agent able to tunnel Bedrock's NetherNet transport.
const PLAYIT_NETHERNET_VERSION: (u32, u32, u32) = (1, 0, 12);

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceState {
    Missing,
    Stopped,
    Starting,
    Running,
    Stopping,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentPhase {
    NeedsAccount,
    InvalidAccount,
    OverLimit,
    Starting,
    Running,
    Stopping,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tunnel {
    /// What players type, such as `steak-ranks.tun.ply.gg:19932`.
    pub address: String,
    /// Where playit forwards it, such as `127.0.0.1:19132`.
    pub destination: String,
    pub disabled_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub message: String,
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub phase: AgentPhase,
    pub error: Option<String>,
    pub email_verified: Option<bool>,
    pub tunnels: Vec<Tunnel>,
    pub notices: Vec<Notice>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayitStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub supports_nethernet: bool,
    pub service: ServiceState,
    /// None while the service is off: only it can say.
    pub agent: Option<Agent>,
}

fn program_files() -> Vec<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect()
}

fn playit_exe() -> Option<PathBuf> {
    program_files()
        .into_iter()
        .map(|base| base.join("playit_gg").join("bin").join("playit.exe"))
        .find(|path| path.is_file())
}

async fn run(program: &Path, args: &[&str], timeout: Duration) -> Option<(bool, String)> {
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = tokio::time::timeout(timeout, command.output()).await.ok()?.ok()?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Some((output.status.success(), text))
}

fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.trim().split('.').map(|part| part.trim().parse::<u32>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

/// `sc query` labels are translated ("ESTADO"), but the state names are not.
fn parse_service_state(text: &str) -> ServiceState {
    let has = |word: &str| text.split_whitespace().any(|token| token == word);
    if has("RUNNING") {
        ServiceState::Running
    } else if has("START_PENDING") {
        ServiceState::Starting
    } else if has("STOP_PENDING") {
        ServiceState::Stopping
    } else if has("STOPPED") {
        ServiceState::Stopped
    } else {
        ServiceState::Missing
    }
}

async fn playit_service() -> ServiceState {
    let sc = Path::new(r"C:\Windows\System32\sc.exe");
    run(sc, &["query", PLAYIT_SERVICE], QUICK_TIMEOUT)
        .await
        .map_or(ServiceState::Missing, |(_, out)| parse_service_state(&out))
}

/// Sends one request to the service and returns its `response` object.
/// The service greets first with the protocol version it speaks.
#[cfg(windows)]
async fn ipc_request(request: Value) -> Result<Value, String> {
    use tokio::net::windows::named_pipe::ClientOptions;

    let exchange = async {
        let pipe = ClientOptions::new()
            .open(PLAYIT_PIPE)
            .map_err(|e| format!("No se pudo conectar con Playit: {e}"))?;
        let (reader, mut writer) = tokio::io::split(pipe);
        let mut lines = BufReader::new(reader).lines();
        let mut next = async || -> Result<Value, String> {
            let line = lines
                .next_line()
                .await
                .map_err(|e| e.to_string())?
                .ok_or("Playit cerró la conexión.")?;
            serde_json::from_str(&line).map_err(|e| format!("Respuesta de Playit no válida: {e}"))
        };

        let hello = next().await?;
        let version = hello["data"]["protocol"]["ipc_version"].as_u64().ok_or("Playit no saludó.")?;
        let message = json!({ "ipc_version": version, "request_id": 1, "request": request });
        writer
            .write_all(format!("{message}\n").as_bytes())
            .await
            .map_err(|e| e.to_string())?;

        loop {
            let message = next().await?;
            if message["message_kind"] == "response" && message["data"]["request_id"] == 1 {
                return Ok(message["data"]["response"].clone());
            }
        }
    };
    tokio::time::timeout(IPC_TIMEOUT, exchange)
        .await
        .map_err(|_| "Playit tardó demasiado en responder.".to_owned())?
}

#[cfg(not(windows))]
async fn ipc_request(_request: Value) -> Result<Value, String> {
    Err("Playit solo se integra en Windows.".to_owned())
}

/// Reads a `get_state` response: `{"type":"state","data":{"state":...,"data":...}}`.
pub fn parse_state(response: &Value) -> Option<Agent> {
    if response["type"] != "state" {
        return None;
    }
    let lifecycle = &response["data"];
    let details = &lifecycle["data"];
    let phase = match lifecycle["state"].as_str()? {
        "waiting_for_secret" => AgentPhase::NeedsAccount,
        "has_invalid_secret" => AgentPhase::InvalidAccount,
        "disabled_over_limit" => AgentPhase::OverLimit,
        "starting" => AgentPhase::Starting,
        "running" => AgentPhase::Running,
        "stopping" => AgentPhase::Stopping,
        _ => AgentPhase::Error,
    };
    let text = |value: &Value| value.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    let error = (phase != AgentPhase::Running).then(|| text(&details["message"])).flatten();

    let tunnels = details["tunnels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tunnel| {
            Some(Tunnel {
                address: text(&tunnel["display_address"])?,
                destination: text(&tunnel["destination"]).unwrap_or_default(),
                disabled_reason: if tunnel["is_disabled"] == true {
                    Some(text(&tunnel["disabled_reason"]).unwrap_or_else(|| "Desactivado".to_owned()))
                } else {
                    None
                },
            })
        })
        .collect();
    let notices = details["notices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|notice| Some(Notice { message: text(&notice["message"])?, link: text(&notice["resolve_link"]) }))
        .collect();
    let email_verified = details["account_status"].as_str().and_then(|status| match status {
        "verified" => Some(true),
        "email_not_verified" | "guest" => Some(false),
        _ => None,
    });
    Some(Agent { phase, error, email_verified, tunnels, notices })
}

async fn agent() -> Option<Agent> {
    parse_state(&ipc_request(json!({ "type": "get_state" })).await.ok()?)
}

pub async fn playit() -> PlayitStatus {
    let Some(exe) = playit_exe() else {
        return PlayitStatus {
            installed: false,
            version: None,
            supports_nethernet: false,
            service: ServiceState::Missing,
            agent: None,
        };
    };
    let (version, service) = tokio::join!(run(&exe, &["version"], QUICK_TIMEOUT), playit_service());
    let version = version.map(|(_, v)| v.trim().to_owned());
    let supports_nethernet = version
        .as_deref()
        .and_then(parse_version)
        .is_some_and(|v| v >= PLAYIT_NETHERNET_VERSION);
    let agent = if service == ServiceState::Running { agent().await } else { None };
    PlayitStatus { installed: true, version, supports_nethernet, service, agent }
}

/// Installs playit, or updates it if it is already there. Windows may ask
/// the user to approve the installer.
pub async fn install_playit() -> Result<PlayitStatus, String> {
    let action = if playit_exe().is_some() { "upgrade" } else { "install" };
    let args = [
        action,
        "--id",
        PLAYIT_WINGET_ID,
        "--exact",
        "--source",
        "winget",
        "--silent",
        "--accept-package-agreements",
        "--accept-source-agreements",
        "--disable-interactivity",
    ];
    let result = run(Path::new("winget"), &args, INSTALL_TIMEOUT).await;

    // winget reports "no update available" as a failure, so judge by the result.
    let status = playit().await;
    if status.installed && status.supports_nethernet {
        return Ok(status);
    }
    Err(match result {
        None => "No se pudo ejecutar winget. Descarga Playit desde playit.gg/download.".to_owned(),
        Some((_, output)) => {
            let last = output.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or_default();
            format!("No se pudo instalar Playit. {last}")
        }
    })
}

/// Starts or stops the playit service, then waits for it to settle.
pub async fn set_playit_running(running: bool) -> Result<PlayitStatus, String> {
    let exe = playit_exe().ok_or("Playit no está instalado.")?;
    let (_, output) = run(&exe, &[if running { "start" } else { "stop" }], QUICK_TIMEOUT)
        .await
        .ok_or("Playit no respondió.")?;

    let target = if running { ServiceState::Running } else { ServiceState::Stopped };
    for _ in 0..40 {
        if playit_service().await == target {
            // "Running" to Windows can still be "starting" to playit.
            if running {
                wait_until_settled().await;
            }
            return Ok(playit().await);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let action = if running { "encender" } else { "apagar" };
    Err(format!("No se pudo {action} Playit: {}", output.trim()))
}

async fn wait_until_settled() {
    for _ in 0..40 {
        if agent().await.is_some_and(|agent| agent.phase != AgentPhase::Starting) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn ipc_error(response: &Value) -> Option<String> {
    (response["type"] == "error").then(|| {
        response["data"]["message"].as_str().unwrap_or("error desconocido").to_owned()
    })
}

/// Playit's own claim codes: 5 random bytes as hex.
fn new_claim_code() -> Result<String, String> {
    let mut bytes = [0u8; 5];
    getrandom::fill(&mut bytes).map_err(|e| format!("No se pudo generar el código: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn is_claim_code(code: &str) -> bool {
    code.len() == 10 && code.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn claim_url(code: &str) -> String {
    format!("{CLAIM_PAGE}{code}")
}

/// Gets the service ready for a new account and returns the claim code
/// whose page the user opens to approve this computer. `replace` unlinks
/// the current account first.
pub async fn start_link(replace: bool) -> Result<String, String> {
    let status = set_playit_running(true).await?;
    let phase = status.agent.as_ref().map(|agent| agent.phase);

    if replace || phase == Some(AgentPhase::InvalidAccount) {
        let response = ipc_request(json!({ "type": "reset_secret" })).await?;
        if let Some(message) = ipc_error(&response) {
            return Err(format!("No se pudo desvincular la cuenta: {message}"));
        }
    } else if phase != Some(AgentPhase::NeedsAccount) {
        return Err("Playit ya tiene una cuenta vinculada.".to_owned());
    }
    new_claim_code()
}

/// `{"status":"success","data":...}`, or the `fail`/`error` reason.
async fn api_call(client: &reqwest::Client, path: &str, body: Value) -> Result<Result<Value, Value>, String> {
    let response = client
        .post(format!("{PLAYIT_API}{path}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("No se pudo contactar con Playit: {e}"))?;
    let value: Value = response.json().await.map_err(|e| format!("Respuesta de Playit no válida: {e}"))?;
    Ok(match value["status"].as_str() {
        Some("success") => Ok(value["data"].clone()),
        _ => Err(value["data"].clone()),
    })
}

/// Waits for the user to approve `code` in the browser, then hands the new
/// secret straight to the service. The secret never leaves this function.
pub async fn finish_link(code: &str) -> Result<PlayitStatus, String> {
    if !is_claim_code(code) {
        return Err("Código de vinculación no válido.".to_owned());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let deadline = tokio::time::Instant::now() + LINK_TIMEOUT;
    let setup = json!({
        "code": code,
        "agent_type": "assignable",
        "version": concat!("MCBedrockControl ", env!("CARGO_PKG_VERSION")),
    });

    loop {
        if tokio::time::Instant::now() > deadline {
            return Err("Se acabó el tiempo para aprobar en Playit. Inténtalo de nuevo.".to_owned());
        }
        match api_call(&client, "/claim/setup", setup.clone()).await {
            Ok(Ok(state)) if state == "UserAccepted" => break,
            Ok(Ok(state)) if state == "UserRejected" => {
                return Err("No se aprobó la vinculación en Playit.".to_owned());
            }
            // Still waiting for the user, or a network hiccup worth retrying.
            _ => tokio::time::sleep(Duration::from_secs(2)).await,
        }
    }

    let secret = loop {
        match api_call(&client, "/claim/exchange", json!({ "code": code })).await? {
            Ok(data) => break data["secret_key"].as_str().ok_or("Playit no entregó la clave.")?.to_owned(),
            Err(reason) if reason == "UserRejected" => return Err("No se aprobó la vinculación en Playit.".to_owned()),
            // Like playit's own CLI, keep retrying "not found" until the deadline.
            Err(reason) if reason == "CodeExpired" => {
                return Err("El código de vinculación caducó. Inténtalo de nuevo.".to_owned());
            }
            Err(_) if tokio::time::Instant::now() < deadline => tokio::time::sleep(Duration::from_secs(2)).await,
            Err(reason) => return Err(format!("Playit no completó la vinculación: {reason}")),
        }
    };

    let response = ipc_request(json!({ "type": "set_secret", "secret": secret })).await?;
    if let Some(message) = ipc_error(&response) {
        return Err(format!("Playit no aceptó la cuenta: {message}"));
    }
    wait_until_settled().await;
    Ok(playit().await)
}

pub fn tunnels_url() -> &'static str {
    TUNNELS_URL
}

#[cfg(test)]
mod tests {
    use super::*;

    // A real `get_state` response from playitd 1.2.0.
    const RUNNING: &str = r#"{"type":"state","data":{"state":"running","data":{"version":"1.2.0",
        "tunnels":[{"display_address":"steak-ranks.tun.ply.gg:19932","destination":"127.0.0.1:19132",
        "is_disabled":false,"disabled_reason":null}],"pending_tunnels":[],
        "notices":[{"priority":"High","message":"Please verify your email address","resolve_link":null}],
        "account_status":"email_not_verified","agent_id":"2cdc","login_link":null,"start_time":1}}}"#;

    #[test]
    fn reads_tunnels_and_notices_from_a_running_agent() {
        let agent = parse_state(&serde_json::from_str(RUNNING).unwrap()).unwrap();
        assert_eq!(agent.phase, AgentPhase::Running);
        assert_eq!(agent.email_verified, Some(false));
        assert_eq!(
            agent.tunnels,
            [Tunnel {
                address: "steak-ranks.tun.ply.gg:19932".into(),
                destination: "127.0.0.1:19132".into(),
                disabled_reason: None,
            }],
        );
        assert_eq!(agent.notices[0].message, "Please verify your email address");
        assert_eq!(agent.notices[0].link, None);
    }

    #[test]
    fn reads_the_states_without_an_agent() {
        let parse = |json: &str| parse_state(&serde_json::from_str(json).unwrap()).unwrap();
        assert_eq!(parse(r#"{"type":"state","data":{"state":"waiting_for_secret"}}"#).phase, AgentPhase::NeedsAccount);

        let invalid = parse(
            r#"{"type":"state","data":{"state":"has_invalid_secret","data":{"code":"InvalidSecret","message":"secret rejected","retryable":false}}}"#,
        );
        assert_eq!(invalid.phase, AgentPhase::InvalidAccount);
        assert_eq!(invalid.error.as_deref(), Some("secret rejected"));
        assert!(invalid.tunnels.is_empty());
    }

    #[test]
    fn marks_disabled_tunnels() {
        let json = r#"{"type":"state","data":{"state":"running","data":{"tunnels":[
            {"display_address":"a.ply.gg:1","destination":"127.0.0.1:19132","is_disabled":true,"disabled_reason":null}]}}}"#;
        let agent = parse_state(&serde_json::from_str(json).unwrap()).unwrap();
        assert_eq!(agent.tunnels[0].disabled_reason.as_deref(), Some("Desactivado"));
    }

    #[test]
    fn claim_codes_match_playits_format() {
        let code = new_claim_code().unwrap();
        assert!(is_claim_code(&code), "{code}");
        assert_ne!(code, new_claim_code().unwrap());
        assert_eq!(claim_url("0a1b2c3d4e"), "https://playit.gg/claim/0a1b2c3d4e");
        for bad in ["", "0a1b2c3d4", "0a1b2c3d4eff", "zzzzzzzzzz", "../../x12"] {
            assert!(!is_claim_code(bad), "{bad}");
        }
    }

    #[test]
    fn ignores_responses_that_are_not_state() {
        assert_eq!(parse_state(&json!({ "type": "error", "data": {} })), None);
    }

    #[test]
    fn reads_service_state_from_translated_sc_output() {
        let spanish = "NOMBRE_DE_SERVICIO: playitd\n  TIPO : 10  WIN32_OWN_PROCESS\n  ESTADO : 1  STOPPED\n";
        assert_eq!(parse_service_state(spanish), ServiceState::Stopped);
        assert_eq!(parse_service_state("STATE : 4  RUNNING"), ServiceState::Running);
        assert_eq!(parse_service_state("STATE : 2  START_PENDING"), ServiceState::Starting);
        assert_eq!(
            parse_service_state("[SC] EnumQueryServicesStatus:OpenService ERROR 1060"),
            ServiceState::Missing,
        );
    }

    #[test]
    fn only_recent_playit_versions_tunnel_nethernet() {
        let supports = |v: &str| parse_version(v).is_some_and(|v| v >= PLAYIT_NETHERNET_VERSION);
        assert!(!supports("1.0.10"));
        assert!(supports("1.0.12"));
        assert!(supports("1.2.0\r\n"));
        assert!(supports("2.0.0"));
        assert!(!supports("raro"));
    }
}
