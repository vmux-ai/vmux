use crate::host::snapshot::CommandBarWorkDirectory;
use vmux_api::command_bar::{CommandBarRecentFile, CommandBarWorkDir};
use vmux_ecs::{LastVisitedAt, PageMetadata, VisitCount};

#[derive(Default)]
pub(super) struct WorkDirectories(Vec<(String, i64)>);

impl WorkDirectories {
    pub(super) fn add(&mut self, path: &str, activated_at: i64) {
        if path.is_empty() {
            return;
        }
        if let Some(existing) = self.0.iter_mut().find(|(candidate, _)| candidate == path) {
            existing.1 = existing.1.max(activated_at);
            return;
        }
        self.0.push((path.to_string(), activated_at));
    }

    pub(super) fn paths(mut self) -> Vec<String> {
        self.0
            .sort_by_key(|(_, activated_at)| std::cmp::Reverse(*activated_at));
        self.0.into_iter().map(|(path, _)| path).collect()
    }
}

impl CommandBarWorkDirectory {
    pub(super) fn entries(&self) -> Vec<CommandBarWorkDir> {
        let Ok(read) = std::fs::read_dir(&self.0) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().is_ok_and(|file_type| file_type.is_dir());
            let path = entry.path().to_string_lossy().to_string();
            rows.push((name, is_dir, path));
        }
        rows.sort_by(|a, b| {
            let a_hidden = a.0.starts_with('.');
            let b_hidden = b.0.starts_with('.');
            b.1.cmp(&a.1)
                .then(a_hidden.cmp(&b_hidden))
                .then(a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        });
        rows.into_iter()
            .map(|(_, is_dir, path)| CommandBarWorkDir { path, is_dir })
            .collect()
    }
}

pub(super) struct RecentFile {
    pub(super) score: f32,
    pub(super) value: CommandBarRecentFile,
}

impl RecentFile {
    pub(super) fn from_page(
        metadata: &PageMetadata,
        visit_count: VisitCount,
        last_visited_at: LastVisitedAt,
        now: i64,
    ) -> Option<Self> {
        let path = metadata.url.strip_prefix("file://")?;
        if std::path::Path::new(path).is_dir() {
            return None;
        }
        let age_hours = ((now - last_visited_at.0).max(0) as f32) / 3_600_000.0;
        let decay = 1.0 / (1.0 + age_hours / 24.0);
        Some(Self {
            score: (visit_count.0 as f32) * decay,
            value: CommandBarRecentFile {
                url: metadata.url.clone(),
                title: metadata.title.clone(),
            },
        })
    }
}
