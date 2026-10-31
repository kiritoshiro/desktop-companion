//! Port of `tests/test_team_identity.py`: a team a person can name, colour
//! and understand.
//!
//! Nothing in the Python file exercises `Creature` itself: the team data
//! model is dc-state's (`state/teams.py`, `state/progression.py`), the rest is
//! CreatureManager, rendering or the settings window. The file is kept so
//! the mapping from Python test files stays one to one.
//!
//! not ported: test_profiles_from_a_preset_block (`teams.normalize_teams`,
//! dc-state).
//! not ported: test_ids_are_case_folded (`teams`, dc-state).
//! not ported: test_colors_parse_and_differ (`teams`, dc-state).
//! not ported: test_shipped_colony_shows_two_teams (CreatureManager).
//! not ported: test_team_color_reaches_the_screen (QPainter pixels; dc-paint).
//! not ported: test_no_ring_is_painted_under_a_spider (QPainter pixels;
//! dc-paint).
//! not ported: test_a_base_is_tinted_by_whose_it_is (base painting;
//! dc-paint/dc-world).
//! not ported: test_stances_round_trip_minimally (`normalize_team_stances`,
//! `teams.minimal_stances`; dc-state).
//! not ported: test_settings_window_round_trips_names_and_colours
//! (`ConfigWindow`; dc-ui).
//! not ported: test_the_interface_says_what_hostility_does (`HOSTILITY_NOTE`
//! and README text; dc-state / docs).
