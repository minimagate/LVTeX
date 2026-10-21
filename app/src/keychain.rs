//! The OpenRouter key in the login Keychain, through the system `security` tool. This needs no new
//! dependency, and the key never touches a file or a process argument: `store` sends its command on
//! `security`'s standard input.

use std::{
    io::Write,
    process::{Command, Stdio},
};

const SECURITY: &str = "/usr/bin/security";
const SERVICE: &str = "Mathstral OpenRouter";
const ACCOUNT: &str = "openrouter";

/// The stored key, or None when the Keychain holds none or cannot be read.
pub fn read() -> Option<String> {
    let output = Command::new(SECURITY)
        .args(["find-generic-password", "-s", SERVICE, "-a", ACCOUNT, "-w"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let key = String::from_utf8(output.stdout).ok()?;
    let key = key.trim();
    (!key.is_empty()).then(|| key.to_owned())
}

/// Stores `key` as the Keychain's OpenRouter key, replacing any stored one. `key` must pass
/// `mathstral_mls::key_is_valid`, which keeps it free of quotes, so it can sit in a quoted command.
/// No error message contains the key.
pub fn store(key: &str) -> Result<(), String> {
    let mut child = Command::new(SECURITY)
        .arg("-i")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "The Keychain is not available.".to_owned())?;
    let command = format!("add-generic-password -U -s \"{SERVICE}\" -a {ACCOUNT} -w \"{key}\"\n");
    // Spawned above with piped stdin, so the handle is always present.
    let mut stdin = child.stdin.take().expect("stdin was piped at spawn");
    // A failed write shows up in the read-back below, so its error adds nothing here.
    let _ = stdin.write_all(command.as_bytes());
    drop(stdin);
    let _ = child.wait();
    if read().as_deref() == Some(key) {
        Ok(())
    } else {
        Err("The Keychain did not keep the key.".to_owned())
    }
}

/// Removes the stored key. Ok when none is left afterwards, whether or not one was stored.
pub fn remove() -> Result<(), String> {
    let deleted = Command::new(SECURITY)
        .args(["delete-generic-password", "-s", SERVICE, "-a", ACCOUNT])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if deleted || read().is_none() {
        Ok(())
    } else {
        Err("The Keychain did not remove the key.".to_owned())
    }
}
