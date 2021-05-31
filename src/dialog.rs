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

use super::{KeepProceed, Requests};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use std::borrow::Cow;
/// A question to which the user must answer yes or no
pub struct YesNoQuestion(mpsc::Receiver<bool>);
impl YesNoQuestion {
    pub fn answer(&self) -> Option<bool> {
        self.0.recv().ok()
    }
}

pub fn yes_no_dialog(message: Cow<'static, str>) -> YesNoQuestion {
    let msg = rfd::AsyncMessageDialog::new()
        .set_description(message.as_ref())
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();
    let (snd, rcv) = mpsc::channel();
    thread::spawn(move || {
        let choice = async move {
            println!("thread spawned");
            let ret = msg.await;
            println!("about to send");
            snd.send(ret);
        };
        futures::executor::block_on(choice);
    });
    YesNoQuestion(rcv)
}

pub fn message(message: Cow<'static, str>, level: rfd::MessageLevel) {
    let msg = rfd::AsyncMessageDialog::new()
        .set_level(level)
        .set_description(message.as_ref())
        .show();
    thread::spawn(move || futures::executor::block_on(msg));
}

/// A message that the user must acknowledge
pub struct MustAckMessage(mpsc::Receiver<()>);
impl MustAckMessage {
    pub fn was_ack(&self) -> bool {
        self.0.try_recv().is_ok()
    }
}

pub fn blocking_message(message: Cow<'static, str>, level: rfd::MessageLevel) -> MustAckMessage {
    let msg = rfd::AsyncMessageDialog::new()
        .set_level(level)
        .set_description(message.as_ref())
        .show();
    let (snd, rcv) = mpsc::channel();
    thread::spawn(move || {
        futures::executor::block_on(msg);
        snd.send(()).unwrap();
    });
    MustAckMessage(rcv)
}

pub struct PathInput(mpsc::Receiver<Option<PathBuf>>);
impl PathInput {
    pub fn get(&self) -> Option<Option<PathBuf>> {
        self.0.try_recv().ok()
    }
}

pub fn save(target_extension: &'static str) -> PathInput {
    let dialog = rfd::AsyncFileDialog::new().save_file();
    let (snd, rcv) = mpsc::channel();
    thread::spawn(move || {
        let save_op = async move {
            let file = dialog.await;
            if let Some(handle) = file {
                let mut path_buf: std::path::PathBuf = handle.path().clone().into();
                let extension = path_buf.extension().clone();
                if extension.is_none() {
                    path_buf.set_extension(target_extension);
                } else if extension.and_then(|e| e.to_str()) != Some(target_extension.into()) {
                    let extension = extension.unwrap();
                    let new_extension =
                        format!("{}.{}", extension.to_str().unwrap(), target_extension);
                    path_buf.set_extension(new_extension);
                }
                snd.send(Some(path_buf));
            } else {
                snd.send(None);
            }
        };
        futures::executor::block_on(save_op);
    });
    PathInput(rcv)
}

pub fn save_before_new() -> YesNoQuestion {
    yes_no_dialog("Do you want to save your design before loading an empty one?".into())
}

pub fn save_before_open() -> YesNoQuestion {
    yes_no_dialog("Do you want to save your design before loading a new one?".into())
}

pub fn save_before_quit() -> YesNoQuestion {
    yes_no_dialog("Do you want to save your design before exiting the app?".into())
}
