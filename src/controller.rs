/*
ENSnano, a 3d graphical application for DNA nanostructures.
    Copyright (C) 2021  Nicolas Levy <nicolaspierrelevy@gmail.com> and Nicolas Schabanel <nicolas.schabanel@ens-lyon.fr>

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

//! This modules defines the `Controller` struct which handles windows and dialog interactions.

use ensnano_design::Nucl;
mod download_staples;
use download_staples::*;
pub use download_staples::{DownloadStappleError, DownloadStappleOk, StaplesDownloader};
mod quit;
use ensnano_interactor::{application::Notification, DesignOperation};
use ensnano_interactor::{DesignReader, RigidBodyConstants, Selection};
use quit::*;
mod set_scaffold_sequence;
use set_scaffold_sequence::*;
pub use set_scaffold_sequence::{ScaffoldSetter, SetScaffoldSequenceError, SetScaffoldSequenceOk};
mod chanel_reader;
mod normal_state;
pub use chanel_reader::{ChanelReader, ChanelReaderUpdate};
pub use normal_state::Action;
use normal_state::NormalState;

use std::path::PathBuf;

use super::dialog;
use super::{gui::UiSize, OverlayType, SplitMode};
use dialog::MustAckMessage;
use std::borrow::Cow;
use std::sync::{Arc, Mutex};

pub struct Controller {
    state: Box<dyn State + 'static>,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            /// The sate of the windows
            state: Box::new(NormalState),
        }
    }

    pub(crate) fn make_progress(&mut self, main_state: &mut dyn MainState) {
        let old_state = std::mem::replace(&mut self.state, Box::new(OhNo));
        self.state = old_state.make_progress(main_state);
    }
}

trait State {
    fn make_progress(self: Box<Self>, main_state: &mut dyn MainState) -> Box<dyn State>;
}

struct OhNo;

impl State for OhNo {
    fn make_progress(self: Box<Self>, _: &mut dyn MainState) -> Box<dyn State> {
        panic!("Oh No !")
    }
}

/// Display a message that must be acknowledged by the user, and transition to a predetermined
/// state.
struct TransitionMessage {
    level: rfd::MessageLevel,
    content: String,
    ack: Option<MustAckMessage>,
    transistion_to: Box<dyn State>,
}

impl TransitionMessage {
    fn new(
        content: String,
        level: rfd::MessageLevel,
        transistion_to: Box<dyn State + 'static>,
    ) -> Box<Self> {
        Box::new(Self {
            level,
            content,
            ack: None,
            transistion_to,
        })
    }
}

impl State for TransitionMessage {
    fn make_progress(mut self: Box<Self>, _: &mut dyn MainState) -> Box<dyn State + 'static> {
        if let Some(ack) = self.ack.as_ref() {
            if ack.was_ack() {
                self.transistion_to
            } else {
                self
            }
        } else {
            let ack =
                dialog::blocking_message(self.content.clone().into(), clone_msg_level(&self.level));
            self.ack = Some(ack);
            self
        }
    }
}

fn clone_msg_level(level: &rfd::MessageLevel) -> rfd::MessageLevel {
    match level {
        rfd::MessageLevel::Warning => rfd::MessageLevel::Warning,
        rfd::MessageLevel::Info => rfd::MessageLevel::Info,
        rfd::MessageLevel::Error => rfd::MessageLevel::Error,
    }
}

use dialog::YesNoQuestion;
/// Ask the user a yes/no question and transition to a state that depends on their answer.
struct YesNo {
    question: Cow<'static, str>,
    answer: Option<YesNoQuestion>,
    yes: Box<dyn State>,
    no: Box<dyn State>,
}

impl YesNo {
    fn new(question: Cow<'static, str>, yes: Box<dyn State>, no: Box<dyn State>) -> Self {
        Self {
            question,
            yes,
            no,
            answer: None,
        }
    }
}

impl State for YesNo {
    fn make_progress(mut self: Box<Self>, _: &mut dyn MainState) -> Box<dyn State> {
        if let Some(ans) = self.answer.as_ref() {
            if let Some(b) = ans.answer() {
                if b {
                    self.yes
                } else {
                    self.no
                }
            } else {
                self
            }
        } else {
            let yesno = dialog::yes_no_dialog(self.question.clone().into());
            self.answer = Some(yesno);
            self
        }
    }
}

use ultraviolet::{Rotor3, Vec3};
pub(crate) trait MainState: ScaffoldSetter {
    fn pop_action(&mut self) -> Option<Action>;
    fn exit_control_flow(&mut self);
    fn new_design(&mut self);
    fn load_design(&mut self, path: PathBuf) -> Result<(), LoadDesignError>;
    fn save_design(&mut self, path: &PathBuf) -> Result<(), SaveDesignError>;
    fn get_chanel_reader(&mut self) -> &mut ChanelReader;
    fn apply_operation(&mut self, operation: DesignOperation);
    fn apply_silent_operation(&mut self, operation: DesignOperation);
    fn undo(&mut self);
    fn redo(&mut self);
    fn get_staple_downloader(&self) -> Box<dyn StaplesDownloader>;
    fn toggle_split_mode(&mut self, mode: SplitMode);
    fn oxdna_export(&mut self, path: &PathBuf) -> std::io::Result<(PathBuf, PathBuf)>;
    fn change_ui_size(&mut self, ui_size: UiSize);
    fn invert_scroll_y(&mut self, inverted: bool);
    fn notify_apps(&mut self, notificiation: Notification);
    fn get_selection(&mut self) -> Box<dyn AsRef<[Selection]>>;
    fn get_design_reader(&mut self) -> Box<dyn DesignReader>;
    fn get_grid_creation_position(&self) -> Option<(Vec3, Rotor3)>;
    fn finish_operation(&mut self);
    fn request_copy(&mut self);
    fn request_pasting_candidate(&mut self, candidate: Option<Nucl>);
    fn init_paste(&mut self);
    fn apply_paste(&mut self);
    fn duplicate(&mut self);
    fn delete_selection(&mut self);
    fn scaffold_to_selection(&mut self);
    fn start_helix_simulation(&mut self, parameters: RigidBodyConstants);
}

pub struct LoadDesignError(String);
pub struct SaveDesignError(String);

impl From<String> for LoadDesignError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl<E: std::error::Error> From<E> for SaveDesignError {
    fn from(e: E) -> Self {
        Self(format!("{}", e))
    }
}

pub enum DesignAction {}
