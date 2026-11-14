//! PADE's reach into the desktop around it: its own windows and their crash
//! recovery (`window`, `recovery`), the OS shell - file manager, terminal,
//! browser, Explorer's context menu (`os`, `contextmenu`) - the clipboard and
//! video transcoding (`clipboard`, `media`), the file previews built on it that
//! the Change Feed and the commit view share (`preview`), and the outside apps
//! it hands off to or reports into (`design`, `discord`), and whether it runs
//! from its MSIX package (`package`).

pub(crate) mod clipboard;
pub(crate) mod contextmenu;
pub(crate) mod design;
pub(crate) mod desktop;
pub(crate) mod discord;
pub(crate) mod media;
pub(crate) mod package;
pub(crate) mod preview;
pub(crate) mod recovery;
pub(crate) mod window;
pub(crate) mod window_handoff;
