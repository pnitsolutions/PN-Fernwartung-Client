use serde::Serialize;
use std::{
    fs,
    path::PathBuf,
    time::Duration,
};
use uuid::Uuid;

const ENROLL_URL: &str = "https://enroll.pn-it-solutions.eu/api/enroll";
const ENROLL_VALIDATE_URL: &str =
    "https://portal.pn-it-solutions.eu/pn-fernwartung/api/enrollment/validate";

fn debug_log(message: &str) {
    use std::io::Write;

    let dir = state_dir();
    let _ = fs::create_dir_all(&dir);

    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("enrollment-debug.log"))
    {
        let _ = writeln!(file, "{}", message);
    }
}

#[derive(Serialize)]
struct EnrollmentPayload {
    id: String,
    hostname: String,
    password: String,
    client: String,
    enrollment_code: Option<String>,
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

fn enrollment_code_file() -> PathBuf {
    state_dir().join("enrollment-code")
}

fn load_enrollment_code() -> Option<String> {
    let path = enrollment_code_file();

    if !path.exists() {
        return None;
    }

    fs::read_to_string(path)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}
pub fn has_enrollment_code() -> bool {
    load_enrollment_code().is_some()
}
pub fn save_enrollment_code(code: &str) -> Result<(), String> {
    let code = code.trim().to_uppercase();

    if code.is_empty() {
        return Err("Enrollment code is empty".to_owned());
    }

    let dir = state_dir();

    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create enrollment state directory: {e}"))?;

    fs::write(enrollment_code_file(), code)
        .map_err(|e| format!("Failed to store enrollment code: {e}"))?;

    Ok(())
}

#[derive(Serialize)]
struct EnrollmentCodeValidateRequest {
    code: String,
}

#[derive(serde::Deserialize)]
struct EnrollmentCodeValidateResponse {
    valid: bool,
    reason: Option<String>,
}

pub async fn validate_enrollment_code(code: &str) -> Result<(), String> {
    let code = code.trim().to_uppercase();

    if code.len() < 8 || code.len() > 64 {
        return Err("invalid".to_owned());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let response = client
        .post(ENROLL_VALIDATE_URL)
        .json(&EnrollmentCodeValidateRequest { code })
        .send()
        .await
        .map_err(|e| format!("Enrollment code validation request failed: {e}"))?;

    let status = response.status();

    let result = response
        .json::<EnrollmentCodeValidateResponse>()
        .await
        .map_err(|e| format!("Invalid enrollment validation response: {e}"))?;

    if status.is_success() && result.valid {
        return Ok(());
    }

    match result.reason.as_deref() {
        Some("expired") => Err("expired".to_owned()),
        Some("device_limit") => Err("device_limit".to_owned()),
        _ => Err("invalid".to_owned()),
    }
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
    debug_log("ensure_enrolled: start");

    if registered_marker().exists() {
        debug_log("ensure_enrolled: already registered");
        return Ok(());
    }

    let token = option_env!("PN_ENROLLMENT_TOKEN")
        .ok_or_else(|| {
            debug_log("ensure_enrolled: token missing");
            "PN_ENROLLMENT_TOKEN was not set during build".to_owned()
        })?;

    debug_log("ensure_enrolled: token available");
    let enrollment_code = load_enrollment_code().ok_or_else(|| {
    debug_log("ensure_enrolled: enrollment code not available yet");
    "Enrollment code not available yet".to_owned()
})?;

debug_log("ensure_enrolled: enrollment code available");
    let password = load_or_create_password()?;
    debug_log("ensure_enrolled: password loaded/created");

    debug_log("ensure_enrolled: waiting for ID");
    let id = wait_for_id().await?;
    debug_log("ensure_enrolled: ID available");

    let hostname = get_hostname();
    debug_log("ensure_enrolled: hostname available");

    if !hbb_common::config::Config::set_permanent_password(&password) {
        debug_log("ensure_enrolled: set_permanent_password failed");
        return Err("Failed to set permanent password".to_owned());
    }

    debug_log("ensure_enrolled: permanent password set");

    let payload = EnrollmentPayload {
    id,
    hostname,
    password: password.clone(),
    client: "PN-Fernwartung".to_owned(),
    enrollment_code: Some(enrollment_code),
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| {
            debug_log("ensure_enrolled: HTTP client build failed");
            format!("Failed to create HTTP client: {e}")
        })?;

    debug_log("ensure_enrolled: sending request");

    let response = client
        .post(ENROLL_URL)
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            debug_log("ensure_enrolled: request failed");
            format!("Enrollment request failed: {e}")
        })?;

    debug_log(&format!(
        "ensure_enrolled: response HTTP {}",
        response.status()
    ));

    if !response.status().is_success() {
        return Err(format!(
            "Enrollment server returned HTTP {}",
            response.status()
        ));
    }

    fs::write(registered_marker(), b"registered")
        .map_err(|e| {
            debug_log("ensure_enrolled: marker write failed");
            format!("Failed to write enrollment marker: {e}")
        })?;

    let _ = fs::remove_file(pending_password_file());

    debug_log("ensure_enrolled: completed successfully");
    log::info!("PN-Fernwartung enrollment completed successfully");

    Ok(())
}
