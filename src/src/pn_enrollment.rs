use serde::Serialize;
use std::{
    fs,
    path::PathBuf,
    time::Duration,
};
use uuid::Uuid;

const ENROLL_URL: &str = "https://enroll.pn-it-solutions.eu/api/enroll";

#[derive(Serialize)]
struct EnrollmentPayload {
    id: String,
    hostname: String,
    password: String,
    client: String,
}

fn state_dir() -> PathBuf {
    let base = std::env::var("ProgramData")
        .unwrap_or_else(|_| r"C:\ProgramData".to_owned());

    PathBuf::from(base).join("PN-Fernwartung")
}

fn registered_marker() -> PathBuf {
    state_dir().join("enrollment-registered")
}

fn pending_password_file() -> PathBuf {
    state_dir().join("enrollment-password")
}

fn generate_password() -> String {
    format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn get_hostname() -> String {
    std::env::var("COMPUTERNAME")
        .unwrap_or_else(|_| "unknown".to_owned())
}

fn load_or_create_password() -> Result<String, String> {
    let dir = state_dir();

    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create enrollment state directory: {e}"))?;

    let password_file = pending_password_file();

    if password_file.exists() {
        return fs::read_to_string(&password_file)
            .map(|v| v.trim().to_owned())
            .map_err(|e| format!("Failed to read enrollment password: {e}"));
    }

    let password = generate_password();

    fs::write(&password_file, &password)
        .map_err(|e| format!("Failed to store enrollment password: {e}"))?;

    Ok(password)
}

async fn wait_for_id() -> Result<String, String> {
    for _ in 0..30 {
        let id = hbb_common::config::Config::get_id();

        if !id.trim().is_empty() {
            return Ok(id);
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    Err("RustDesk ID was not available in time".to_owned())
}

pub async fn ensure_enrolled() -> Result<(), String> {
    if registered_marker().exists() {
        return Ok(());
    }

    let token = option_env!("PN_ENROLLMENT_TOKEN")
        .ok_or_else(|| "PN_ENROLLMENT_TOKEN was not set during build".to_owned())?;

    let password = load_or_create_password()?;

    let id = wait_for_id().await?;
    let hostname = get_hostname();

    /*
     * Password setting is added in the next step.
     * We intentionally do not send anything yet.
     */

    let _payload = EnrollmentPayload {
        id,
        hostname,
        password,
        client: "PN-Fernwartung".to_owned(),
    };

    let _client = reqwest::Client::new();

    let _ = ENROLL_URL;
    let _ = token;

    Ok(())
}
