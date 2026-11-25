//! The pages above Home. Home is the stack's floor; a page is pushed by a press on a line of Home (or on a page's
//! own action) and left by its back action, which pops it. Each page lives in its own file, with the state it keeps
//! (`State`) and a `build` that draws it inside the shell Home gives every page (`LauncherHome::page_frame`: the
//! title line with its back action, the page's width, centred), so that the screens are filled in their own
//! files and touch Home's only to add what they ask of the api (a `Tag`, a `Want`).
//!
//! The pages are placeholders for now: the profile's and the server's show the settings Home used to show in
//! place, until their screens replace them.

pub mod add_mod;
pub mod profile;
pub mod server;
pub mod setup;

/// A page above Home.
#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    /// A profile's page, by its id.
    Profile(String),
    /// Adding a mod to a profile, by its id.
    AddMod(String),
    /// The test server's page.
    Server,
    /// The first start's set-up.
    Setup,
}

/// What each page keeps between frames.
#[derive(Default)]
pub struct Pages {
    pub profile: profile::State,
    pub add_mod: add_mod::State,
    pub server: server::State,
    pub setup: setup::State,
}
