//! Switching a running Claude session live: type `/model` and `/effort` into it,
//! wait for Claude to save each value, put the user's saved default back so only
//! that session changed, and hold the permission that lets PADE's own model
//! switch skip Claude's confirmation.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{
    read, read_settings, saved_effort, settings_file, state_directory, ModelChoice, MODEL_KEY,
    MODEL_SETTINGS_KEY,
};
use crate::agents::running_model::state::record_choice;

const SWITCH_KEYS: [&str; 2] = [MODEL_KEY, MODEL_SETTINGS_KEY];

/// How long a switch permission PADE grants stays good - the switch it was
/// granted for lands well within this.
const SWITCH_PERMISSION_LIFETIME: Duration = Duration::from_secs(15);
const SWITCH_PERMISSION_SUFFIX: &str = ".switch-permission";

/// Ctrl+S: Claude stashes the draft in its input box and brings it back once
/// the next command has run - so a switch never mixes into half a prompt.
const STASH_DRAFT: &str = "\u{13}";
const SUBMIT: &str = "\r";
/// The pause between keystroke groups, so Claude reads typing as typing - a
/// command and its Enter sent as one burst arrive as a paste, whose Enter is a
/// newline rather than a submit.
const KEYSTROKE_SETTLE: Duration = Duration::from_millis(150);
/// How long a `/model` switch may take to land in the settings file. Claude can
/// check the account's access to the model first (a network round trip; Fable
/// measured past 1.5 s), so this is generous - polling returns the moment it lands.
const MODEL_LANDING: Duration = Duration::from_secs(8);
/// How long an `/effort` switch may take to land before PADE takes it to be
/// sitting on Claude's "Change effort level?" dialog and accepts it. Long
/// enough that a slow save is never mistaken for the dialog: an Enter sent
/// with no dialog up would reach the draft Claude restores.
const EFFORT_LANDING: Duration = Duration::from_secs(4);
/// How long the settings file must go unwritten before PADE restores it.
const SETTINGS_QUIET: Duration = Duration::from_millis(300);
const SETTINGS_QUIET_LIMIT: Duration = Duration::from_secs(3);
const LANDING_POLL: Duration = Duration::from_millis(50);

/// Types a string into the session's terminal, as if from its keyboard.
type TypeKeys<'a> = &'a dyn Fn(&str) -> Result<(), String>;

/// Switch a running session live, as if typed: stash the draft, `/model`,
/// then stash again and `/effort`. Each step is confirmed by Claude saving the
/// value - never by reading the screen - and the user's saved default is put
/// back afterwards, so only this session changes. Works mid-turn: the next
/// request already uses the new model (verified on 2.1.292).
pub(in crate::agents::running_model) fn apply_live(
    conversation_id: &str,
    choice: &ModelChoice,
    type_keys: TypeKeys,
) -> Result<(), String> {
    let settings = settings_file().ok_or("no Claude settings directory")?;
    let snapshot = std::fs::read_to_string(&settings).ok();
    let outcome = switch_by_commands(conversation_id, choice, type_keys);
    // Claude can write the file more than once per command; restoring between
    // two of its writes would let the second put the switch back.
    wait_for_quiet_settings();
    restore_saved_defaults(&settings, snapshot.as_deref())?;
    let applied = outcome?;
    // Newer than the default just restored, so the tab shows what now runs.
    record_choice(conversation_id, &applied);
    Ok(())
}

/// Whether PADE granted this session's pending switch. Every ask during the
/// switch gets the same answer; PADE revokes the permission once it landed.
pub(in crate::agents::running_model) fn has_switch_permission(conversation_id: &str) -> bool {
    let Some(path) = switch_permission_file(conversation_id) else {
        return false;
    };
    std::fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|granted| SystemTime::now().duration_since(granted).ok())
        .is_some_and(|age| age <= SWITCH_PERMISSION_LIFETIME)
}

pub(super) fn switch_by_commands(
    conversation_id: &str,
    choice: &ModelChoice,
    type_keys: TypeKeys,
) -> Result<ModelChoice, String> {
    let mut current_model = read(conversation_id).map(|running| running.model);
    if let Some(model) = &choice.model {
        switch_model(conversation_id, model, type_keys)?;
        current_model = Some(model.clone());
    }
    if let Some(effort) = &choice.effort {
        let model = current_model
            .as_deref()
            .ok_or("the session's model isn't known yet")?;
        switch_effort(model, effort, type_keys)?;
    }
    Ok(ModelChoice {
        model: current_model,
        effort: choice.effort.clone(),
    })
}

fn switch_model(conversation_id: &str, model: &str, type_keys: TypeKeys) -> Result<(), String> {
    let permission = SwitchPermission::grant(conversation_id);
    let typed_at = SystemTime::now();
    run_command(type_keys, &format!("/model {model}"))?;
    // Saved SINCE typing, not merely equal: switching to the model a session
    // already runs matches before Claude has written anything.
    let has_landed = wait_until(MODEL_LANDING, || {
        if let Some(permission) = &permission {
            permission.keep();
        }
        is_settings_saved_since(typed_at) && saved_model().as_deref() == Some(model)
    });
    drop(permission);
    if !has_landed {
        return Err(format!("Claude didn't switch to {model}"));
    }
    Ok(())
}

/// `model` is the one the session runs now - `/effort` saves its level per model.
fn switch_effort(model: &str, effort: &str, type_keys: TypeKeys) -> Result<(), String> {
    let typed_at = SystemTime::now();
    let is_effort_saved = || {
        is_settings_saved_since(typed_at)
            && saved_effort(model).map(|saved| saved.value).as_deref() == Some(effort)
    };
    run_command(type_keys, &format!("/effort {effort}"))?;
    // No hook can pre-answer Claude's "Change effort level?" dialog; a level
    // that hasn't landed by now is sitting on it, and Enter accepts it.
    let has_landed = wait_until(EFFORT_LANDING, is_effort_saved) || {
        type_keys(SUBMIT)?;
        wait_until(EFFORT_LANDING, is_effort_saved)
    };
    if !has_landed {
        return Err(format!("Claude didn't switch to {effort} effort"));
    }
    Ok(())
}

fn run_command(type_keys: TypeKeys, command: &str) -> Result<(), String> {
    type_keys(STASH_DRAFT)?;
    std::thread::sleep(KEYSTROKE_SETTLE);
    type_keys(command)?;
    std::thread::sleep(KEYSTROKE_SETTLE);
    type_keys(SUBMIT)
}

fn settings_modified_at() -> Option<SystemTime> {
    std::fs::metadata(settings_file()?).ok()?.modified().ok()
}

fn is_settings_saved_since(moment: SystemTime) -> bool {
    settings_modified_at().is_some_and(|modified| modified > moment)
}

/// Wait until the settings file has gone `SETTINGS_QUIET` without a write
/// (at most `SETTINGS_QUIET_LIMIT`).
fn wait_for_quiet_settings() {
    let deadline = std::time::Instant::now() + SETTINGS_QUIET_LIMIT;
    let mut last_seen = settings_modified_at();
    while std::time::Instant::now() < deadline {
        std::thread::sleep(SETTINGS_QUIET);
        let now_seen = settings_modified_at();
        if now_seen == last_seen {
            return;
        }
        last_seen = now_seen;
    }
}

/// Poll `has_landed` until it holds or `within` runs out.
fn wait_until(within: Duration, has_landed: impl Fn() -> bool) -> bool {
    let deadline = std::time::Instant::now() + within;
    while std::time::Instant::now() < deadline {
        if has_landed() {
            return true;
        }
        std::thread::sleep(LANDING_POLL);
    }
    has_landed()
}

fn saved_model() -> Option<String> {
    let settings = read_settings()?;
    Some(settings.get(MODEL_KEY)?.as_str()?.to_string())
}

/// Put back the default `/model` and `/effort` just saved over - the keys they
/// write, `model` and `modelSettings`. When nothing else changed meanwhile the
/// file goes back byte for byte; otherwise only those two keys are reverted.
fn restore_saved_defaults(settings: &Path, snapshot: Option<&str>) -> Result<(), String> {
    let Some(snapshot) = snapshot else {
        return Ok(());
    };
    let Ok(current) = std::fs::read_to_string(settings) else {
        return Ok(());
    };
    if current == snapshot {
        return Ok(());
    }
    let (Ok(before), Ok(mut after)) = (
        serde_json::from_str::<serde_json::Value>(snapshot),
        serde_json::from_str::<serde_json::Value>(&current),
    ) else {
        return Ok(());
    };
    revert_switch_keys(&before, &mut after);
    let restored = restored_text(snapshot, &before, &after)?;
    std::fs::write(settings, restored).map_err(|error| error.to_string())
}

/// The original bytes when reverting left nothing else changed; otherwise the
/// reverted settings, pretty-printed like Claude writes them.
fn restored_text(
    snapshot: &str,
    before: &serde_json::Value,
    after: &serde_json::Value,
) -> Result<String, String> {
    if after == before {
        return Ok(snapshot.to_string());
    }
    let mut text = serde_json::to_string_pretty(after).map_err(|error| error.to_string())?;
    if snapshot.ends_with('\n') {
        text.push('\n');
    }
    Ok(text)
}

/// The keys a model or effort switch writes, set back to `before`'s values
/// (removed where `before` had none).
pub(super) fn revert_switch_keys(before: &serde_json::Value, after: &mut serde_json::Value) {
    let Some(after) = after.as_object_mut() else {
        return;
    };
    for key in SWITCH_KEYS {
        match before.get(key) {
            Some(value) => {
                after.insert(key.to_string(), value.clone());
            }
            None => {
                after.remove(key);
            }
        }
    }
}

fn switch_permission_file(conversation_id: &str) -> Option<PathBuf> {
    Some(state_directory()?.join(format!("{conversation_id}{SWITCH_PERMISSION_SUFFIX}")))
}

/// Lets the model switch PADE is about to type skip Claude's confirmation, for
/// as long as it lives; dropping it revokes the permission.
///
/// Claude can ask more than once for one switch (once per permission mode whose
/// model differs, and again when the model moved while its hooks ran), and a
/// hook sink from a host built before the permission lasted the whole switch
/// deletes it on the first ask - leaving the next ask unanswered, so Claude
/// stops on its "Switch model?" dialog. Hosts run their own build for as long
/// as they hold sessions, so the file is held open without delete sharing,
/// which such a sink's delete can't get past, and granted again if it did.
struct SwitchPermission {
    path: PathBuf,
    held: Option<std::fs::File>,
}

impl SwitchPermission {
    fn grant(conversation_id: &str) -> Option<Self> {
        let path = switch_permission_file(conversation_id)?;
        let held = hold_open(&path);
        if let Err(error) = &held {
            eprintln!("pade: could not grant the model switch: {error}");
        }
        Some(Self {
            path,
            held: held.ok(),
        })
    }

    /// Grant it again if a hook managed to use it up.
    fn keep(&self) {
        if !self.path.exists() {
            let _ = std::fs::write(&self.path, []);
        }
    }
}

impl Drop for SwitchPermission {
    fn drop(&mut self) {
        self.held = None;
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(windows)]
pub(super) fn hold_open(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    /// `FILE_SHARE_READ | FILE_SHARE_WRITE`: everything but `FILE_SHARE_DELETE`.
    const SHARE_ALL_BUT_DELETE: u32 = 0x1 | 0x2;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .share_mode(SHARE_ALL_BUT_DELETE)
        .open(path)
}

#[cfg(not(windows))]
pub(super) fn hold_open(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::File::create(path)
}
