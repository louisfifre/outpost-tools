//! The settings panel's "This view only" filter: which settings are listed for each view, with the filter on
//! and off, and that the panel follows a change of view.

use mere_base::Vec2;
use mere_ui::{Id, Theme, Ui};
use preview::settings::SETTINGS;
use preview::text;
use preview::view_filter::{groups_for, visible};
use preview::{FILTER_HELP, PreviewState, ShellIds, build, picture_with};

const GROUP_BIOMES: usize = 0;
const GROUP_RELIEF: usize = 1;
const GROUP_CONTINENTS: usize = 2;
const GROUP_CAVES: usize = 3;
const GROUP_OTHER: usize = 4;

fn listed(only: bool, view: usize) -> Vec<usize> {
    // As a set: the panel lists Tectonic (37) last of its group, so the order is not the index order.
    let mut all: Vec<usize> = visible(only, view).into_iter().flatten().collect();
    all.sort_unstable();
    all
}

fn groups_listed(only: bool, view: usize) -> Vec<usize> {
    let mut g: Vec<usize> = listed(only, view).iter().map(|&i| SETTINGS[i].group).collect();
    g.dedup();
    g.sort_unstable();
    g.dedup();
    g
}

/// The shell at 1400 x 940 with the filter as given and the view pressed; the rows that were built.
fn built(only: bool, view: usize) -> (Vec<usize>, usize, ShellIds) {
    let mut state = PreviewState::for_world(None);
    state.view = view;
    state.view_only = only;
    let (_, ui, ids) = picture_with(Theme::dark(), Vec2::new(1400.0, 940.0), None, state);
    let rows: Vec<usize> = (0..SETTINGS.len()).filter(|&i| ui.rect(ids.rows[i]).is_some()).collect();
    let heads = ids.group_heads.len();
    (rows, heads, ids)
}

#[test]
fn filter_off_lists_every_setting_for_every_view() {
    for view in 0..9 {
        assert_eq!(listed(false, view), (0..43).collect::<Vec<_>>(), "view {view}");
    }
}
#[test]
fn filter_on_lists_the_groups_of_the_view() {
    // The page's `opensFor` (preview.html 880-881), with the ores group left out: it is not built here.
    let expect: [(usize, &[usize]); 9] = [
        (0, &[GROUP_BIOMES, GROUP_CONTINENTS]),
        (1, &[GROUP_RELIEF]),
        (2, &[GROUP_RELIEF]),
        (3, &[GROUP_RELIEF]),
        (4, &[GROUP_BIOMES]),
        (5, &[GROUP_RELIEF, GROUP_CONTINENTS]),
        (6, &[GROUP_BIOMES]),
        (7, &[GROUP_BIOMES]),
        (8, &[GROUP_RELIEF, GROUP_CAVES]),
    ];
    for (view, groups) in expect {
        assert_eq!(groups_listed(true, view), groups, "groups listed for view {view}");
        assert_eq!(groups_for(view), groups, "groups_for view {view}");
        let want: Vec<usize> = (0..43).filter(|&i| groups.contains(&SETTINGS[i].group)).collect();
        assert_eq!(listed(true, view), want, "settings listed for view {view}");
    }
}

#[test]
fn the_group_nobody_draws_is_never_listed_with_the_filter_on() {
    for view in 0..9 {
        assert!(!groups_listed(true, view).contains(&GROUP_OTHER), "view {view}");
        assert!(groups_listed(false, view).contains(&GROUP_OTHER), "view {view}");
    }
}

#[test]
fn the_panel_builds_only_the_listed_rows_and_headers() {
    for view in 0..9 {
        let (rows, heads, _) = built(false, view);
        assert_eq!(rows.len(), 43, "filter off, view {view}");
        assert_eq!(heads, 5, "filter off, view {view}");

        let (rows, heads, _) = built(true, view);
        assert_eq!(rows, listed(true, view), "filter on, view {view}");
        assert_eq!(heads, groups_listed(true, view).len(), "a header per group on show, view {view}");
    }
}

#[test]
fn the_filter_names_the_pressed_map() {
    assert_eq!(text::view_only(0), "Only for the Biomes map");
    assert_eq!(text::view_only(5), "Only for the Relief map");
    assert_eq!(text::view_only(6), "Only for the Temperature map");
    assert_eq!(text::view_only(8), "Only for the section");
}

#[test]
fn the_count_shows_only_while_the_filter_is_on_and_matches_the_rows() {
    for view in 0..9 {
        let (_, _, ids) = built(false, view);
        assert_eq!(ids.filter_count, Id::ROOT, "no count with the filter off, view {view}");
        let (rows, _, ids) = built(true, view);
        assert_ne!(ids.filter_count, Id::ROOT, "a count with the filter on, view {view}");
        assert_eq!(text::shown_of(rows.len(), 43), format!("{} of 43", rows.len()));
    }
}

#[test]
fn the_filter_has_its_own_help_that_opens_over_it() {
    let mut state = PreviewState::for_world(None);
    state.help = Some(FILTER_HELP);
    let (_, ui, ids) = picture_with(Theme::dark(), Vec2::new(1400.0, 940.0), None, state);
    let (row, panel) = (ui.rect(ids.view_only).expect("filter row"), ui.rect(ids.help).expect("help panel"));
    assert!((panel.y - row.y).abs() < 0.01, "the panel starts at the filter's row");
    // It takes no room: the list starts right under the filter's row, as with no help open.
    let list = ui.rect(ids.settings_scroll).expect("list");
    let (_, closed, closed_ids) = picture_with(Theme::dark(), Vec2::new(1400.0, 940.0), None, PreviewState::for_world(None));
    assert_eq!(Some(list), closed.rect(closed_ids.settings_scroll), "the list did not move");
    assert_eq!(ids.help_facts, Id::ROOT, "the filter has no values to say");
}

#[test]
fn the_filter_toggle_lines_up_with_the_fields_with_and_without_a_scrollbar() {
    // The Temperature map's list is short (no bar, the rows reach the panel's edge); the Relief map's is long (the
    // bar's gutter on the right of the rows). The filter's row ends at the panel's edge and keeps the same gutter
    // inside it, so its toggle ends where the fields do.
    for (view, bar) in [(6, false), (5, true)] {
        let mut s = Session::new(view, true);
        s.frame();
        let first = s.rows()[0];
        let (row, filter) = (s.ui.rect(s.ids.rows[first]).unwrap(), s.ui.rect(s.ids.view_only).unwrap());
        let gutter = if bar { mere_ui::GUTTER } else { 0.0 };
        assert!((row.x + row.w - (filter.x + filter.w - gutter)).abs() < 0.5, "view {view}: rows end at {}, filter at {}", row.x + row.w, filter.x + filter.w);
        assert_eq!(s.state.settings_bar, bar, "view {view}");
    }
}

/// One shell on one state, built again after each change as the window does.
struct Session {
    ui: Ui,
    state: PreviewState,
    ids: ShellIds,
}

impl Session {
    fn new(view: usize, only: bool) -> Session {
        let mut state = PreviewState::for_world(None);
        state.view = view;
        state.view_only = only;
        let mut s = Session { ui: Ui::new(Theme::dark()), state, ids: ShellIds::default() };
        s.frame();
        s
    }

    fn frame(&mut self) {
        let size = Vec2::new(1400.0, 940.0);
        for _ in 0..2 {
            self.ui.begin_frame(size);
            self.ids = build(&mut self.ui, &mut self.state, size, None, None, None);
            self.ui.end_frame();
        }
    }

    fn rows(&self) -> Vec<usize> {
        (0..SETTINGS.len()).filter(|&i| self.ui.rect(self.ids.rows[i]).is_some()).collect()
    }
}

#[test]
fn changing_the_view_updates_the_list_at_once_and_keeps_the_filter() {
    let mut s = Session::new(1, true);
    assert_eq!(s.rows(), listed(true, 1));
    s.state.view = 6;
    s.frame();
    assert_eq!(s.rows(), listed(true, 6));
    assert!(s.state.view_only, "the filter survives the view change");
    s.state.view = 8;
    s.frame();
    assert_eq!(s.rows(), listed(true, 8));
}

#[test]
fn switching_the_filter_off_brings_every_setting_back() {
    let mut s = Session::new(1, true);
    assert!(s.rows().len() < 43);
    s.state.view_only = false;
    s.frame();
    assert_eq!(s.rows().len(), 43);
}

#[test]
fn a_new_cut_of_the_list_starts_at_the_top_but_a_view_change_with_the_filter_off_keeps_the_scroll() {
    let mut s = Session::new(0, false);
    s.state.settings_y = 60.0;
    s.state.view = 5;
    s.frame();
    assert!(s.state.settings_y > 0.0, "the whole list does not change with the view, so the scroll stays");
    s.state.view_only = true;
    s.frame();
    assert_eq!(s.state.settings_y, 0.0, "a shorter list starts at its first group");
    s.state.settings_y = 10.0;
    s.state.view = 0;
    s.frame();
    assert_eq!(s.state.settings_y, 0.0, "another view's list starts at its first group");
}