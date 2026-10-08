//! The agents' folders (agent form → Permissions → Folders, `gizai_core::folders`) in the app: the form's check, and
//! the Team Lead's folders, which it only reads in chat.
use std::path::Path;

use gizai_core::folders::{self, Folder, FolderCheck, Places};
use gizai_core::team::{self, Member};

use crate::AppState;

/// What the agent form shows next to each folder: why it's refused, or a warning.
pub fn check(st: &AppState, list: &[Folder]) -> Vec<FolderCheck> {
    folders::check(list, &Places::of(&st.db))
}

/// The Team Lead's folders that are there now, as they are on disk. A chat turn gets them with `--add-dir`: it reads
/// them (Read, Glob and Grep) and has no tool that writes, whatever they are set to.
pub fn lead_folders(st: &AppState, lead: &Member) -> Vec<Folder> {
    if lead.folders.is_empty() {
        return vec![];
    }
    folders::for_run(&lead.folders, &Places::of(&st.db)).0
}

/// For `update_checkout` (GA-44), before it updates a project's linked folder after you said yes in the chat: refused
/// when the Team Lead's folders set that folder (or the closest one that holds it) to read; allowed when it is set to
/// read and change, or not in the list.
pub fn lead_may_update(st: &AppState, folder: &Path) -> Result<(), String> {
    match team::chat_agent(&st.db) {
        Ok(Some(lead)) => folders::may_update(&lead.folders, folder, &Places::of(&st.db)),
        _ => Ok(()),
    }
}
