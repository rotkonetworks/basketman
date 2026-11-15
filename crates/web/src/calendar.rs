//! The calendar page: a month of the club's games, friendlies, reserve games
//! and the league's dated events, with the day's whole league under it.

use basketman_core::calendar::{CalEvent, CalKind, days_from_civil, days_in_month, weekday_of};
use eframe::egui::{self, Align2, FontFamily, FontId, RichText, Sense, Stroke, Ui, pos2, vec2};

use crate::app::App;
use crate::nav::{Link, link};
use crate::theme::{self, Backdrop};

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn kind_color(kind: CalKind) -> egui::Color32 {
    match kind {
        CalKind::League => theme::ACCENT,
        CalKind::Playoff => theme::WARN,
        CalKind::Friendly => theme::TEXT_DIM,
        CalKind::Reserve => theme::TEXT_MUTE,
        CalKind::Marker | CalKind::Draft => theme::POS,
    }
}

impl App {
    fn mine(&self, e: &CalEvent) -> bool {
        e.home == Some(self.team) || e.away == Some(self.team)
    }

    /// "W 88-80", "L 71-75" or "v Riga" for one of our games.
    fn cell_text(&self, e: &CalEvent, wide: bool) -> String {
        let me = self.team;
        let home = e.home == Some(me);
        let opp = if home { e.away } else { e.home };
        let name = opp.map_or(String::new(), |t| {
            let n = &self.world.teams[t].name;
            n.split(' ').next().unwrap_or(n).to_string()
        });
        match e.score {
            Some(s) => {
                let (us, them) = if home { (s[0], s[1]) } else { (s[1], s[0]) };
                let r = if us > them { "W" } else { "L" };
                let f = if e.kind == CalKind::Friendly { "F " } else { "" };
                if wide {
                    format!("{f}{r} {us}-{them}")
                } else {
                    format!("{f}{r}")
                }
            }
            None => {
                let ha = if e.kind == CalKind::Friendly {
                    "F"
                } else if home {
                    "H"
                } else {
                    "A"
                };
                if wide {
                    format!("{ha} {name}")
                } else {
                    ha.to_string()
                }
            }
        }
    }

    pub(crate) fn calendar_page(&mut self, ui: &mut Ui, _bd: &Backdrop) {
        let narrow = ui.available_width() < 520.0;
        let today = self.world.day;
        let today_date = self.world.date_of(today);
        let (year, month) = self
            .sh
            .cal_month
            .unwrap_or((today_date.year, today_date.month));
        let phase = self.world.view_calendar().phase;

        theme::eyebrow(ui, "Calendar");
        ui.horizontal(|ui| {
            if ui.button("<").clicked() {
                self.sh.cal_month = Some(if month == 1 {
                    (year - 1, 12)
                } else {
                    (year, month - 1)
                });
            }
            ui.label(
                RichText::new(format!(
                    "{} {year}",
                    basketman_core::calendar::MONTHS[month as usize - 1]
                ))
                .font(FontId::new(20.0, theme::semibold()))
                .color(theme::TEXT),
            );
            if ui.button(">").clicked() {
                self.sh.cal_month = Some(if month == 12 {
                    (year + 1, 1)
                } else {
                    (year, month + 1)
                });
            }
            if ui.button("Today").clicked() {
                self.sh.cal_month = None;
                self.sh.cal_day = Some(today);
            }
        });
        let status = match phase {
            "preseason" => "Training camp. Friendlies do not count in the table.",
            "playoffs" => "The playoffs.",
            "offseason" => "The season is over. The draft is coming.",
            _ => "The league season.",
        };
        ui.label(
            RichText::new(format!("{}. {status}", self.world.day_long(today)))
                .size(12.5)
                .color(theme::TEXT_DIM),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            self.continue_button(ui, narrow);
        });
        ui.add_space(6.0);

        // ---- the month ----------------------------------------------------
        let first_abs = days_from_civil(year as i64, month as i64, 1);
        let dim = days_in_month(year, month) as i64;
        let start = self.world.season_start_abs();
        let last_day = first_abs + dim - 1 - start;
        let events: Vec<CalEvent> = if last_day >= 0 {
            self.world
                .calendar_events((first_abs - start).max(0) as u32, last_day as u32)
        } else {
            Vec::new()
        };
        let lead = weekday_of(first_abs) as i64;
        let rows = (lead + dim + 6) / 7;
        let width = ui.available_width();
        let cell_w = width / 7.0;
        let cell_h = if narrow { 54.0 } else { 72.0 };
        let (head, _) = ui.allocate_exact_size(vec2(width, 18.0), Sense::hover());
        for (i, d) in WEEKDAYS.iter().enumerate() {
            ui.painter().text(
                pos2(head.left() + cell_w * (i as f32 + 0.5), head.center().y),
                Align2::CENTER_CENTER,
                d,
                FontId::new(10.0, FontFamily::Monospace),
                theme::TEXT_MUTE,
            );
        }
        let wide = cell_w > 76.0;
        for row in 0..rows {
            let (strip, _) = ui.allocate_exact_size(vec2(width, cell_h), Sense::hover());
            for col in 0..7i64 {
                let n = row * 7 + col - lead + 1;
                if n < 1 || n > dim {
                    continue;
                }
                let rect = egui::Rect::from_min_size(
                    pos2(strip.left() + cell_w * col as f32, strip.top()),
                    vec2(cell_w, cell_h),
                )
                .shrink(1.0);
                let day = first_abs + n - 1 - start;
                let resp = ui.interact(rect, ui.make_persistent_id(("cal", year, month, n)), Sense::click());
                let p = ui.painter().with_clip_rect(rect);
                let chosen = day >= 0 && self.sh.cal_day == Some(day as u32);
                let is_today = day == today as i64;
                p.rect_filled(
                    rect,
                    6.0,
                    if chosen {
                        theme::ACCENT.gamma_multiply(0.16)
                    } else if resp.hovered() {
                        theme::LINE
                    } else {
                        theme::ink(6)
                    },
                );
                if is_today {
                    p.rect_stroke(
                        rect,
                        6.0,
                        Stroke::new(1.5, theme::ACCENT),
                        egui::StrokeKind::Inside,
                    );
                }
                p.text(
                    rect.left_top() + vec2(4.0, 3.0),
                    Align2::LEFT_TOP,
                    n.to_string(),
                    FontId::new(10.5, FontFamily::Monospace),
                    if day >= 0 && day < today as i64 {
                        theme::TEXT_MUTE
                    } else {
                        theme::TEXT
                    },
                );
                if day >= 0 {
                    let on: Vec<&CalEvent> = events.iter().filter(|e| e.day == day as u32).collect();
                    let mut y = rect.top() + 17.0;
                    // Our games, friendlies and reserves; league-wide
                    // markers get a coloured dot.
                    for e in on.iter().filter(|e| self.mine(e)) {
                        let reserve = e.kind == CalKind::Reserve;
                        let text = if reserve {
                            format!("R {}", self.cell_text(e, false))
                        } else {
                            self.cell_text(e, wide)
                        };
                        let won = e.score.is_some_and(|s| {
                            let home = e.home == Some(self.team);
                            (s[0] > s[1]) == home
                        });
                        let color = match e.score {
                            Some(_) if won => theme::POS,
                            Some(_) => theme::WARN,
                            None => kind_color(e.kind),
                        };
                        p.text(
                            pos2(rect.left() + 4.0, y),
                            Align2::LEFT_TOP,
                            text,
                            FontId::new(if reserve { 9.5 } else { 11.0 }, theme::mono_semibold()),
                            color,
                        );
                        y += 13.0;
                    }
                    for e in on
                        .iter()
                        .filter(|e| matches!(e.kind, CalKind::Marker | CalKind::Draft))
                    {
                        let label = if narrow { "*" } else { e.title.as_str() };
                        p.text(
                            pos2(rect.left() + 4.0, y),
                            Align2::LEFT_TOP,
                            label,
                            FontId::new(9.5, FontFamily::Proportional),
                            kind_color(e.kind),
                        );
                        y += 12.0;
                    }
                }
                if resp.clicked() && day >= 0 {
                    self.sh.cal_day = Some(day as u32);
                }
            }
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Purple: our league game. Orange: playoff. Grey: friendly. R: reserves. F: friendly. H home, A away.",
            )
            .size(11.0)
            .color(theme::TEXT_MUTE),
        );
        ui.add_space(8.0);

        // ---- the chosen day -----------------------------------------------
        let day = self.sh.cal_day.unwrap_or(today);
        theme::eyebrow(ui, &self.world.day_long(day));
        let on = self.world.events_on(day);
        if on.is_empty() {
            ui.label(
                RichText::new("Nothing is scheduled on this day.")
                    .size(13.0)
                    .color(theme::TEXT_DIM),
            );
            return;
        }
        for e in on
            .iter()
            .filter(|e| matches!(e.kind, CalKind::Marker | CalKind::Draft))
        {
            ui.label(
                RichText::new(&e.title)
                    .family(theme::semibold())
                    .color(kind_color(e.kind)),
            );
        }
        let games: Vec<&CalEvent> = on
            .iter()
            .filter(|e| !matches!(e.kind, CalKind::Marker | CalKind::Draft))
            .collect();
        let mut last: Option<(CalKind, usize)> = None;
        // Ours first, then the rest by kind and division.
        let mut order: Vec<&CalEvent> = games.clone();
        order.sort_by_key(|e| (!self.mine(e), e.kind as u8, e.division));
        for e in order {
            if last != Some((e.kind, e.division)) || self.mine(e) {
                let heading = match e.kind {
                    CalKind::League => "League game",
                    CalKind::Playoff => "Playoff game",
                    CalKind::Friendly => "Friendly",
                    CalKind::Reserve => "Reserve game",
                    _ => "",
                };
                let div = if self.world.division_count() > 1 {
                    format!(" / {}", self.world.division_name(e.division))
                } else {
                    String::new()
                };
                ui.add_space(4.0);
                ui.label(theme::eyebrow_text(&format!(
                    "{heading}{div}{}",
                    if self.mine(e) { " / yours" } else { "" }
                )));
                last = Some((e.kind, e.division));
            }
            self.event_row(ui, e);
        }
    }

    fn event_row(&self, ui: &mut Ui, e: &CalEvent) {
        let (Some(h), Some(a)) = (e.home, e.away) else {
            return;
        };
        let mine = self.mine(e);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            let name = |t: usize| {
                let c = if t == self.team {
                    theme::ACCENT
                } else {
                    theme::TEXT
                };
                RichText::new(&self.world.teams[t].name)
                    .size(13.5)
                    .color(c)
            };
            link(ui, Link::Club(h), name(h));
            let middle = match e.score {
                Some(s) => format!("{} - {}", s[0], s[1]),
                None => "v".to_string(),
            };
            let text = theme::mono(middle).color(if mine {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            });
            match e.fixture {
                Some(i) if e.score.is_some() => {
                    link(ui, Link::Game(i), text);
                }
                _ => {
                    ui.label(text);
                }
            }
            link(ui, Link::Club(a), name(a));
            if e.score.is_none() {
                ui.label(RichText::new("not played").size(11.0).color(theme::TEXT_MUTE));
            }
        });
    }
}
