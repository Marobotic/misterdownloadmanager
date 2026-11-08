//! File list for any torrent, including ones no longer held in the torrent session.
use std::path::PathBuf;

use crate::{
    dialogs::{self, Outcome},
    model::{Item, Status},
    winui::{self, ButtonKind, LIGHT_BG, r},
};

pub struct TorrentFiles {
    pub id: u64,
    rows: Vec<(usize, mdm_torrent::FileEntry, bool, u64)>,
    base: PathBuf,
    complete: bool,
    pending: Option<(Vec<usize>, bool, u64)>,
}

impl TorrentFiles {
    pub fn new(item: &Item, host: &crate::torrents::Host) -> Option<Self> {
        let spec = item.torrent.as_ref()?;
        let session = host.get().and_then(|t| {
            t.files(&spec.hash)
                .map(|(files, chosen, _)| (files, chosen))
        });
        let (files, chosen) = if let Some((files, chosen)) = session {
            (files, chosen)
        } else {
            let bytes = std::fs::read(crate::torrents::torrent_file(&spec.hash)?).ok()?;
            (
                mdm_torrent::files_from_bytes(&bytes).ok()?,
                spec.files.clone(),
            )
        };
        let root = if files.iter().filter(|f| !f.padding).count() > 1 {
            spec.folder.join(&item.file_name)
        } else {
            spec.folder.clone()
        };
        let complete = item.status == Status::Complete;
        let rows = files
            .into_iter()
            .enumerate()
            .filter(|(_, f)| !f.padding)
            .map(|(i, f)| {
                let chosen = chosen.as_ref().is_none_or(|v| v.contains(&i));
                let done = if complete {
                    f.size
                } else {
                    std::fs::metadata(root.join(&f.path))
                        .map(|m| m.len().min(f.size))
                        .unwrap_or(0)
                };
                (i, f, chosen, done)
            })
            .collect();
        Some(Self {
            id: item.id,
            rows,
            base: root,
            complete,
            pending: None,
        })
    }

    pub fn take_selection(&mut self) -> Option<(Vec<usize>, bool, u64)> {
        self.pending.take()
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<Option<(Vec<usize>, bool, u64)>> {
        let mut changed = None;
        let outcome = dialogs::window(
            ctx,
            &format!("torrent-files-{}", self.id),
            "Torrent files",
            [548.0, 430.0],
            false,
            LIGHT_BG(),
            |ui, o| {
                let p = ui.painter().clone();
                let mut all = self.rows.iter().all(|r| r.2);
                let response = winui::checkbox(
                    ui,
                    o + egui::vec2(20.0, 249.0),
                    &mut all,
                    crate::i18n::t("Select all"),
                    true,
                );
                if response.changed() {
                    for row in &mut self.rows {
                        row.2 = all;
                    }
                    if !all && let Some(last) = self.rows.last_mut() {
                        last.2 = true;
                    }
                }
                let table_rows: Vec<Vec<String>> = self
                    .rows
                    .iter()
                    .map(|(_, f, on, done)| {
                        vec![
                            f.path.display().to_string(),
                            crate::model::size(f.size),
                            if !on {
                                crate::i18n::t("Skipped").into()
                            } else {
                                crate::i18n::percent(
                                    if f.size == 0 {
                                        1.0
                                    } else {
                                        *done as f64 / f.size as f64
                                    },
                                    1,
                                )
                            },
                        ]
                    })
                    .collect();
                let ticks: Vec<bool> = self.rows.iter().map(|r| r.2).collect();
                if let Some(row) =
                    crate::progress::files_table(ui, &p, o, "torrent-files", &table_rows, &ticks)
                {
                    self.rows[row].2 = !self.rows[row].2;
                    if !self.rows.iter().any(|row| row.2) {
                        self.rows[row].2 = true;
                    }
                }
                let double = ui.input(|i| {
                    i.pointer
                        .button_double_clicked(egui::PointerButton::Primary)
                        .then(|| i.pointer.interact_pos())
                        .flatten()
                });
                if self.complete
                    && let Some(pos) = double
                {
                    let viewport = ui.ctx().viewport_id();
                    let table_id = egui::Id::unique(("torrent-table", "torrent-files", viewport));
                    let first = ui
                        .ctx()
                        .data(|d| d.get_temp::<usize>(table_id))
                        .unwrap_or(0);
                    let row = ((pos.y - (o.y + 299.0)) / 17.0).floor() as isize + first as isize;
                    if pos.y >= o.y + 299.0
                        && pos.y < o.y + 399.0
                        && row >= 0
                        && let Some((_, file, _, _)) = self.rows.get(row as usize)
                    {
                        dialogs::open_file(&self.base.join(&file.path));
                    }
                }
                let (open_at, folder_at, close_at) = (18.0, 380.0, 458.0);
                let open = winui::button(
                    ui,
                    r(o, open_at, 407.0, 75.0, 22.0),
                    "Open",
                    if self.complete {
                        ButtonKind::Normal
                    } else {
                        ButtonKind::Disabled
                    },
                    false,
                    None,
                )
                .clicked();
                if open
                    && self.complete
                    && let Some((_, f, _, _)) = self.rows.first()
                {
                    dialogs::open_file(&self.base.join(&f.path));
                }
                if winui::button(
                    ui,
                    r(o, folder_at, 407.0, 75.0, 22.0),
                    "Open folder",
                    ButtonKind::Normal,
                    false,
                    None,
                )
                .clicked()
                {
                    dialogs::show_in_folder(&self.base);
                }
                let close = winui::button(
                    ui,
                    r(o, close_at, 407.0, 72.0, 22.0),
                    "Close",
                    ButtonKind::Normal,
                    false,
                    None,
                )
                .clicked();
                if winui::escape_pressed(ui) || close {
                    Outcome::Closed
                } else if response.changed()
                    || !ticks.iter().zip(&self.rows).all(|(a, b)| *a == b.2)
                {
                    let chosen = self.rows.iter().filter(|r| r.2);
                    let files = chosen.clone().map(|r| r.0).collect::<Vec<_>>();
                    let all = files.len() == self.rows.len();
                    let size = chosen.map(|r| r.1.size).sum();
                    changed = Some((files, all, size));
                    Outcome::Open
                } else {
                    Outcome::Open
                }
            },
        );
        match outcome {
            Outcome::Open => {
                self.pending = changed;
                Outcome::Open
            }
            Outcome::Closed => Outcome::Closed,
            Outcome::Done(()) => Outcome::Done(None),
        }
    }
}
