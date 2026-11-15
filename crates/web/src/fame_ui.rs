//! Fame, fans and media work on screen: the player's public life on his
//! page, the fame meter and media control used in tables, the Fans and
//! shirt-sales cards on Finances, and the names and hall of fame on the
//! League page.
//!
//! Drawing only reads the world, except the media workload, which goes
//! through `Command::SetMedia`.

use basketman_core::command::Command;
use basketman_core::fame::fame_words;
use basketman_core::fans::CAUSES;
use basketman_core::media::{MediaLevel, presence_words};
use basketman_core::player::PlayerId;
use eframe::egui::{self, Color32, RichText, Sense, Ui, vec2};

use crate::app::{App, GAP, header_row, money, name};
use crate::pages::dim;
use crate::theme::{self, Backdrop};

/// A thin meter for fame, 0..=100.
pub(crate) fn fame_meter(ui: &mut Ui, fame: f32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 8.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, theme::LINE);
    let mut fill = rect;
    fill.set_width(rect.width() * (fame / 100.0).clamp(0.0, 1.0));
    let color = if fame >= 40.0 {
        theme::ACCENT
    } else {
        Color32::from_rgb(0x9A, 0x8F, 0xD6)
    };
    p.rect_filled(fill, 4.0, color);
}

/// Fame as a small meter and its number, for a table row.
pub(crate) fn fame_chip(ui: &mut Ui, fame: f32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        fame_meter(ui, fame, 46.0);
        ui.label(theme::mono(format!("{fame:.0}")).color(if fame >= 20.0 {
            theme::TEXT
        } else {
            theme::TEXT_MUTE
        }));
    })
    .response
    .on_hover_text(fame_words(fame));
}

/// The media workload as a combo box; returns the level chosen.
pub(crate) fn media_combo(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    now: MediaLevel,
) -> MediaLevel {
    let mut level = now;
    egui::ComboBox::from_id_salt(id)
        .width(70.0)
        .selected_text(level.label())
        .show_ui(ui, |ui| {
            for l in MediaLevel::ALL {
                ui.selectable_value(&mut level, l, l.label());
            }
        });
    level
}

fn signed_people(v: f32) -> RichText {
    theme::mono(format!("{v:+.0}")).color(if v >= 0.0 { theme::POS } else { theme::WARN })
}

impl App {
    /// A player's public life: fame, shirts, media work, presence, honours.
    pub(crate) fn public_life(&mut self, ui: &mut Ui, bd: &Backdrop, pid: PlayerId) {
        let w = &self.world;
        let p = &w.players[pid as usize];
        let r = &p.renown;
        let mine = self.is_mine(pid);
        let club = w.team_of(pid);
        let mut set: Option<MediaLevel> = None;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Public life");
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(30.0, 12.0);
                ui.vertical(|ui| {
                    ui.label(theme::eyebrow_text("Fame"));
                    ui.label(
                        RichText::new(fame_words(r.fame))
                            .font(egui::FontId::new(18.0, theme::semibold())),
                    );
                    ui.horizontal(|ui| {
                        fame_meter(ui, r.fame, 150.0);
                        ui.label(
                            theme::mono(format!("{:.0} / 100", r.fame)).color(theme::TEXT_DIM),
                        );
                    });
                });
                if let Some(t) = club {
                    let sells = w.player_merch(pid);
                    ui.vertical(|ui| {
                        ui.label(theme::eyebrow_text("Shirt sales"));
                        ui.label(
                            RichText::new(format!("{} a week", money(sells)))
                                .font(egui::FontId::new(18.0, theme::semibold())),
                        );
                        ui.label(dim(format!("{} this season", money(r.shirts))));
                    });
                    let years = if r.tenure_club == Some(t) {
                        r.tenure
                    } else {
                        0
                    };
                    ui.vertical(|ui| {
                        ui.label(theme::eyebrow_text("Years at the club"));
                        ui.label(
                            RichText::new(years.to_string())
                                .font(egui::FontId::new(18.0, theme::semibold())),
                        );
                        ui.label(dim(if years >= 4 {
                            "a local hero: bigger at home"
                        } else {
                            "pull is the same at home"
                        }));
                    });
                }
                ui.vertical(|ui| {
                    ui.label(theme::eyebrow_text("In front of the cameras"));
                    let known = mine || w.scouting.knowledge(pid) >= 0.6;
                    if mine {
                        ui.label(
                            RichText::new(presence_words(p.personality.presence))
                                .font(egui::FontId::new(16.0, theme::semibold())),
                        );
                        ui.label(dim(format!(
                            "{}. Media work with a press officer improves it.",
                            p.personality.kind().label()
                        )));
                    } else if known {
                        ui.label(RichText::new(p.personality.kind().label()));
                        ui.label(dim("what the scouts make of his character"));
                    } else {
                        ui.label(dim("Unknown: needs more scouting"));
                    }
                });
            });
            if r.farewell {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "Farewell season: he has said this is his last. Halls fill to see him \
                         wherever he plays.",
                    )
                    .color(theme::WARN),
                );
            }
            if let Some(s) = r.retired {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("Retired after season {}.", s + 1))
                        .color(theme::TEXT_DIM)
                        .family(theme::semibold()),
                );
            }
            let count = |n: u8, one: &str, many: &str| match n {
                0 => None,
                1 => Some(format!("1 {one}")),
                n => Some(format!("{n} {many}")),
            };
            let honours: Vec<String> = [
                count(
                    r.mvps,
                    "most valuable player award",
                    "most valuable player awards",
                ),
                count(r.titles, "title", "titles"),
                count(r.finals, "final reached", "finals reached"),
                count(r.scoring, "scoring title", "scoring titles"),
                count(
                    r.elite,
                    "season at the top level",
                    "seasons at the top level",
                ),
            ]
            .into_iter()
            .flatten()
            .collect();
            ui.add_space(8.0);
            ui.label(theme::eyebrow_text("Career honours"));
            if honours.is_empty() {
                ui.label(dim("None yet."));
            } else {
                ui.label(honours.join(", ") + ".");
            }
            if mine && r.retired.is_none() {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(theme::eyebrow_text("Media work each week"));
                    let now = r.media;
                    let level = media_combo(ui, ("media_profile", pid), now);
                    if level != now {
                        set = Some(level);
                    }
                    ui.label(dim(
                        "It lifts his name and the club's following, and takes some of his \
                         training and freshness.",
                    ));
                });
            }
        });
        if let Some(level) = set {
            let _ = self.world.apply(Command::SetMedia { player: pid, level });
        }
    }

    /// Shirt sales by player, and the Fans card.
    pub(crate) fn merch_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        let top = w.top_sellers(self.team, 6);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Shirt sales by player");
            ui.add_space(6.0);
            egui::Grid::new("fin_shirts")
                .spacing(vec2(24.0, 5.0))
                .show(ui, |ui| {
                    header_row(ui, &["Player", "Fame", "A week", "This season"]);
                    ui.end_row();
                    for (p, per) in top {
                        let pl = &w.players[p as usize];
                        crate::profile::player_link(ui, p, name(&pl.name));
                        fame_chip(ui, pl.renown.fame);
                        ui.label(theme::mono(money(per)));
                        ui.label(theme::mono(money(pl.renown.shirts)));
                        ui.end_row();
                    }
                });
            ui.add_space(4.0);
            ui.label(dim(
                "What a name sells follows his fame, how he is playing and how big the \
                 following is. The club's own colours sell on top of this.",
            ));
        });
    }

    pub(crate) fn fans_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        let me = self.team;
        let trend = w.fan_trend(me);
        let moved = if w
            .fame
            .moved
            .get(me)
            .is_some_and(|m| m.iter().any(|v| *v != 0.0))
        {
            w.fans_moved(me)
        } else {
            w.fame.last_moved.get(me).copied().unwrap_or_default()
        };
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Fans");
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 30.0;
                ui.vertical(|ui| {
                    ui.label(theme::eyebrow_text("Supporters"));
                    ui.label(
                        RichText::new(money(w.fan_base(me) as i64))
                            .font(egui::FontId::new(22.0, theme::semibold())),
                    );
                });
                ui.vertical(|ui| {
                    ui.label(theme::eyebrow_text("This season"));
                    ui.label(
                        RichText::new(format!("{:+.1}%", trend * 100.0))
                            .font(egui::FontId::new(22.0, theme::semibold()))
                            .color(if trend >= 0.0 {
                                theme::POS
                            } else {
                                theme::WARN
                            }),
                    );
                });
                ui.vertical(|ui| {
                    ui.label(theme::eyebrow_text("Next to the hall"));
                    ui.label(dim(match w.fan_index(me) {
                        i if i >= 0.75 => "far more than the seats can hold",
                        i if i >= 0.55 => "plenty for the hall",
                        i if i >= 0.4 => "about right for the hall",
                        _ => "few for the hall",
                    }));
                });
            });
            let past = &w.fame.past;
            if !past.is_empty() {
                ui.add_space(8.0);
                ui.label(theme::eyebrow_text("Season by season"));
                let line: Vec<String> = past
                    .iter()
                    .enumerate()
                    .rev()
                    .take(5)
                    .rev()
                    .map(|(s, v)| format!("{}: {}", s + 1, money(v[me] as i64)))
                    .collect();
                ui.label(theme::mono(line.join("   ")).color(theme::TEXT_DIM));
            }
            ui.add_space(8.0);
            ui.label(theme::eyebrow_text(
                if w.fame
                    .moved
                    .get(me)
                    .is_some_and(|m| m.iter().any(|v| *v != 0.0))
                {
                    "What moved it this season"
                } else {
                    "What moved it last season"
                },
            ));
            egui::Grid::new("fin_fans")
                .spacing(vec2(24.0, 4.0))
                .show(ui, |ui| {
                    for (i, cause) in CAUSES.iter().enumerate() {
                        ui.label(dim(*cause));
                        ui.label(signed_people(moved[i]));
                        ui.end_row();
                    }
                });
            ui.add_space(4.0);
            ui.label(dim(
                "Winning, playoff runs, names on the team, exciting games, low prices and \
                 media work bring supporters. Losing, dear tickets and a favourite benched or \
                 sold cost them.",
            ));
        });
    }

    /// The biggest names, and the hall of fame.
    pub(crate) fn names_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Biggest names in the league");
            ui.add_space(6.0);
            egui::Grid::new("names")
                .striped(true)
                .spacing(vec2(22.0, 6.0))
                .show(ui, |ui| {
                    header_row(ui, &["Player", "Club", "Age", "Fame", ""]);
                    ui.end_row();
                    for (p, f) in w.biggest_names(10) {
                        let pl = &w.players[p as usize];
                        crate::profile::player_link(ui, p, name(&pl.name));
                        match w.team_of(p) {
                            Some(t) => self.club_link(ui, t),
                            None => ui.label(dim("-")),
                        };
                        ui.label(theme::mono(pl.age.to_string()));
                        fame_chip(ui, f);
                        let sigs: Vec<&str> =
                            w.known_signatures(p).iter().map(|s| s.name()).collect();
                        ui.horizontal(|ui| {
                            ui.label(if pl.renown.farewell {
                                RichText::new("farewell season").color(theme::WARN)
                            } else {
                                dim(fame_words(f))
                            });
                            if !sigs.is_empty() {
                                ui.label(RichText::new(sigs.join(", ")).color(theme::ACCENT))
                                    .on_hover_text("Signature move");
                            }
                        });
                        ui.end_row();
                    }
                });
        });
    }

    pub(crate) fn hall_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        let hall = w.hall_of_fame();
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Hall of fame");
            ui.add_space(6.0);
            if hall.is_empty() {
                ui.label(dim(
                    "Nobody yet. The league's greats are elected when they retire.",
                ));
                return;
            }
            egui::Grid::new("hall")
                .striped(true)
                .spacing(vec2(22.0, 6.0))
                .show(ui, |ui| {
                    header_row(ui, &["Player", "Retired", "Awards", "Titles", "Career"]);
                    ui.end_row();
                    for h in hall.iter().take(12) {
                        let r = &w.players[h.player as usize].renown;
                        crate::profile::player_link(
                            ui,
                            h.player,
                            name(&w.players[h.player as usize].name),
                        );
                        ui.label(theme::mono(format!("season {}", h.season + 1)));
                        ui.label(theme::mono(r.mvps.to_string()));
                        ui.label(theme::mono(r.titles.to_string()));
                        ui.label(theme::mono(format!("{:.0}", h.score)));
                        ui.end_row();
                    }
                });
        });
        let _ = GAP;
    }
}
