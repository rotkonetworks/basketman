//! Signature moves on screen: the card on a player's page and the small
//! marker beside a name in the tables. Only moves the club knows about are
//! ever drawn (`World::known_signatures`): nothing here hints at the rest.

use basketman_core::player::PlayerId;
use basketman_core::signature::Signature;
use eframe::egui::{self, RichText, Ui};

use crate::app::{App, GAP};
use crate::pages::dim;
use crate::theme::{self, Backdrop};

/// A small marker for a player with a known signature move; hover lists
/// them. Draws nothing for none.
pub(crate) fn mark(ui: &mut Ui, sigs: &[Signature]) {
    if sigs.is_empty() {
        return;
    }
    let names: Vec<&str> = sigs.iter().map(|s| s.name()).collect();
    let text = if sigs.len() == 1 {
        "Sig".to_string()
    } else {
        format!("Sig {}", sigs.len())
    };
    ui.label(RichText::new(text).size(crate::theme::fs(10.5)).strong().color(theme::ACCENT))
        .on_hover_text(format!("Signature: {}", names.join(", ")));
}

/// A name that is a link to the profile, with the marker after it.
pub(crate) fn name_with_mark(ui: &mut Ui, pid: PlayerId, text: RichText, sigs: &[Signature]) {
    if sigs.is_empty() {
        crate::profile::player_link(ui, pid, text);
        return;
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        crate::profile::player_link(ui, pid, text);
        mark(ui, sigs);
    });
}

impl App {
    /// His signatures the manager may know of.
    pub(crate) fn known_signatures(&self, pid: PlayerId) -> Vec<Signature> {
        self.world.known_signatures(pid)
    }

    /// The Signature card of a profile. Does nothing if none is known.
    pub(crate) fn signature_card(&self, ui: &mut Ui, bd: &Backdrop, pid: PlayerId) {
        let sigs = self.known_signatures(pid);
        if sigs.is_empty() {
            return;
        }
        let p = &self.world.players[pid as usize];
        let mine = self.is_mine(pid);
        // A scout sees whether the body is still up to it only once he
        // knows the man well; the club's own staff always do.
        let body_known =
            mine || self.world.scouting.knowledge(pid) >= basketman_core::view::BODY_KNOWN_AT;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, if sigs.len() == 1 { "Signature" } else { "Signatures" });
            ui.add_space(6.0);
            for (i, s) in sigs.iter().enumerate() {
                if i > 0 {
                    ui.add_space(10.0);
                }
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(s.name())
                            .font(egui::FontId::new(16.0, theme::semibold()))
                            .color(theme::TEXT),
                    );
                    ui.label(dim(s.group().label()));
                });
                ui.label(s.description());
                ui.label(
                    RichText::new(format!("The answer: {}", s.answer())).color(theme::TEXT_DIM),
                );
                if body_known && !s.live(p) {
                    ui.label(
                        RichText::new("He can no longer do it: his body is not up to it.")
                            .color(theme::WARN),
                    );
                }
            }
            if mine {
                ui.add_space(8.0);
                ui.label(dim(
                    "A signature is part of him. It cannot be taught and training will not change it.",
                ));
            }
        });
        ui.add_space(GAP);
    }
}
