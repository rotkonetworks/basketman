use basketman_core::command::Command;
use basketman_core::engine::GameResult;
use basketman_core::league::{LiveRound, TeamId, World};
use basketman_core::player::{Attr, PlayerId, Position};
use basketman_core::training::{Intensity, TrainingFocus};
use eframe::egui::{self, Color32, FontId, Rect, RichText, Ui, vec2};

use crate::matchview::{Look, Playback, Touchline, ViewMode};
use crate::persist;
use crate::theme::{self, Backdrop};
#[cfg(not(target_arch = "wasm32"))]
use crate::view3d::Preset;

/// Clubs in a division, and the number of divisions.
pub(crate) const TEAMS: usize = 12;
pub(crate) const DIVISIONS: usize = 3;
pub(crate) const GAP: f32 = 16.0;
/// Storage key for the court view preference.
const VIEW_KEY: &str = "basketman.view";
const HUD_KEY: &str = "basketman.hud";

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum Tab {
    Inbox,
    /// The month view under Home.
    Calendar,
    Squad,
    Tactics,
    Training,
    Reserves,
    Staff,
    Scouting,
    Trade,
    /// Two players side by side; opened from the squad page and from
    /// profiles, not from the menu.
    Compare,
    /// The club's money.
    Finances,
    Stats,
    League,
    Match,
    /// One player's profile; reached through his name, not the menu.
    Player,
    /// Any club's page and any game's box score, reached the same way.
    Club,
    Game,
    /// The draft room, open while the draft is held.
    Draft,
    /// The film room, a page of the match group.
    Film,
    /// The club's own hall: the club page with the arena card.
    Arena,
    /// The arrival at a club: five short pages. Not in the menu.
    Welcome,
    /// The club's story, its people and the board's confidence in you.
    About,
    /// The manager's reputation, jobs and offers.
    Career,
    /// A job offer and the deal for it. Reached from the inbox and Career.
    Offer,
}

impl Tab {
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) const ALL: [Tab; 13] = [
        Tab::Inbox,
        Tab::Calendar,
        Tab::Squad,
        Tab::Tactics,
        Tab::Training,
        Tab::Reserves,
        Tab::Staff,
        Tab::Scouting,
        Tab::Trade,
        Tab::Finances,
        Tab::Stats,
        Tab::League,
        Tab::Match,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Tab::Inbox => "Inbox",
            Tab::Calendar => "Calendar",
            Tab::Squad => "Squad",
            Tab::Tactics => "Tactics",
            Tab::Training => "Training",
            Tab::Staff => "Staff",
            Tab::Reserves => "Reserves",
            Tab::Scouting => "Scouting",
            Tab::Trade => "Trade",
            Tab::Draft => "Draft",
            Tab::Compare => "Compare",
            Tab::Finances => "Finances",
            Tab::Stats => "Stats",
            Tab::League => "League",
            Tab::Match => "Match",
            Tab::Player => "Player",
            Tab::Club => "Club",
            Tab::Game => "Game",
            Tab::Film => "Film",
            Tab::Arena => "Arena",
            Tab::Welcome => "Welcome",
            Tab::About => "About",
            Tab::Career => "Career",
            Tab::Offer => "Offer",
        }
    }
}

pub struct App {
    pub(crate) world: World,
    pub(crate) seed: u64,
    /// False while the title screen is up.
    pub(crate) started: bool,
    /// A career exists to go back to.
    pub(crate) career: bool,
    /// Clubs on offer on the title screen for a seed, and the one picked.
    pub(crate) candidates: (u64, Vec<crate::title::Candidate>),
    pub(crate) pick: usize,
    /// The club the user manages.
    pub(crate) team: TeamId,
    /// The arrival pages, the career pages: see `club_ui.rs`.
    pub(crate) cu: crate::club_ui::ClubUi,
    pub(crate) tab: Tab,
    /// Player whose profile is open, and the screen to go back to.
    pub(crate) profile: Option<PlayerId>,
    pub(crate) club: Option<TeamId>,
    pub(crate) game: Option<usize>,
    /// Screens left behind, for Back.
    pub(crate) history: Vec<crate::nav::Place>,
    /// The trade being put together, and the answer to the last one made.
    pub(crate) deal: basketman_core::inbox::Deal,
    pub(crate) deal_note: Option<String>,
    /// The two players in the head-to-head view.
    pub(crate) compare: Vec<PlayerId>,
    /// Position filter on the scouting pool.
    pub(crate) scout_pos: Option<Position>,
    /// Playbook entry open in the tactics editor.
    pub(crate) slot: usize,
    /// What the Tactics tab has open: board or film, the selected player.
    pub(crate) room: crate::tactic_room::Room,
    /// Fixture index of the user's most recent game.
    pub(crate) last_game: Option<usize>,
    pub(crate) playback: Playback,
    /// How the court is drawn: in 3D, from an angle as on television, or
    /// straight down; and where the 3D camera is.
    pub(crate) look: Look,
    /// Message open in the inbox, and the folder it is filtered to.
    pub(crate) mail: Option<u32>,
    pub(crate) mail_kind: Option<basketman_core::inbox::Kind>,
    /// The game being coached right now; the rest of its round waits on it.
    pub(crate) live: Option<LiveRound>,
    pub(crate) touchline: Touchline,
    /// The search box that Ctrl+K opens.
    pub(crate) search: crate::search::Search,
    /// False in `--screenshot` mode: the harness never touches the save.
    persist: bool,
    /// Why the career in storage could not be opened. While this is set
    /// nothing is saved, so the stored one is not overwritten until the user
    /// starts a new career.
    pub(crate) load_failed: Option<String>,
    /// The title screen asked "really start a new career?" and waits.
    pub(crate) confirm_new: bool,
    /// Where the calendar stood when the career was last saved.
    saved_at: Option<(u16, usize, u32)>,
    pub(crate) backdrop: Backdrop,
    /// How the screens are laid out and what the scenes remember.
    pub(crate) sh: crate::shell::Shell,
    /// The F3 readout of drawing speed is up.
    show_speed: bool,
    /// The address fragment last acted on, in the browser.
    #[cfg(target_arch = "wasm32")]
    web_hash: String,
    /// `--screenshot <file> [tab]`: render a few frames, save one, quit.
    /// Lets the look be reviewed without a person at the window.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) shot: Option<(std::path::PathBuf, u32)>,
}

pub(crate) fn money(v: i64) -> String {
    let digits = v.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if v < 0 { format!("-{out}") } else { out }
}

/// Attribute value coloured by how good it is, FM style.
pub(crate) fn rating(v: f32, focused: bool) -> RichText {
    let (color, family) = match v {
        v if v >= 15.0 => (theme::POS, theme::mono_semibold()),
        v if v >= 11.0 => (theme::TEXT, egui::FontFamily::Monospace),
        v if v >= 7.0 => (theme::TEXT_DIM, egui::FontFamily::Monospace),
        _ => (theme::LINE_2, egui::FontFamily::Monospace),
    };
    let text = RichText::new(format!("{v:.0}"))
        .font(FontId::new(13.0, family))
        .color(color);
    if focused { text.underline() } else { text }
}

pub(crate) fn header_row(ui: &mut Ui, headers: &[&str]) {
    for h in headers {
        ui.label(theme::eyebrow_text(h));
    }
}

pub(crate) fn name(text: &str) -> RichText {
    RichText::new(text).family(theme::semibold())
}

impl App {
    /// Whether this run is the screenshot harness (never, in the browser).
    pub(crate) fn harness(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.shot.is_some()
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        #[allow(unused_mut)]
        let mut app = App {
            world: World::generate_tiers(1, TEAMS, DIVISIONS),
            seed: 1,
            started: false,
            career: false,
            candidates: (0, Vec::new()),
            pick: 0,
            team: 0,
            cu: Default::default(),
            tab: Tab::Inbox,
            compare: Vec::new(),
            profile: None,
            club: None,
            game: None,
            history: Vec::new(),
            deal: Default::default(),
            deal_note: None,
            scout_pos: None,
            slot: 0,
            room: Default::default(),
            last_game: None,
            playback: Playback::start(),
            look: Look::default(),
            mail: None,
            mail_kind: None,
            live: None,
            touchline: Touchline::default(),
            search: Default::default(),
            persist: true,
            load_failed: None,
            confirm_new: false,
            saved_at: None,
            backdrop: Backdrop::load(&cc.egui_ctx),
            sh: Default::default(),
            show_speed: std::env::var("SHOT_SPEED").is_ok(),
            #[cfg(target_arch = "wasm32")]
            web_hash: String::new(),
            #[cfg(not(target_arch = "wasm32"))]
            shot: None,
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            // `SHOT_SAVE=1` lets the harness save and load after all, to
            // test it: point `XDG_DATA_HOME` somewhere harmless first.
            app.persist = !std::env::args().any(|a| a == "--screenshot")
                || std::env::var("SHOT_SAVE").is_ok();
        }
        if app.persist
            && let Some(storage) = cc.storage
        {
            match persist::read(storage) {
                persist::Loaded::Nothing => {}
                persist::Loaded::Career(world) => {
                    app.team = world.watch.unwrap_or(0);
                    app.world = *world;
                    app.career = true;
                    app.saved_at = Some(app.save_mark());
                }
                persist::Loaded::Unreadable(why) => app.load_failed = Some(why),
            }
            if let Some(text) = storage.get_string(HUD_KEY) {
                app.sh.hud.prefs = crate::matchview::HudPrefs::decode(&text);
                app.sh.hud.drawer = app.sh.hud.prefs.tab.filter(|_| app.sh.hud.prefs.open);
            }
            // The court view the player last chose. Older saves hold only
            // "angled" or "flat".
            if let Some(mode) = storage
                .get_string(VIEW_KEY)
                .and_then(|v| ViewMode::from_key(&v))
            {
                // The angled view is gone; whoever had it gets 3D.
                app.look.mode = if mode == ViewMode::Angled {
                    ViewMode::Three
                } else {
                    mode
                };
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let args: Vec<String> = std::env::args().collect();
            if let Some(i) = args.iter().position(|a| a == "--screenshot") {
                app.shot = args.get(i + 1).map(|path| (path.into(), 0));
                // `SHOT_VIEW=3d|angled|flat` picks the court view,
                // `SHOT_FLAT=1` is the older way to ask for flat, and
                // `SHOT_CAM=broadcast|orbit|top|basket` the 3D camera.
                if std::env::var("SHOT_FLAT").is_ok() {
                    app.look.mode = ViewMode::Flat;
                }
                if let Ok(v) = std::env::var("SHOT_VIEW") {
                    app.look.mode = ViewMode::from_key(&v.to_lowercase()).unwrap_or(app.look.mode);
                }
                if let Ok(c) = std::env::var("SHOT_CAM") {
                    app.look.cam.preset =
                        Preset::parse(&c.to_lowercase()).unwrap_or(app.look.cam.preset);
                }
                // `SHOT_SEARCH=car` opens the search box with that typed in.
                if let Ok(q) = std::env::var("SHOT_SEARCH") {
                    app.search.show();
                    app.search.query = q;
                }
                let tab = args.get(i + 2).map(String::as_str);
                // `--screenshot file title` stays on the title screen.
                if tab == Some("title") {
                    return app;
                }
                // `SHOT_CLUB=n` manages club number n instead of the first.
                let club: usize = std::env::var("SHOT_CLUB")
                    .ok()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                app.begin(club);
                app.tab = Tab::ALL
                    .into_iter()
                    .chain([
                        Tab::Player,
                        Tab::Club,
                        Tab::Compare,
                        Tab::Film,
                        Tab::Arena,
                        Tab::Welcome,
                        Tab::About,
                        Tab::Career,
                        Tab::Offer,
                    ])
                    .find(|t| Some(t.label().to_lowercase().as_str()) == tab)
                    .unwrap_or(Tab::Inbox);
                // `SHOT_OFFER=n` has club n lose its manager and call.
                if let Some(n) = std::env::var("SHOT_OFFER").ok().and_then(|n| n.parse::<usize>().ok()) {
                    app.cu.offer = app.world.stage_offer(n);
                    app.cu.keep_set = false;
                }
                // `SHOT_STEP=n` opens page n (from 0) of the arrival.
                if let Some(n) = std::env::var("SHOT_STEP").ok().and_then(|n| n.parse().ok()) {
                    app.cu.step = n;
                }
                // `--screenshot file club` shows the club page; `SHOT_PAGE=n`
                // shows club n's instead of ours.
                if app.tab == Tab::Club {
                    app.club = Some(
                        std::env::var("SHOT_PAGE")
                            .ok()
                            .and_then(|n| n.parse().ok())
                            .unwrap_or(app.team),
                    );
                }
                // Follow a few opponents and play three rounds, so every
                // screen has something to show.
                let mut others: Vec<PlayerId> = (0..app.world.players.len() as PlayerId)
                    .filter(|&p| !app.is_mine(p))
                    .collect();
                others.sort_by(|&a, &b| {
                    let ov = |p: PlayerId| app.world.players[p as usize].overall();
                    ov(b).total_cmp(&ov(a))
                });
                for &p in others.iter().take(5) {
                    app.world.scouting.toggle(p);
                }
                app.compare = vec![app.world.teams[app.team].roster[0], others[0]];
                // `--screenshot file player scouted` opens an opponent.
                // `famous` opens the league's biggest name, `retired` the
                // first retired player (after `SHOT_SEASONS`).
                app.profile = Some(if args.get(i + 3).map(String::as_str) == Some("scouted") {
                    others[0]
                } else if args.get(i + 3).map(String::as_str) == Some("famous") {
                    app.world.biggest_names(1)[0].0
                } else {
                    app.world.teams[app.team].roster[0]
                });
                // `--screenshot file player signed` opens the first of our
                // players with a signature move, `player seen` an opponent
                // with one the scouts have revealed, and `inbox sig` the
                // assistant's mail about the next opponent after one of his
                // players' moves became known. A world with no such player
                // is given one, for the picture only.
                let want = args.get(i + 3).map(String::as_str);
                let sig_mode = match (tab, want) {
                    (Some("player"), Some("signed")) => 1,
                    (Some("player"), Some("seen")) => 2,
                    (Some("inbox"), Some("sig")) => 3,
                    _ => 0,
                };
                if sig_mode > 0 {
                    use basketman_core::signature::Signature;
                    let reveal = |app: &mut App, p: PlayerId| {
                        if app.world.players[p as usize].signatures.is_empty() {
                            app.world.players[p as usize].signatures =
                                vec![Signature::StepBackThree];
                        }
                        let sigs = app.world.players[p as usize].signatures.clone();
                        for s in sigs {
                            app.world.scouting.observe_signature(p, s);
                        }
                    };
                    if sig_mode == 1 {
                        let own = app.world.teams[app.team].roster.clone();
                        let p = own
                            .iter()
                            .copied()
                            .find(|&p| !app.world.players[p as usize].signatures.is_empty())
                            .unwrap_or(own[0]);
                        if app.world.players[p as usize].signatures.is_empty() {
                            app.world.players[p as usize].signatures = vec![Signature::PosterDunk];
                        }
                        app.profile = Some(p);
                    } else if sig_mode == 2 {
                        let p = others
                            .iter()
                            .copied()
                            .find(|&p| {
                                !app.world.players[p as usize].signatures.is_empty()
                                    && app.world.team_of(p).is_some()
                            })
                            .unwrap_or(others[0]);
                        reveal(&mut app, p);
                        app.profile = Some(p);
                    } else {
                        let me = app.team;
                        let next = app
                            .world
                            .fixtures
                            .iter()
                            .find(|f| f.result.is_none() && (f.home == me || f.away == me))
                            .map(|f| if f.home == me { f.away } else { f.home });
                        if let Some(opp) = next {
                            let star = app.world.teams[opp].roster[0];
                            reveal(&mut app, star);
                            app.world.preview();
                            let day = app.world.day;
                            if let Some(m) = app.world.desk.messages.last_mut() {
                                m.day = day;
                                app.mail = Some(m.id);
                            }
                        }
                    }
                }
                // `SHOT_SEASONS=2` plays that many whole seasons first, the
                // assistant drafting, so the history screens have history.
                let seasons = std::env::var("SHOT_SEASONS")
                    .ok()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                for _ in 0..seasons {
                    use basketman_core::inbox::Stop;
                    loop {
                        match app.world.advance() {
                            Stop::MatchDay => {
                                app.world.play_round();
                            }
                            Stop::Mail => {}
                            Stop::Draft => break,
                        }
                    }
                    app.world.draft_auto(true);
                }
                if seasons > 0 {
                    app.profile = Some(app.world.teams[app.team].roster[0]);
                    if args.get(i + 3).map(String::as_str) == Some("retired")
                        && let Some(p) =
                            app.world
                                .hall_of_fame()
                                .first()
                                .map(|h| h.player)
                                .or_else(|| {
                                    app.world
                                        .players
                                        .iter()
                                        .find(|p| p.is_retired())
                                        .map(|p| p.id)
                                })
                    {
                        app.profile = Some(p);
                    }
                    app.compare = app.world.teams[app.team].roster[..2].to_vec();
                }
                // `SHOT_MEDIA=1` makes the squad poor talkers doing a lot of
                // media work, so gaffes and appearance requests show up.
                if std::env::var("SHOT_MEDIA").is_ok() {
                    for p in app.world.teams[app.team].roster.clone() {
                        let pl = &mut app.world.players[p as usize];
                        pl.personality.presence = 2;
                        pl.personality.temperament = 3;
                        pl.renown.media = basketman_core::media::MediaLevel::ALot;
                    }
                }
                // `SHOT_ROUNDS=12` plays that many rounds instead of three.
                // `SHOT_DAYS=10` instead passes that many days of the camp.
                let days: Option<u32> = std::env::var("SHOT_DAYS")
                    .ok()
                    .and_then(|n| n.parse().ok());
                let rounds = if days.is_some() {
                    0
                } else {
                    std::env::var("SHOT_ROUNDS")
                        .ok()
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(3)
                };
                // `SHOT_SCOUT=1` sends the scout to a game of another club
                // on each of those days.
                for _ in 0..days.unwrap_or(0) {
                    if std::env::var("SHOT_SCOUT").is_ok() {
                        let (me, day) = (app.team, app.world.day);
                        // Our next opponent first, then anybody.
                        let next = app.world.user_fixture().map(|i| {
                            let f = &app.world.fixtures[i];
                            if f.home == me { f.away } else { f.home }
                        });
                        let order = next.into_iter().chain(0..app.world.teams.len());
                        for t in order {
                            if t != me {
                                let cmd = Command::AssignScout { team: t, day };
                                if app.world.apply(cmd).is_ok() {
                                    break;
                                }
                            }
                        }
                    }
                    let _ = app.world.apply(Command::Continue);
                }
                for _ in 0..rounds {
                    let played = app.world.play_round();
                    app.last_game = played.into_iter().find(|&i| {
                        app.world.fixtures[i].home == app.team
                            || app.world.fixtures[i].away == app.team
                    });
                }
                // `SHOT_SACK=1` has the board dismiss the manager,
                // `SHOT_RESIGN=1` has him resign, and `SHOT_ACCEPT=1` takes
                // the first open offer (stage one with `SHOT_OFFER=n`, and
                // `SHOT_ROUNDS=0` so it does not lapse).
                if std::env::var("SHOT_SACK").is_ok() {
                    app.world.stage_dismissal();
                    app.follow_watch();
                }
                if std::env::var("SHOT_RESIGN").is_ok() {
                    let _ = app.world.apply(Command::Resign);
                    app.follow_watch();
                }
                if std::env::var("SHOT_ACCEPT").is_ok()
                    && let Some(i) = app.world.open_job_offers().first().copied()
                {
                    let cmd = Command::AcceptJob {
                        offer: i,
                        bring: Vec::new(),
                        keep: basketman_core::staff::Job::HIRED.to_vec(),
                    };
                    app.cu.note = app.world.apply(cmd).ok().and_then(|o| o.note);
                    app.follow_watch();
                    app.start_intro();
                }
                // `SHOT_CONDITION=0.1` sets every hall's condition and
                // `SHOT_FILL=0.3` how full every played game's crowd was.
                if let Some(c) = std::env::var("SHOT_CONDITION")
                    .ok()
                    .and_then(|v| v.parse().ok())
                {
                    for a in &mut app.world.econ.arenas {
                        a.condition = c;
                    }
                }
                if let Some(f) = std::env::var("SHOT_FILL")
                    .ok()
                    .and_then(|v| v.parse::<f32>().ok())
                {
                    for (i, fx) in app.world.fixtures.iter().enumerate() {
                        if let Some(c) = app.world.econ.crowds.get_mut(i) {
                            c.0 = (app.world.econ.arenas[fx.home].capacity as f32 * f) as u32;
                        }
                    }
                }
                // `SHOT_DEBT=300000` puts the user's club that deep in
                // overdraft; `SHOT_OVER_CAP=120000` pushes its payroll that
                // far over the cap.
                if let Some(d) = std::env::var("SHOT_DEBT")
                    .ok()
                    .and_then(|v| v.parse::<i64>().ok())
                {
                    let t = app.team;
                    app.world.econ.ledgers[t].start -= app.world.teams[t].budget + d;
                    app.world.teams[t].budget = -d;
                }
                if let Some(over) = std::env::var("SHOT_OVER_CAP")
                    .ok()
                    .and_then(|v| v.parse::<i64>().ok())
                {
                    let t = app.team;
                    let roster = app.world.teams[t].roster.clone();
                    let extra = (basketman_core::season::SALARY_CAP + over - app.world.payroll(t))
                        / roster.len() as i64;
                    for p in roster {
                        let s = &mut app.world.players[p as usize].contract.salary;
                        *s = (*s as i64 + extra).max(1000) as u32;
                    }
                }
                // Let the week's mail arrive.
                while days.is_none() && app.world.advance() == basketman_core::inbox::Stop::Mail {}
                // `--screenshot file draft` plays the season out to the draft;
                // `--screenshot file trade` shows a deal half built.
                if tab == Some("draft") {
                    use basketman_core::inbox::Stop;
                    loop {
                        match app.world.advance() {
                            Stop::MatchDay => {
                                app.world.play_round();
                            }
                            Stop::Mail => {}
                            Stop::Draft => break,
                        }
                    }
                    app.tab = Tab::Draft;
                }
                if app.tab == Tab::Trade {
                    let w = &app.world;
                    app.deal = basketman_core::inbox::Deal {
                        club: 3,
                        give: vec![w.teams[app.team].roster[4]],
                        get: vec![w.teams[3].roster[1]],
                        give_picks: w
                            .picks
                            .iter()
                            .position(|p| p.owner == app.team)
                            .into_iter()
                            .collect(),
                        ..Default::default()
                    };
                }
                // `--screenshot file inbox 3` opens the fourth message.
                if app.tab == Tab::Inbox
                    && let Some(n) = args.get(i + 3).and_then(|s| s.parse::<usize>().ok())
                {
                    app.mail = app.world.inbox().get(n).map(|m| m.id);
                }
                // `--screenshot file inbox report` opens the latest scout report.
                if app.tab == Tab::Inbox && args.get(i + 3).map(String::as_str) == Some("report") {
                    app.mail = app
                        .world
                        .inbox()
                        .into_iter()
                        .find(|m| m.subject.starts_with("Scout report"))
                        .map(|m| m.id);
                }
                // `--screenshot file inbox budget` opens the board's budgets letter.
                if app.tab == Tab::Inbox && args.get(i + 3).map(String::as_str) == Some("budget") {
                    app.mail = app
                        .world
                        .inbox()
                        .into_iter()
                        .find(|m| m.subject.starts_with("Budgets for season"))
                        .map(|m| m.id);
                }
                // `--screenshot file inbox gaffe|appearance` opens the latest
                // press story about a gaffe, or sponsor's appearance request
                // (with `SHOT_MEDIA=1`; the request is kept open for it).
                if app.tab == Tab::Inbox
                    && args.get(i + 3).map(String::as_str) == Some("appearance")
                {
                    let day = app.world.day;
                    for a in &mut app.world.fame.asks {
                        a.expires = day + 3;
                        a.event_day = day + 4;
                        a.state = basketman_core::inbox::OfferState::Open;
                    }
                    app.mail = app
                        .world
                        .inbox()
                        .into_iter()
                        .find(|m| m.appearance.is_some())
                        .map(|m| m.id);
                }
                if app.tab == Tab::Inbox && args.get(i + 3).map(String::as_str) == Some("gaffe") {
                    app.mail = app
                        .world
                        .inbox()
                        .into_iter()
                        .find(|m| m.subject.contains("wrong reasons"))
                        .map(|m| m.id);
                }
                // `--screenshot file inbox sponsor` opens a sponsor's offer.
                if app.tab == Tab::Inbox && args.get(i + 3).map(String::as_str) == Some("sponsor") {
                    app.mail = app
                        .world
                        .inbox()
                        .into_iter()
                        .filter(|m| m.sponsor.is_some())
                        .max_by_key(|m| {
                            let o = &app.world.econ.offers[m.sponsor.unwrap_or(0)];
                            (o.open_on(app.world.day), m.id)
                        })
                        .map(|m| m.id);
                    // Keep it answerable, whatever the calendar did to it.
                    if let Some(i) = app
                        .mail
                        .and_then(|id| app.world.desk.messages.get(id as usize))
                        .and_then(|m| m.sponsor)
                    {
                        app.world.econ.offers[i].expires = app.world.day + 3;
                    }
                }
                // `--screenshot file tactics film [best|turnovers|breakdowns [n]]`
                // opens the film room with a clip picked and held just before
                // its event. `SHOT_DEFENSE` (man, switch, zone23, zone32, pack),
                // `SHOT_PRESS` (three, full) and `SHOT_WITHOUT=1` set up the
                // board's defensive view; `SHOT_SELECT=n` picks a starter.
                if app.tab == Tab::Tactics {
                    use basketman_core::tactics::{DefenseScheme, Press};
                    let arg = args.get(i + 3).map(String::as_str);
                    if arg == Some("nofilm") {
                        // As after loading a save: no tapes at all.
                        app.room.view = crate::tactic_room::RoomView::Film;
                        app.world.replay = None;
                        app.world.tapes.clear();
                    }
                    if arg == Some("film") {
                        use crate::film::List;
                        app.room.view = crate::tactic_room::RoomView::Film;
                        app.room.film.freeze = true;
                        app.room.film.playing = false;
                        let list = match args.get(i + 4).map(String::as_str) {
                            Some("turnovers") => List::Turnovers,
                            Some("breakdowns") => List::Breakdowns,
                            _ => List::Best,
                        };
                        let n = args.get(i + 5).and_then(|n| n.parse().ok()).unwrap_or(0);
                        app.room.film.want = Some((list, n));
                    }
                    let mut t = app.world.teams[app.team].playbook[0].tactics.clone();
                    match std::env::var("SHOT_DEFENSE").as_deref() {
                        Ok("man") => t.defense = DefenseScheme::ManToMan,
                        Ok("switch") => t.defense = DefenseScheme::SwitchEverything,
                        Ok("zone23") => t.defense = DefenseScheme::Zone23,
                        Ok("zone32") => t.defense = DefenseScheme::Zone32,
                        Ok("pack") => t.defense = DefenseScheme::PackLine,
                        _ => {}
                    }
                    match std::env::var("SHOT_PRESS").as_deref() {
                        Ok("three") => t.press = Press::ThreeQuarter,
                        Ok("full") => t.press = Press::FullCourt,
                        _ => {}
                    }
                    let _ = app.world.apply(Command::SetTactics {
                        slot: 0,
                        tactics: t.clone(),
                    });
                    app.room.with_ball = std::env::var("SHOT_WITHOUT").is_err();
                    if let Some(n) = std::env::var("SHOT_SELECT")
                        .ok()
                        .and_then(|n| n.parse::<usize>().ok())
                    {
                        app.room.selected = t.rotation.get(n).map(|s| s.player);
                    }
                }
                // `--screenshot file match 95` freezes the replay there;
                // `--screenshot file match live` stops a live game after
                // five minutes with a bit of everything done on the bench.
                let at = args.get(i + 3).and_then(|s| s.parse().ok());
                app.playback = Playback {
                    time: at.unwrap_or(95.0),
                    playing: false,
                    speed: 2.0,
                };
                // `SHOT_FIND=made3|made_jumper|miss_jumper|miss3|layup|miss_layup|
                // dunk|hook|block|offreb|defreb|ft` freezes the replay at the
                // `SHOT_NTH`-th (from 0) such moment, `SHOT_T0` seconds (default
                // -0.5) from it: a shell loop over `SHOT_T0` steps through it.
                if let Ok(what) = std::env::var("SHOT_FIND")
                    && let Some((_, rec)) = &app.world.replay
                {
                    let found = crate::view3d::finder::moments(rec, &what);
                    let nth: usize = std::env::var("SHOT_NTH")
                        .ok()
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(0);
                    let t0: f32 = std::env::var("SHOT_T0")
                        .ok()
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(-0.5);
                    eprintln!("{} moments of {what}", found.len());
                    if let Some(t) = found.get(nth) {
                        app.playback.time = (t + t0).max(0.0);
                        // `SHOT_VIEWPOINT=side|behind|close` pins the camera at the
                        // moment, so that the frames of a sequence can be compared.
                        if let Ok(vp) = std::env::var("SHOT_VIEWPOINT") {
                            app.look.cam.fixed = crate::view3d::finder::viewpoint(rec, *t, &vp);
                        }
                    }
                }
                if args.get(i + 3).map(String::as_str) == Some("live")
                    && let Some(mut live) = app.world.kickoff(app.team)
                {
                    use basketman_core::league::{LiveInput, Stoppage};
                    // `SHOT_LIVE_TO=1500` plays that many seconds first.
                    let to: f32 = std::env::var("SHOT_LIVE_TO")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(420.0);
                    // `SHOT_STOP=tip|timeout|theirs|break|half` has play
                    // stop there, and the coaching room open; without it
                    // the game is live, with a timeout and a swap asked
                    // for. `SHOT_ROOM=read|tactics|players|talk` picks its
                    // tab, `SHOT_PICK=n` opens the n-th player's row.
                    let stop = std::env::var("SHOT_STOP").ok();
                    if stop.as_deref() != Some("tip") {
                        while live.game.time() < to && !live.game.is_over() {
                            live.step();
                        }
                    }
                    let side = live.side;
                    let bench = live.game.players(side);
                    let off = bench.iter().find(|p| p.on_court).map(|p| p.id);
                    let on = bench.iter().find(|p| !p.on_court).map(|p| p.id);
                    match stop.as_deref() {
                        Some("timeout") | Some("theirs") => {
                            let ours = stop.as_deref() == Some("timeout");
                            if ours {
                                live.input(LiveInput::Timeout);
                            }
                            while !live.game.is_over() {
                                match live.step() {
                                    Some(basketman_core::engine::Pause::Timeout(s)) if (s == side) == ours => break,
                                    _ => {}
                                }
                            }
                        }
                        Some("break") | Some("half") => {
                            let half = stop.as_deref() == Some("half");
                            while !live.game.is_over() {
                                let hit = live.step().is_some();
                                if hit && (!half || matches!(live.stoppage(), Some(Stoppage::Break { half: true, .. }))) {
                                    break;
                                }
                            }
                        }
                        Some("tip") => {}
                        _ => {
                            if let (Some(off), Some(on)) = (off, on) {
                                live.input(LiveInput::Swap { off, on });
                            }
                            live.input(LiveInput::Timeout);
                        }
                    }
                    app.touchline.off = bench.iter().filter(|p| p.on_court).nth(2).map(|p| p.id);
                    app.playback.time = live.game.time();
                    if stop.is_some() {
                        app.playback.playing = false;
                        app.open_room(&live);
                        if let Some(r) = app.touchline.room.as_mut() {
                            r.pick_for_harness(
                                std::env::var("SHOT_ROOM").ok().as_deref(),
                                std::env::var("SHOT_PICK").ok().and_then(|n| n.parse().ok()),
                                &live,
                            );
                        }
                    }
                    app.live = Some(live);
                }
                // `SHOT_HUD=full|minimal|clean` and `SHOT_PANEL_ALPHA=0.4`.
                match std::env::var("SHOT_HUD").as_deref() {
                    Ok("minimal") => app.sh.hud.prefs.level = 1,
                    Ok("clean") => app.sh.hud.prefs.level = 2,
                    _ => {}
                }
                if let Some(a) = std::env::var("SHOT_PANEL_ALPHA").ok().and_then(|a| a.parse::<f32>().ok()) {
                    app.sh.hud.prefs.alpha = a.clamp(0.35, 1.0);
                }
                // `SHOT_DRAWER=scores|shots|flow|box|feed` opens that tab.
                if let Ok(d) = std::env::var("SHOT_DRAWER") {
                    app.sh.hud.pick(&d);
                }
            }
        }
        app
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn capture(&mut self, ui: &Ui) {
        let Some((path, frame)) = &mut self.shot else {
            return;
        };
        *frame += 1;
        ui.ctx().request_repaint();
        // `SHOT_PERF=1`: time frames 30 to 90 and print where they went.
        if std::env::var("SHOT_PERF").is_ok() {
            if *frame == 30 {
                crate::perf::reset();
            }
            if *frame == 90 {
                crate::perf::report();
            }
        }
        if *frame != 90 {
            return;
        }
        let (path, ctx) = (path.clone(), ui.ctx().clone());
        let save = egui::ScreenshotCallback::new(move |image| {
            let [w, h] = image.size;
            if let Err(e) = image::save_buffer(
                &path,
                image.as_raw(),
                w as u32,
                h as u32,
                image::ColorType::Rgba8,
            ) {
                eprintln!("could not save screenshot: {e}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        });
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Screenshot(save));
    }

    /// Start a new career in charge of `team` in the world of `self.seed`.
    pub(crate) fn begin(&mut self, team: TeamId) {
        self.world = World::new_career_tiers(self.seed, TEAMS, DIVISIONS);
        self.team = team;
        let _ = self.world.apply(Command::TakeCharge(team));
        self.mail = None;
        self.mail_kind = None;
        self.live = None;
        self.touchline = Touchline::default();
        self.last_game = None;
        self.slot = 0;
        self.compare.clear();
        self.profile = None;
        self.club = None;
        self.game = None;
        self.history.clear();
        self.deal = Default::default();
        self.deal_note = None;
        self.cu = Default::default();
        self.start_intro();
        self.started = true;
        self.career = true;
        self.confirm_new = false;
        // The user chose to replace whatever is stored, readable or not.
        self.load_failed = None;
        self.saved_at = None;
    }

    /// What changes when the calendar moves: a day, a round, a season.
    fn save_mark(&self) -> (u16, usize, u32) {
        (self.world.season, self.world.round, self.world.day)
    }

    /// Whether the career may be written now. Not during a live game (the
    /// world has already started the round, and the game cannot be saved),
    /// not in screenshot mode, and not over a save that did not load.
    fn may_save(&self) -> bool {
        self.persist && self.career && self.live.is_none() && self.load_failed.is_none()
    }

    pub(crate) fn squad(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let roster = self.world.teams[self.team].roster.clone();
        let mut open_compare = false;
        bd.card(ui, |ui| {
            ui.horizontal(|ui| {
                theme::eyebrow(ui, &format!("First team / {} players", roster.len()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("Compare players")
                        .on_hover_text(
                            "Two players side by side. Pick them with the Compare buttons in \
                             the table, or choose on the next page.",
                        )
                        .clicked()
                    {
                        open_compare = true;
                    }
                });
            });
            ui.label(
                RichText::new(
                    "Underlined attributes are the ones each player's training focus works on.",
                )
                .color(theme::TEXT_DIM),
            );
            ui.add_space(8.0);
            egui::ScrollArea::horizontal()
                .id_salt("squad_scroll")
                .show(ui, |ui| {
                    egui::Grid::new("squad")
                        .striped(true)
                        .spacing(vec2(14.0, 9.0))
                        .show(ui, |ui| {
                            header_row(
                                ui,
                                &[
                                    "Player",
                                    "Pos",
                                    "Age",
                                    "Ht",
                                    "Wt",
                                    "Wing",
                                    "Reach",
                                    "Ovr",
                                    "Character",
                                    "Fame",
                                    "Media",
                                    "Training focus",
                                    "Intensity",
                                    "Fit",
                                    "Sharp",
                                    "Morale",
                                ],
                            );
                            for a in Attr::ALL {
                                ui.label(theme::eyebrow_text(a.label()));
                            }
                            header_row(ui, &["Salary", "Years", ""]);
                            ui.end_row();

                            for pid in roster {
                                let p = &self.world.players[pid as usize];
                                let (mut new_focus, mut new_intensity) = (p.training, p.intensity);
                                crate::signature_ui::name_with_mark(
                                    ui,
                                    pid,
                                    name(&p.name),
                                    &p.signatures,
                                );
                                let also: Vec<&str> =
                                    p.also_plays().iter().map(|x| x.label()).collect();
                                let pos = std::iter::once(p.position.label())
                                    .chain(also)
                                    .collect::<Vec<_>>()
                                    .join("/");
                                ui.label(theme::mono(pos).color(theme::ACCENT));
                                ui.label(theme::mono(p.age.to_string()));
                                for v in [
                                    p.body.height_cm,
                                    p.body.weight_kg,
                                    p.body.wingspan_cm,
                                    p.body.standing_reach_cm,
                                ] {
                                    ui.label(theme::mono(format!("{v:.0}")));
                                }
                                ui.label(rating(p.overall(), false));
                                let kind = p.personality.kind();
                                ui.label(RichText::new(kind.label()).color(if kind.is_concern() {
                                    theme::WARN
                                } else {
                                    theme::TEXT_DIM
                                }))
                                .on_hover_text(kind.description());
                                crate::fame_ui::fame_chip(ui, p.renown.fame);
                                let media_now = p.renown.media;
                                let new_media =
                                    crate::fame_ui::media_combo(ui, ("media", pid), media_now);
                                egui::ComboBox::from_id_salt(("focus", pid))
                                    .width(150.0)
                                    .selected_text(new_focus.label())
                                    .show_ui(ui, |ui| {
                                        for f in TrainingFocus::ALL {
                                            ui.selectable_value(&mut new_focus, f, f.label());
                                        }
                                    });
                                egui::ComboBox::from_id_salt(("intensity", pid))
                                    .width(80.0)
                                    .selected_text(new_intensity.label())
                                    .show_ui(ui, |ui| {
                                        for i in Intensity::ALL {
                                            ui.selectable_value(&mut new_intensity, i, i.label());
                                        }
                                    });
                                let fit = theme::mono(format!("{:.0}%", p.condition * 100.0));
                                match &p.injury {
                                    Some(inj) => ui
                                        .label(
                                            theme::mono(format!("out {} wk", inj.weeks_left))
                                                .color(theme::WARN),
                                        )
                                        .on_hover_text(inj.what.label()),
                                    None => ui.label(match p.condition {
                                        c if c >= 0.95 => fit.color(theme::POS),
                                        c if c < 0.85 => fit.color(theme::WARN),
                                        _ => fit,
                                    }),
                                };
                                let sharp = theme::mono(format!("{:.0}%", p.sharp * 100.0));
                                ui.label(match p.sharp {
                                    s if s >= 0.9 => sharp.color(theme::POS),
                                    s if s < 0.75 => sharp.color(theme::WARN),
                                    _ => sharp,
                                })
                                .on_hover_text(
                                    "Match sharpness: kept by playing, lost on the bench and in \
                                     the treatment room. Reserve games bring it back.",
                                );
                                ui.label(RichText::new(p.mood()).color(match p.morale {
                                    m if m >= 0.62 => theme::POS,
                                    m if m < 0.42 => theme::WARN,
                                    _ => theme::TEXT_DIM,
                                }));
                                let focus = p.training.attrs();
                                for a in Attr::ALL {
                                    ui.label(rating(p.attrs[a], focus.contains(&a)));
                                }
                                ui.label(theme::mono(money(p.contract.salary as i64)));
                                ui.label(theme::mono(p.contract.years_left.to_string()));
                                if (new_focus, new_intensity) != (p.training, p.intensity) {
                                    let _ = self.world.apply(Command::SetTraining {
                                        player: pid,
                                        focus: new_focus,
                                        intensity: new_intensity,
                                    });
                                }
                                if new_media != media_now {
                                    let _ = self.world.apply(Command::SetMedia {
                                        player: pid,
                                        level: new_media,
                                    });
                                }
                                crate::pages::compare_button(ui, &mut self.compare, pid);
                                ui.end_row();
                            }
                        });
                    // Room for the horizontal scrollbar below the last row.
                    ui.add_space(14.0);
                });
        });
        if open_compare {
            // Two picked players stay; otherwise the two best of the squad.
            if self.compare.len() < 2 {
                let mut best = self.world.teams[self.team].roster.clone();
                best.sort_by(|&a, &b| {
                    let ov = |p: PlayerId| self.world.players[p as usize].overall();
                    ov(b).total_cmp(&ov(a))
                });
                self.compare = best.into_iter().take(2).collect();
            }
            self.go(Tab::Compare);
        }
    }

    pub(crate) fn standings(&self, ui: &mut Ui, bd: &Backdrop) {
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Standings");
            let division = crate::calendar::division_picker(ui, &self.world);
            ui.label(
                RichText::new(self.world.division_name(division))
                    .family(theme::semibold())
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(6.0);
            egui::Grid::new("table")
                .striped(true)
                .spacing(vec2(18.0, 8.0))
                .show(ui, |ui| {
                    header_row(ui, &["#", "Team", "W", "L", "PF", "PA", "Diff"]);
                    ui.end_row();
                    for (rank, t) in self.world.table_of(division).into_iter().enumerate() {
                        let s = &self.world.standings[t];
                        ui.label(theme::mono((rank + 1).to_string()).color(theme::TEXT_MUTE));
                        self.club_link(ui, t);
                        for v in [
                            s.wins as i64,
                            s.losses as i64,
                            s.points_for as i64,
                            s.points_against as i64,
                        ] {
                            ui.label(theme::mono(v.to_string()));
                        }
                        let diff = s.points_for as i64 - s.points_against as i64;
                        let color = if diff >= 0 { theme::POS } else { theme::WARN };
                        ui.label(theme::mono(format!("{diff:+}")).color(color));
                        ui.end_row();
                    }
                });
        });
    }

    pub(crate) fn results(&self, ui: &mut Ui, bd: &Backdrop) {
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Fixtures and results");
            let division = crate::calendar::viewed_division(ui, &self.world);
            ui.label(
                RichText::new(format!(
                    "{}. Every game of the season. A score opens its box score.",
                    self.world.division_name(division)
                ))
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(6.0);
            self.fixture_list(ui, None, 640.0);
        });
    }

    pub(crate) fn box_score(&self, ui: &mut Ui, r: &GameResult, side: usize) {
        egui::Grid::new(("box", side))
            .striped(true)
            .spacing(vec2(18.0, 8.0))
            .show(ui, |ui| {
                header_row(
                    ui,
                    &[
                        "Player", "Min", "Pts", "FG", "3PT", "FT", "Reb", "Ast", "Stl", "Blk",
                        "TO", "PF", "+/-", "Rtg",
                    ],
                );
                ui.end_row();
                // Every figure is graded against all who played, both
                // teams, so the standout lines stand out.
                let played = || {
                    r.boxes
                        .iter()
                        .flatten()
                        .map(|(_, l)| l)
                        .filter(|l| l.secs > 0)
                };
                let column = |f: &dyn Fn(&basketman_core::engine::BoxLine) -> f32| {
                    played().map(f).collect::<Vec<f32>>()
                };
                // Makes above what an average shooter gets from the same
                // attempts: volume and accuracy in one number.
                let shooting = |made: u16, tried: u16, par: f32| made as f32 - tried as f32 * par;
                let fg = column(&|l| shooting(l.fgm, l.fga, 0.45));
                let three = column(&|l| shooting(l.tpm, l.tpa, 0.34));
                let ft = column(&|l| shooting(l.ftm, l.fta, 0.72));
                let counts = [
                    column(&|l| l.pts as f32),
                    column(&|l| l.reb() as f32),
                    column(&|l| l.ast as f32),
                    column(&|l| l.stl as f32),
                    column(&|l| l.blk as f32),
                ];
                let plus_minus = column(&|l| l.pm as f32);
                for (pid, l) in &r.boxes[side] {
                    crate::profile::player_link(
                        ui,
                        *pid,
                        name(&self.world.players[*pid as usize].name),
                    );
                    use crate::pages::{dim, faded, graded, graded_count, match_rating};
                    if l.secs == 0 {
                        ui.label(dim("did not play"));
                        ui.end_row();
                        continue;
                    }
                    ui.label(theme::mono((l.secs / 60).to_string()).color(theme::TEXT_DIM));
                    ui.label(graded_count(l.pts, &counts[0]));
                    for (made, tried, par, col) in [
                        (l.fgm, l.fga, 0.45, &fg),
                        (l.tpm, l.tpa, 0.34, &three),
                        (l.ftm, l.fta, 0.72, &ft),
                    ] {
                        let text = format!("{made}/{tried}");
                        ui.label(if tried == 0 {
                            faded("-")
                        } else {
                            graded(text, shooting(made, tried, par), col, false)
                        });
                    }
                    for (v, col) in [l.reb(), l.ast, l.stl, l.blk].into_iter().zip(&counts[1..]) {
                        ui.label(graded_count(v, col));
                    }
                    // Turnovers and fouls: quiet unless they pile up.
                    for (v, many) in [(l.tov, 4), (l.pf, self.world.rules.foul_limit - 1)] {
                        let text = theme::mono(v.to_string());
                        ui.label(if v >= many {
                            text.color(theme::WARN)
                        } else {
                            text.color(theme::TEXT_MUTE)
                        });
                    }
                    ui.label(graded(
                        format!("{:+}", l.pm),
                        l.pm as f32,
                        &plus_minus,
                        false,
                    ));
                    ui.label(match_rating(l.rating()));
                    ui.end_row();
                }
            });
    }
}

impl eframe::App for App {
    /// `SHOT_CLICK="80,420;300,200"` clicks those points one after another
    /// before a screenshot is taken, so what a click does can be checked
    /// without a person at the window.
    #[cfg(not(target_arch = "wasm32"))]
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        let Some((_, frame)) = &self.shot else { return };
        // The real mouse stays out of it, wherever it happens to be.
        raw.events.retain(|e| {
            !matches!(
                e,
                egui::Event::PointerMoved(_)
                    | egui::Event::PointerButton { .. }
                    | egui::Event::PointerGone
                    | egui::Event::MouseMoved(_)
            )
        });
        let Ok(clicks) = std::env::var("SHOT_CLICK") else {
            return;
        };
        // "x,y" is a click; "x,y>x,y" a drag from the first point to the
        // second.
        let point = |t: &str| -> Option<egui::Pos2> {
            let (x, y) = t.split_once(',')?;
            Some(egui::pos2(x.trim().parse().ok()?, y.trim().parse().ok()?))
        };
        let steps: Vec<(egui::Pos2, egui::Pos2)> = clicks
            .split(';')
            .filter_map(|c| match c.split_once('>') {
                Some((a, b)) => Some((point(a)?, point(b)?)),
                None => point(c).map(|p| (p, p)),
            })
            .collect();
        // One step every ten frames from frame 40: move, press, (drag),
        // release.
        let Some(n) = frame.checked_sub(40) else {
            return;
        };
        let Some(&(pos, to)) = steps.get((n / 10) as usize) else {
            return;
        };
        let button = |at, pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        match n % 10 {
            0 => raw.events.push(egui::Event::PointerMoved(pos)),
            2 => raw.events.push(button(pos, true)),
            3 if to != pos => raw
                .events
                .push(egui::Event::PointerMoved(pos + (to - pos) * 0.5)),
            4 if to != pos => raw.events.push(egui::Event::PointerMoved(to)),
            5 => raw.events.push(button(to, false)),
            _ => {}
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.persist {
            storage.set_string(VIEW_KEY, self.look.mode.key().to_string());
            storage.set_string(HUD_KEY, self.sh.hud.prefs.encode());
        }
        if self.may_save() {
            persist::write(storage, &self.world);
            self.saved_at = Some(self.save_mark());
        }
    }

    /// The calendar rarely moves, so this is only a safety net.
    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(300)
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let _perf = crate::perf::scope("frame: all ui");
        self.follow_watch();
        // What eframe measured for the frame before: our code plus turning
        // the shapes into triangles, without the GPU's share.
        if let Some(cpu) = frame.info().cpu_usage {
            crate::perf::add("eframe: ui+tessellate", cpu as f64 * 1000.0);
        }
        // The web page around the game says what the visitor chose by
        // setting the address: `#continue` goes straight into the saved
        // career, `#new` to the club picker.
        #[cfg(target_arch = "wasm32")]
        {
            let hash = frame.info().web_info.location.hash.clone();
            if hash != self.web_hash {
                match hash.as_str() {
                    "#continue" if self.career => self.started = true,
                    "#new" => self.started = false,
                    _ => {}
                }
                self.web_hash = hash;
            }
        }
        // Save when the calendar has moved (a day, a round, a game, the
        // draft): not every frame, and not in the middle of a live game.
        if self.may_save()
            && self.saved_at != Some(self.save_mark())
            && let Some(storage) = frame.storage_mut()
        {
            persist::write(storage, &self.world);
            self.saved_at = Some(self.save_mark());
        }
        // The title screen is the photograph; the game proper is a place.
        self.backdrop.inset = self.started;
        if !self.started {
            let _p = crate::perf::scope("backdrop");
            self.backdrop.paint(ui);
        }
        // F3 shows how fast the game is drawing, for when it feels slow.
        if ui.input(|i| i.key_pressed(egui::Key::F3)) {
            self.show_speed = !self.show_speed;
        }
        if self.show_speed {
            let (dt, ppp, screen) =
                ui.input(|i| (i.stable_dt, i.pixels_per_point, i.content_rect()));
            let work = frame.info().cpu_usage.unwrap_or(0.0) * 1000.0;
            let text = format!(
                "{:.0} frames a second   {:.1} ms of work a frame   {:.0} x {:.0} pixels",
                1.0 / dt.max(1e-4),
                work,
                screen.width() * ppp,
                screen.height() * ppp
            );
            let painter = ui.ctx().debug_painter();
            let at = screen.left_bottom() + vec2(10.0, -10.0);
            let galley = painter.layout_no_wrap(text, FontId::monospace(12.0), Color32::WHITE);
            let back = Rect::from_min_size(
                at - vec2(4.0, galley.size().y + 4.0),
                galley.size() + vec2(8.0, 8.0),
            );
            painter.rect_filled(back, 4.0, Color32::from_black_alpha(190));
            painter.galley(back.min + vec2(4.0, 4.0), galley, Color32::WHITE);
            ui.ctx().request_repaint();
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.capture(ui);
        let bd = self.backdrop.clone();
        self.search_keys(ui.ctx());
        self.match_keys(ui.ctx());
        if !self.started {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| self.title(ui, &bd));
            return;
        }

        self.shell(ui);
        self.search_overlay(ui.ctx());
    }
}
