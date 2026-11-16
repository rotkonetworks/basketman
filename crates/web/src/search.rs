//! Global search: one box, opened from the nav bar or with Ctrl+K (Cmd+K on
//! a Mac), that finds players, clubs and staff by name from any screen.
//!
//! `search` is pure: it takes the world and the text and returns the ranked
//! rows, so the ranking can be tested without a window. The overlay below it
//! only draws them and turns a choice into a link.

use basketman_core::league::{TeamId, World};
use basketman_core::player::{Player, PlayerId};
use basketman_core::staff::Job;
use eframe::egui::{
    self, Align2, Color32, Context, FontId, Id, Key, Modifiers, Order, Sense, Stroke, Ui, pos2,
    vec2,
};

use crate::app::{App, Tab};
use crate::nav::{Link, follow};
use crate::theme;

/// Most rows shown at once.
pub(crate) const LIMIT: usize = 12;
/// How many clubs and staff may crowd the players out.
const SHARE: usize = 3;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Group {
    Players,
    Clubs,
    Staff,
}

impl Group {
    fn label(self) -> &'static str {
        match self {
            Group::Players => "Players",
            Group::Clubs => "Clubs",
            Group::Staff => "Staff",
        }
    }
}

/// Where a row leads.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Open {
    Player(PlayerId),
    Club(TeamId),
    /// The Staff tab: his own staff, or somebody looking for work.
    OwnStaff,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Hit {
    pub group: Group,
    pub title: String,
    /// Position, age and club; a job and a club; the club's record.
    pub detail: String,
    /// Overall, for players, as the rest of the game shows it.
    pub rating: Option<String>,
    pub open: Open,
}

/// How well `name` matches every word of the query: 0 when the name starts
/// with it, 1 when each word starts a word of the name, 2 when they are only
/// somewhere inside. `None` when a word is missing.
pub(crate) fn rank(name: &str, query: &str) -> Option<u8> {
    let name = name.to_lowercase();
    let query = query.to_lowercase();
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }
    if name.starts_with(query.trim()) {
        return Some(0);
    }
    let mut worst = 1;
    for t in tokens {
        if name
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w.starts_with(t))
        {
            continue;
        }
        if name.contains(t) {
            worst = 2;
        } else {
            return None;
        }
    }
    Some(worst)
}

/// Overall as the user may know it: exact for his own first team, an
/// estimate that narrows with scouting for everybody else.
fn overall_text(world: &World, mine: TeamId, p: &Player) -> String {
    let (lo, hi) = if world.teams[mine].roster.contains(&p.id) {
        (p.overall(), p.overall())
    } else {
        world.scouting.estimate_overall(p)
    };
    if hi - lo < 0.8 {
        format!("{:.0}", (lo + hi) / 2.0)
    } else {
        format!("{lo:.0}-{hi:.0}")
    }
}

fn player_hits(world: &World, mine: TeamId, query: &str) -> Vec<(u8, Hit)> {
    // Where everybody plays, once, instead of once per player.
    let mut club: Vec<Option<(TeamId, bool)>> = vec![None; world.players.len()];
    for t in &world.teams {
        for &p in &t.roster {
            club[p as usize] = Some((t.id, false));
        }
        for &p in &t.reserves {
            club[p as usize] = Some((t.id, true));
        }
    }
    let prospects = world.draft.as_ref().map(|d| &d.pool);
    let mut out = Vec::new();
    for p in &world.players {
        let Some(r) = rank(&p.name, query) else {
            continue;
        };
        let at = match club[p.id as usize] {
            None if p.is_retired() => "Retired".to_string(),
            Some((t, false)) => world.teams[t].name.clone(),
            Some((t, true)) => format!("{} reserves", world.teams[t].name),
            None if prospects.is_some_and(|d| d.contains(&p.id)) => "Draft prospect".to_string(),
            None => "Free agent".to_string(),
        };
        out.push((
            r,
            Hit {
                group: Group::Players,
                title: p.name.clone(),
                detail: format!("{}  {}  {}", p.position.label(), p.age, at),
                rating: Some(overall_text(world, mine, p)),
                open: Open::Player(p.id),
            },
        ));
    }
    out
}

fn club_hits(world: &World, query: &str) -> Vec<(u8, Hit)> {
    let mut out = Vec::new();
    for t in &world.teams {
        if let Some(r) = rank(&t.name, query) {
            let s = &world.standings[t.id];
            out.push((
                r,
                Hit {
                    group: Group::Clubs,
                    title: t.name.clone(),
                    detail: format!("Record {}-{}", s.wins, s.losses),
                    rating: None,
                    open: Open::Club(t.id),
                },
            ));
        }
    }
    out
}

fn staff_hits(world: &World, mine: TeamId, query: &str) -> Vec<(u8, Hit)> {
    let mut out = Vec::new();
    let mut add = |name: &str, what: String, open: Open| {
        if let Some(r) = rank(name, query) {
            out.push((
                r,
                Hit {
                    group: Group::Staff,
                    title: name.to_string(),
                    detail: what,
                    rating: None,
                    open,
                },
            ));
        }
    };
    for t in &world.teams {
        let open = if t.id == mine {
            Open::OwnStaff
        } else {
            Open::Club(t.id)
        };
        // For the user's club the manager is the user.
        let gm = if t.id == mine {
            "Assistant general manager"
        } else {
            "General manager"
        };
        add(&t.office.gm.name, format!("{gm}, {}", t.name), open);
        for job in Job::ALL {
            if let Some(s) = t.office.staffer(job) {
                add(&s.name, format!("{}, {}", job.label(), t.name), open);
            }
        }
    }
    for s in &world.staff_pool {
        add(
            &s.name,
            "Coach or scout, looking for work".to_string(),
            Open::OwnStaff,
        );
    }
    out
}

/// Everything that matches `query`, at most `LIMIT` rows: players first,
/// then clubs, then staff. Within a group the better match comes first, then
/// the name in alphabetical order; never the rating, which would give away
/// what the user has not scouted. An empty query finds nothing.
pub(crate) fn search(world: &World, mine: TeamId, query: &str) -> Vec<Hit> {
    let sorted = |mut v: Vec<(u8, Hit)>| {
        v.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.title.to_lowercase().cmp(&b.1.title.to_lowercase()))
        });
        v.into_iter().map(|(_, h)| h).collect::<Vec<_>>()
    };
    if query.trim().is_empty() {
        return Vec::new();
    }
    let (players, clubs, staff) = (
        sorted(player_hits(world, mine, query)),
        sorted(club_hits(world, query)),
        sorted(staff_hits(world, mine, query)),
    );
    // Clubs and staff keep a few places each; players get the rest, and
    // whatever they leave unused goes back to the others.
    let (mut c, mut s) = (clubs.len().min(SHARE), staff.len().min(SHARE));
    let p = players.len().min(LIMIT - c - s);
    let mut spare = LIMIT - p - c - s;
    for (taken, all) in [(&mut s, staff.len()), (&mut c, clubs.len())] {
        let more = (all - *taken).min(spare);
        *taken += more;
        spare -= more;
    }
    let mut out: Vec<Hit> = players.into_iter().take(p).collect();
    out.extend(clubs.into_iter().take(c));
    out.extend(staff.into_iter().take(s));
    out
}

/// The overlay's own state.
#[derive(Default)]
pub(crate) struct Search {
    pub(crate) open: bool,
    pub(crate) query: String,
    selected: usize,
}

impl Search {
    pub(crate) fn show(&mut self) {
        self.open = true;
        self.selected = 0;
    }
}

/// A small magnifier in the nav bar. Returns true when clicked.
pub(crate) fn nav_button(ui: &mut Ui) -> bool {
    let h = ui.spacing().interact_size.y.max(28.0);
    let (rect, r) = ui.allocate_exact_size(vec2(h.max(30.0), h), Sense::click());
    let r = r
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Search players, clubs and staff (Ctrl+K, Cmd+K on a Mac)");
    let p = ui.painter();
    if r.hovered() {
        p.rect_filled(rect, 6.0, theme::ACCENT.gamma_multiply(0.12));
    }
    let ink = if r.hovered() {
        theme::ACCENT
    } else {
        theme::TEXT_DIM
    };
    let c = rect.center() + vec2(-1.5, -1.5);
    p.circle_stroke(c, 5.5, Stroke::new(1.6, ink));
    let d = vec2(1.0, 1.0).normalized();
    p.line_segment([c + d * 5.8, c + d * 10.0], Stroke::new(2.0, ink));
    r.clicked()
}

impl App {
    /// Ctrl+K (Cmd+K) opens and closes the search, Escape closes it. Called
    /// before the page is drawn so that no text field gets the key first.
    pub(crate) fn search_keys(&mut self, ctx: &Context) {
        if !self.started {
            return;
        }
        let (toggle, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::K),
                self.search.open && i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if toggle {
            if self.search.open {
                self.search.open = false;
            } else {
                self.search.show();
            }
        }
        if escape {
            self.search.open = false;
        }
    }

    /// The box in the middle of the window, while it is open.
    pub(crate) fn search_overlay(&mut self, ctx: &Context) {
        if !self.started || !self.search.open {
            return;
        }
        // Keys that a text field would take for itself.
        let (down, up, enter) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
            )
        });
        let screen = ctx.content_rect();
        // A dimmed backdrop: clicking outside the box closes it.
        let mut close = false;
        egui::Area::new(Id::unique("search_backdrop"))
            .order(Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, r) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_black_alpha(90));
                close |= r.clicked();
            });

        let mut hits = search(&self.world, self.team, &self.search.query);
        let mut chosen: Option<Open> = None;
        let width = 580.0_f32.min(screen.width() - 32.0);
        egui::Area::new(Id::unique("search_box"))
            .order(Order::Tooltip)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 96.0))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(theme::BG)
                    .stroke(Stroke::new(1.0, theme::LINE_2))
                    .corner_radius(14)
                    .shadow(egui::Shadow {
                        offset: [0, 10],
                        blur: 40,
                        spread: 0,
                        color: Color32::from_black_alpha(80),
                    })
                    .inner_margin(egui::Margin::same(16))
                    .show(ui, |ui| {
                        ui.set_width(width - 32.0);
                        let field = Id::unique("search_field");
                        let before = self.search.query.clone();
                        ui.add(
                            egui::TextEdit::singleline(&mut self.search.query)
                                .id(field)
                                .hint_text("Search players, clubs and staff")
                                .font(FontId::new(17.0, theme::semibold()))
                                .desired_width(f32::INFINITY)
                                .margin(egui::Margin::symmetric(10, 8)),
                        );
                        ui.memory_mut(|m| m.request_focus(field));
                        if self.search.query != before {
                            self.search.selected = 0;
                            hits = search(&self.world, self.team, &self.search.query);
                        }
                        let n = hits.len();
                        if n > 0 {
                            if down {
                                self.search.selected = (self.search.selected + 1) % n;
                            }
                            if up {
                                self.search.selected = (self.search.selected + n - 1) % n;
                            }
                        }
                        self.search.selected = self.search.selected.min(n.saturating_sub(1));
                        ui.add_space(8.0);
                        if hits.is_empty() {
                            let text = if self.search.query.trim().is_empty() {
                                "Type a name."
                            } else {
                                "Nobody by that name."
                            };
                            ui.label(egui::RichText::new(text).color(theme::TEXT_DIM));
                        }
                        let mut group = None;
                        for (i, hit) in hits.iter().enumerate() {
                            if group != Some(hit.group) {
                                group = Some(hit.group);
                                ui.add_space(4.0);
                                ui.label(theme::eyebrow_text(hit.group.label()));
                            }
                            if row(ui, hit, i == self.search.selected) {
                                chosen = Some(hit.open);
                            }
                        }
                        if enter && let Some(hit) = hits.get(self.search.selected) {
                            chosen = Some(hit.open);
                        }
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("Up and Down to move, Enter to open, Esc to close")
                                .size(crate::theme::fs(11.5))
                                .color(theme::TEXT_MUTE),
                        );
                        if let Some(open) = chosen {
                            match open {
                                Open::Player(p) => follow(ui, Link::Player(p)),
                                Open::Club(t) => follow(ui, Link::Club(t)),
                                Open::OwnStaff => {}
                            }
                        }
                    });
            });
        if let Some(open) = chosen {
            if open == Open::OwnStaff {
                self.go(Tab::Staff);
            }
            self.search = Search::default();
            ctx.request_repaint();
        } else if close {
            self.search.open = false;
        }
    }
}

/// One result: title and detail on the left, overall on the right.
/// Returns true when clicked.
fn row(ui: &mut Ui, hit: &Hit, selected: bool) -> bool {
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
    let paint = ui.painter();
    if selected || r.hovered() {
        paint.rect_filled(
            rect,
            8.0,
            theme::ACCENT.gamma_multiply(if selected { 0.16 } else { 0.08 }),
        );
    }
    let title = paint.layout_no_wrap(
        hit.title.clone(),
        FontId::new(14.5, theme::semibold()),
        theme::TEXT,
    );
    let tw = title.size().x;
    let cy = rect.center().y;
    paint.galley(
        pos2(rect.left() + 12.0, cy - title.size().y / 2.0),
        title,
        theme::TEXT,
    );
    let right = if let Some(rating) = &hit.rating {
        let g = paint.layout_no_wrap(
            rating.clone(),
            FontId::new(13.0, theme::mono_semibold()),
            theme::TEXT,
        );
        let x = rect.right() - 12.0 - g.size().x;
        paint.galley(pos2(x, cy - g.size().y / 2.0), g, theme::TEXT);
        x - 10.0
    } else {
        rect.right() - 12.0
    };
    let room = (right - (rect.left() + 12.0 + tw + 14.0)).max(0.0);
    let mut detail = hit.detail.clone();
    let font = FontId::new(12.5, egui::FontFamily::Proportional);
    let mut g = paint.layout_no_wrap(detail.clone(), font.clone(), theme::TEXT_DIM);
    // Shorten it with an ellipsis until it fits beside the title.
    while g.size().x > room && detail.chars().count() > 4 {
        detail = detail.chars().take(detail.chars().count() - 2).collect();
        g = paint.layout_no_wrap(format!("{detail}..."), font.clone(), theme::TEXT_DIM);
    }
    if g.size().x <= room {
        paint.galley(
            pos2(right - g.size().x, cy - g.size().y / 2.0),
            g,
            theme::TEXT_DIM,
        );
    }
    r.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut w = World::generate(3, 12);
        let names = [
            "Carlos Carter",
            "Anna Carson",
            "Oscar Carlsen",
            "Marcus Scarborough",
            "Bob Smith",
        ];
        for (i, n) in names.iter().enumerate() {
            w.players[i].name = (*n).to_string();
        }
        // Make sure nobody else is called Car-anything.
        for p in w.players.iter_mut().skip(names.len()) {
            if p.name.to_lowercase().contains("car") {
                p.name = "Zed Nobody".to_string();
            }
        }
        for t in &mut w.teams {
            for job in Job::ALL {
                if let Some(s) = t.office.slot_mut(job).as_mut()
                    && s.name.to_lowercase().contains("car")
                {
                    s.name = "Staff Member".to_string();
                }
            }
        }
        w
    }

    #[test]
    fn matches_are_case_insensitive_substrings_with_word_starts_first() {
        assert_eq!(rank("Carlos Carter", "car"), Some(0));
        assert_eq!(rank("Anna Carson", "CAR"), Some(1));
        assert_eq!(rank("Marcus Scarborough", "car"), Some(2));
        assert_eq!(rank("Bob Smith", "car"), None);
        assert_eq!(rank("Bob Smith", ""), None);
        // Several words must all be there.
        assert_eq!(rank("Anna Carson", "ann car"), Some(1));
        assert_eq!(rank("Anna Carson", "ann zzz"), None);
    }

    #[test]
    fn better_matches_come_first_then_alphabetical() {
        let w = world();
        let hits = search(&w, 0, "car");
        let names: Vec<&str> = hits
            .iter()
            .filter(|h| h.group == Group::Players)
            .map(|h| h.title.as_str())
            .collect();
        // Names that start with it, then words that start with it (in
        // alphabetical order), then the rest.
        assert_eq!(
            names,
            [
                "Carlos Carter",
                "Anna Carson",
                "Oscar Carlsen",
                "Marcus Scarborough"
            ]
        );
    }

    #[test]
    fn an_empty_query_finds_nothing() {
        let w = world();
        assert!(search(&w, 0, "").is_empty());
        assert!(search(&w, 0, "   ").is_empty());
    }

    #[test]
    fn the_result_is_capped_and_grouped_players_clubs_staff() {
        let w = World::generate(3, 12);
        // A letter nearly everybody has.
        let hits = search(&w, 0, "a");
        assert!(hits.len() <= LIMIT);
        assert_eq!(hits.len(), LIMIT);
        let groups: Vec<Group> = hits.iter().map(|h| h.group).collect();
        let mut sorted = groups.clone();
        sorted.sort_by_key(|g| *g as u8);
        assert_eq!(groups, sorted, "players, then clubs, then staff");
        assert!(groups.contains(&Group::Players));
    }

    #[test]
    fn players_are_found_wherever_they_are() {
        let mut w = World::generate(3, 12);
        // Put somebody on a reserve squad, wherever the generator left them.
        let reserve = w.teams[2].roster.pop().unwrap();
        w.teams[2].reserves.push(reserve);
        w.players[reserve as usize].name = "Zzyzx Reserve".into();
        let free = w.teams[3].roster.pop().unwrap();
        w.players[free as usize].name = "Zzyzx Free".into();
        let hits = search(&w, 0, "zzyzx");
        let find = |n: &str| hits.iter().find(|h| h.title.contains(n)).unwrap();
        assert!(find("Reserve").detail.contains("reserves"));
        assert!(find("Free").detail.contains("Free agent"));
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn a_retired_player_is_found_and_marked() {
        let mut w = World::generate(3, 12);
        let p = w.teams[4].roster[0];
        w.players[p as usize].name = "Zzyzx Retired".into();
        w.teams[4].roster.retain(|&q| q != p);
        w.players[p as usize].renown.retired = Some(0);
        let hits = search(&w, 0, "zzyzx");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].detail.contains("Retired"));
    }

    #[test]
    fn another_clubs_overall_is_an_estimate_not_the_truth() {
        let mut w = World::generate(3, 12);
        let theirs = w.teams[5].roster[0];
        w.players[theirs as usize].name = "Qqxy Theirs".into();
        let mine = w.teams[0].roster[0];
        w.players[mine as usize].name = "Qqxy Mine".into();
        let hits = search(&w, 0, "qqxy");
        let mine_hit = hits.iter().find(|h| h.title.contains("Mine")).unwrap();
        let exact = format!("{:.0}", w.players[mine as usize].overall());
        assert_eq!(mine_hit.rating.as_deref(), Some(exact.as_str()));
        let theirs_hit = hits.iter().find(|h| h.title.contains("Theirs")).unwrap();
        let (lo, hi) = w.scouting.estimate_overall(&w.players[theirs as usize]);
        let shown = theirs_hit.rating.clone().unwrap();
        if hi - lo >= 0.8 {
            assert_eq!(shown, format!("{lo:.0}-{hi:.0}"));
            assert!(shown.contains('-'), "a range, not a number: {shown}");
        }
    }

    #[test]
    fn clubs_and_staff_are_found_and_lead_the_right_way() {
        let w = World::generate(3, 12);
        let club = &w.teams[4];
        let first = club.name.split_whitespace().next().unwrap();
        let hits = search(&w, 0, first);
        assert!(hits.iter().any(|h| h.open == Open::Club(4)));
        let gm = &w.teams[6].office.gm.name;
        let hits = search(&w, 0, gm);
        let h = hits.iter().find(|h| &h.title == gm).unwrap();
        assert_eq!(h.group, Group::Staff);
        assert_eq!(h.open, Open::Club(6));
        // Staff at the user's own club open the Staff tab.
        let mine = &w.teams[0].office.gm.name;
        let hits = search(&w, 0, mine);
        assert!(
            hits.iter()
                .any(|h| &h.title == mine && h.open == Open::OwnStaff)
        );
        // So do the unemployed.
        if let Some(s) = w.staff_pool.first() {
            let hits = search(&w, 0, &s.name);
            assert!(
                hits.iter()
                    .any(|h| h.title == s.name && h.open == Open::OwnStaff)
            );
        }
    }
}
