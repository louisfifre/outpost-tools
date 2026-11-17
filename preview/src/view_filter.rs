//! The settings panel's "This view only" filter: which groups of settings relate to which view.
//!
//! The page's `opensFor` table (preview.html lines 880-881) says which groups a view scrolls the list to; the
//! native panel uses the same relation to hide the groups that do not move what the view shows. It is data in
//! one place: change a view's groups here and nowhere else.
//!
//! Groups are named by their index in `text::GROUPS`: 0 biomes and climate, 1 relief, 2 continents and
//! oceans, 3 caves, 4 "Not drawn here". The page's `ores` group is not built (the native panel has no ores
//! settings), so the section view lists caves and relief only. Group 4 relates to no view: it is what no view
//! draws, so with the filter on it is always hidden.

use crate::settings::grouped;

/// The groups each view relates to, by `PreviewState::view` (0 to 4 the strip's views, 5 the section).
/// Page: biomes ['biomes','continents'], flat ['relief'], height ['relief','continents'], temperature
/// ['biomes'], structures ['biomes'], section ['ores','caves','relief'].
const OPENS_FOR: [&[usize]; 6] = [
    &[0, 2],    // Biomes
    &[1],       // Flat
    &[1, 2],    // Height (shown as "Relief")
    &[0],       // Temperature
    &[0],       // Structures
    &[1, 3],    // Section (ores: not built)
];

/// The groups that relate to `view`; none for a view that does not exist.
pub fn groups_for(view: usize) -> &'static [usize] {
    OPENS_FOR.get(view).copied().unwrap_or(&[])
}

/// The settings to list, as indices into `SETTINGS` per group (`grouped()` order). With `only` off, all of
/// them; with it on, the groups of `view` keep theirs and the others are empty, so the panel builds no header
/// for them.
pub fn visible(only: bool, view: usize) -> [Vec<usize>; 5] {
    let mut all = grouped();
    if only {
        let keep = groups_for(view);
        for (g, members) in all.iter_mut().enumerate() {
            if !keep.contains(&g) {
                members.clear();
            }
        }
    }
    all
}

/// What the list depends on: None for the whole list, else the view it is cut to. A change of it resets the
/// panel's scroll, because the old offset points into a list that is no longer there.
pub fn list_key(only: bool, view: usize) -> Option<usize> {
    only.then_some(view)
}
