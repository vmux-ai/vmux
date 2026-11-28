use std::collections::{HashMap, HashSet};
use std::path::{Component as PathComponent, Path, PathBuf};

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::Order;
use vmux_ecs::event::{MdBlock, MdInline, NoteBlock};
use vmux_path::PathIdentity;

use crate::{MarkdownMetadata, RawNote, RawSnapshot, WikiLink};

pub struct KnowledgeGraphPlugin;

impl Plugin for KnowledgeGraphPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(materialize)
            .add_observer(rebuild)
            .add_observer(resolve_links)
            .add_observer(publish)
            .add_observer(retire);
    }
}

#[derive(Component, Default)]
pub struct KnowledgeIndex {
    root: PathBuf,
    loaded: bool,
    revision: u64,
}

impl KnowledgeIndex {
    pub fn loaded(&self) -> bool {
        self.loaded
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Component)]
pub struct LiveKnowledge;

#[derive(Component, Default)]
struct Lookup(HashMap<String, Vec<Entity>>);

#[derive(Component)]
pub(crate) struct KnowledgeNote {
    pub(crate) path: PathBuf,
    pub(crate) relative: String,
    aliases: Vec<String>,
    pub(crate) text: String,
    lowercase_text: String,
}

impl KnowledgeNote {
    fn capture(root: &Path, file: RawNote) -> (Self, Name) {
        let metadata = MarkdownMetadata::parse(&file.text);
        let title = if !metadata.title.is_empty() {
            metadata.title
        } else if let Some(title) = Self::first_heading(&file.text) {
            title
        } else {
            file.path
                .file_stem()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let relative = Self::relative(root, &file.path);
        let lowercase_text = file.text.to_lowercase();
        (
            Self {
                path: file.path,
                relative,
                aliases: metadata.aliases,
                text: file.text,
                lowercase_text,
            },
            Name::new(title),
        )
    }

    pub(crate) fn normalized(value: &str) -> String {
        let value = value.trim();
        let value = [".markdown", ".mdx", ".md"]
            .into_iter()
            .find_map(|extension| value.strip_suffix(extension))
            .unwrap_or(value);
        value
            .replace('\\', "/")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    pub(crate) fn relative(root: &Path, path: &Path) -> String {
        let mut relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
        relative.set_extension("");
        relative.to_string_lossy().replace('\\', "/")
    }

    fn first_heading(text: &str) -> Option<String> {
        for line in text.lines() {
            let Some(heading) = line.trim().strip_prefix("# ") else {
                continue;
            };
            let heading = heading.trim_end_matches('#').trim();
            if !heading.is_empty() {
                return Some(heading.to_string());
            }
        }
        None
    }

    fn keys(&self, title: &str) -> Vec<String> {
        let mut values = vec![self.relative.clone(), title.to_string()];
        if let Some(stem) = self.path.file_stem() {
            values.push(stem.to_string_lossy().into_owned());
        }
        values.extend(self.aliases.clone());
        let mut keys = Vec::new();
        for value in values {
            let key = Self::normalized(&value);
            if !key.is_empty() && !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    }

    fn contains_term(line: &str, term: &str) -> bool {
        let line = line.to_lowercase();
        let term = term.to_lowercase();
        let mut offset = 0;
        while let Some(found) = line[offset..].find(&term) {
            let start = offset + found;
            let end = start + term.len();
            let left = line[..start].chars().next_back();
            let right = line[end..].chars().next();
            if left.is_none_or(|character| !character.is_alphanumeric())
                && right.is_none_or(|character| !character.is_alphanumeric())
            {
                return true;
            }
            offset = end;
        }
        false
    }
}

#[derive(Component)]
struct Anchor {
    key: String,
    line: u32,
    block: bool,
}

impl Anchor {
    fn parse(text: &str) -> Vec<Self> {
        let mut anchors = Vec::new();
        let mut headings = HashSet::new();
        let mut blocks = HashSet::new();
        let mut fenced = false;
        for (line, raw) in text.lines().enumerate() {
            let value = raw.trim();
            if value.starts_with("```") || value.starts_with("~~~") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                continue;
            }
            let hashes = value.bytes().take_while(|byte| *byte == b'#').count();
            if (1..=6).contains(&hashes)
                && value
                    .as_bytes()
                    .get(hashes)
                    .is_some_and(u8::is_ascii_whitespace)
            {
                let heading = value[hashes..].trim().trim_end_matches('#').trim();
                let key = KnowledgeNote::normalized(heading);
                if !heading.is_empty() && headings.insert(key.clone()) {
                    anchors.push(Self {
                        key,
                        line: line as u32,
                        block: false,
                    });
                }
            }
            if let Some((prefix, block)) = value.rsplit_once('^')
                && prefix.chars().next_back().is_some_and(char::is_whitespace)
                && !block.is_empty()
                && block.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                })
            {
                let key = KnowledgeNote::normalized(block);
                if blocks.insert(key.clone()) {
                    anchors.push(Self {
                        key,
                        line: line as u32,
                        block: true,
                    });
                }
            }
        }
        anchors
    }
}

#[derive(Component)]
pub(crate) struct NoteLink {
    pub(crate) link: WikiLink,
    line: u32,
    preview: String,
}

impl NoteLink {
    fn parse(text: &str) -> Vec<Self> {
        let mut links = Vec::new();
        let mut rows = text.split_inclusive('\n');
        let mut row = rows.next().unwrap_or_default();
        let mut line = 0;
        let mut end = row.len();
        let mut preview: String = row.trim().chars().take(240).collect();
        for link in WikiLink::parse_all(text) {
            while link.start >= end && end < text.len() {
                row = rows.next().unwrap_or_default();
                end += row.len();
                line += 1;
                preview = row.trim().chars().take(240).collect();
            }
            links.push(Self {
                link,
                line,
                preview: preview.clone(),
            });
        }
        links
    }
}

#[derive(Component)]
#[relationship(relationship_target = IncomingLinks)]
pub(crate) struct LinkTarget(pub(crate) Entity);

#[derive(Component)]
#[relationship_target(relationship = LinkTarget)]
pub(crate) struct IncomingLinks(Vec<Entity>);

#[derive(EntityEvent)]
pub struct MaterializeKnowledge {
    pub entity: Entity,
    pub snapshot: RawSnapshot,
}

#[derive(EntityEvent)]
struct RebuildKnowledge {
    entity: Entity,
}

#[derive(EntityEvent)]
struct ResolveKnowledge {
    entity: Entity,
}

#[derive(EntityEvent)]
struct PublishKnowledge {
    entity: Entity,
}

#[derive(EntityEvent)]
pub struct RetireKnowledge {
    pub entity: Entity,
}

#[derive(EntityEvent)]
pub struct KnowledgeReady {
    pub entity: Entity,
}

fn materialize(
    mut trigger: On<MaterializeKnowledge>,
    mut indexes: Query<&mut KnowledgeIndex>,
    notes: Query<(Entity, &KnowledgeNote, &ChildOf)>,
    children: Query<&Children>,
    mut commands: Commands,
) {
    let request = trigger.event_mut();
    let Ok(mut index) = indexes.get_mut(request.entity) else {
        return;
    };
    index.root.clone_from(&request.snapshot.root);
    let mut files = std::mem::take(&mut request.snapshot.files);
    files.sort_by_key(|file| KnowledgeNote::relative(&request.snapshot.root, &file.path));
    for (entity, note, owner) in &notes {
        if owner.parent() == request.entity && !files.iter().any(|file| file.path == note.path) {
            commands.entity(entity).despawn();
        }
    }
    for (order, file) in files.into_iter().enumerate() {
        let mut existing = None;
        for (entity, note, owner) in &notes {
            if owner.parent() == request.entity && note.path == file.path {
                existing = Some(entity);
                break;
            }
        }
        let (note, title) = KnowledgeNote::capture(&request.snapshot.root, file);
        let anchors = Anchor::parse(&note.text);
        let links = NoteLink::parse(&note.text);
        let entity = match existing {
            Some(entity) => {
                if let Ok(children) = children.get(entity) {
                    for child in children.iter() {
                        commands.entity(child).despawn();
                    }
                }
                commands
                    .entity(entity)
                    .insert((note, title, Order(order as u32)));
                entity
            }
            None => commands
                .spawn((note, title, Order(order as u32), ChildOf(request.entity)))
                .id(),
        };
        for anchor in anchors {
            commands.spawn((anchor, ChildOf(entity)));
        }
        for (order, link) in links.into_iter().enumerate() {
            commands.spawn((link, Order(order as u32), ChildOf(entity)));
        }
    }
    commands.trigger(RebuildKnowledge {
        entity: request.entity,
    });
}

fn rebuild(
    trigger: On<RebuildKnowledge>,
    notes: Query<(Entity, &KnowledgeNote, &Name, &Order, &ChildOf)>,
    mut commands: Commands,
) {
    let mut ordered = Vec::new();
    for (entity, note, name, order, owner) in &notes {
        if owner.parent() == trigger.entity {
            ordered.push((order.0, entity, note, name));
        }
    }
    ordered.sort_by_key(|(order, _, _, _)| *order);
    let mut lookup = Lookup::default();
    for (_, entity, note, name) in ordered {
        for key in note.keys(name.as_str()) {
            lookup.0.entry(key).or_default().push(entity);
        }
    }
    commands.entity(trigger.entity).insert(lookup);
    commands.trigger(ResolveKnowledge {
        entity: trigger.entity,
    });
}

#[derive(SystemParam)]
pub(crate) struct KnowledgeResolver<'w, 's> {
    indexes: Query<'w, 's, (Entity, &'static KnowledgeIndex), With<LiveKnowledge>>,
    roots: Query<'w, 's, &'static KnowledgeIndex>,
    lookups: Query<'w, 's, &'static Lookup>,
    pub(crate) notes: Query<
        'w,
        's,
        (
            Entity,
            &'static KnowledgeNote,
            &'static Name,
            &'static Order,
            &'static ChildOf,
        ),
    >,
    anchors: Query<'w, 's, &'static Anchor>,
    children: Query<'w, 's, &'static Children>,
}

impl KnowledgeResolver<'_, '_> {
    fn note_entity(&self, owner: Entity, source: &Path, target: &str) -> Option<Entity> {
        let source = PathIdentity::resolve(source).into_path_buf();
        self.note_entity_canonical(owner, &source, target)
    }

    fn note_entity_canonical(&self, owner: Entity, source: &Path, target: &str) -> Option<Entity> {
        if target.trim().is_empty() {
            return self
                .notes
                .iter()
                .find(|(_, note, _, _, parent)| parent.parent() == owner && note.path == source)
                .map(|(entity, _, _, _, _)| entity);
        }
        let lookup = self.lookups.get(owner).ok()?;
        let candidates = lookup.0.get(&KnowledgeNote::normalized(target))?;
        let mut found = None;
        for entity in candidates {
            let Ok((_, note, _, _, parent)) = self.notes.get(*entity) else {
                continue;
            };
            if parent.parent() != owner {
                continue;
            }
            let score = (
                note.path.parent() != source.parent(),
                note.relative.matches('/').count(),
                note.relative.len(),
            );
            if found.is_none_or(|(_, previous)| score < previous) {
                found = Some((*entity, score));
            }
        }
        found.map(|(entity, _)| entity)
    }

    pub(crate) fn resolve(
        &self,
        owner: Entity,
        source: &Path,
        note: &str,
        anchor: Option<&str>,
    ) -> KnowledgeResolvedLink {
        let Some(entity) = self.note_entity(owner, source, note) else {
            let path = self
                .roots
                .get(owner)
                .ok()
                .and_then(|index| Self::safe_candidate(index.root(), source, note))
                .unwrap_or_default();
            return KnowledgeResolvedLink {
                path,
                title: note.trim().to_string(),
                line: None,
                exists: false,
            };
        };
        let (_, target, name, _, _) = self.notes.get(entity).unwrap();
        let mut line = None;
        if let Some(anchor) = anchor {
            let (key, block) = match anchor.strip_prefix('^') {
                Some(anchor) => (anchor, true),
                None => (anchor, false),
            };
            let key = KnowledgeNote::normalized(key);
            let children = self.children.get(entity).ok();
            for child in children.into_iter().flat_map(|children| children.iter()) {
                let Ok(value) = self.anchors.get(child) else {
                    continue;
                };
                if value.key == key && value.block == block {
                    line = Some(value.line);
                    break;
                }
            }
            if line.is_none() && block {
                let key = KnowledgeNote::normalized(anchor);
                for child in children.into_iter().flat_map(|children| children.iter()) {
                    let Ok(value) = self.anchors.get(child) else {
                        continue;
                    };
                    if value.key == key && !value.block {
                        line = Some(value.line);
                        break;
                    }
                }
            }
        }
        KnowledgeResolvedLink {
            path: target.path.clone(),
            title: name.as_str().to_string(),
            line,
            exists: true,
        }
    }

    fn safe_candidate(root: &Path, source: &Path, target: &str) -> Option<PathBuf> {
        let target = Path::new(target);
        if target.is_absolute()
            || target.components().any(|component| {
                !matches!(component, PathComponent::Normal(_) | PathComponent::CurDir)
            })
        {
            return None;
        }
        let base = if target.components().count() > 1 {
            root.to_path_buf()
        } else {
            PathIdentity::resolve(source)
                .into_path_buf()
                .parent()
                .unwrap_or(root)
                .to_path_buf()
        };
        let mut path = base.join(target);
        if path.extension().is_none() {
            path.set_extension("md");
        }
        path.starts_with(root).then_some(path)
    }
}

fn resolve_links(
    trigger: On<ResolveKnowledge>,
    resolver: KnowledgeResolver,
    links: Query<(Entity, &NoteLink, &ChildOf)>,
    mut commands: Commands,
) {
    for (entity, link, source) in &links {
        let Ok((_, note, _, _, owner)) = resolver.notes.get(source.parent()) else {
            continue;
        };
        if owner.parent() == trigger.entity {
            match resolver.note_entity_canonical(trigger.entity, &note.path, &link.link.note) {
                Some(target) => {
                    commands.entity(entity).insert(LinkTarget(target));
                }
                None => {
                    commands.entity(entity).remove::<LinkTarget>();
                }
            }
        }
    }
    commands.trigger(PublishKnowledge {
        entity: trigger.entity,
    });
}

fn publish(
    trigger: On<PublishKnowledge>,
    mut indexes: Query<&mut KnowledgeIndex>,
    mut commands: Commands,
) {
    if let Ok(mut index) = indexes.get_mut(trigger.entity) {
        index.loaded = true;
        index.revision = index.revision.wrapping_add(1);
    }
    commands.trigger(KnowledgeReady {
        entity: trigger.entity,
    });
}

#[derive(SystemParam)]
pub struct KnowledgeGraph<'w, 's> {
    pub(crate) resolver: KnowledgeResolver<'w, 's>,
    pub(crate) links: Query<
        'w,
        's,
        (
            &'static NoteLink,
            &'static ChildOf,
            &'static Order,
            Option<&'static LinkTarget>,
        ),
    >,
    pub(crate) children: Query<'w, 's, &'static Children>,
    incoming: Query<'w, 's, &'static IncomingLinks>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeResolvedLink {
    pub path: PathBuf,
    pub title: String,
    pub line: Option<u32>,
    pub exists: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeSearchHit {
    pub path: PathBuf,
    pub title: String,
    pub line: u32,
    pub preview: String,
    pub score: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeBacklink {
    pub path: PathBuf,
    pub title: String,
    pub line: u32,
    pub preview: String,
    pub unlinked: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeBrokenLink {
    pub target: String,
    pub line: u32,
}

impl KnowledgeGraph<'_, '_> {
    pub fn root(&self, owner: Entity) -> Option<&Path> {
        self.resolver
            .roots
            .get(owner)
            .ok()
            .map(KnowledgeIndex::root)
    }

    pub fn index(&self) -> Option<(Entity, &KnowledgeIndex)> {
        self.resolver.indexes.single().ok()
    }

    pub fn resolve(
        &self,
        owner: Entity,
        source: &Path,
        note: &str,
        anchor: Option<&str>,
    ) -> KnowledgeResolvedLink {
        self.resolver.resolve(owner, source, note, anchor)
    }

    pub fn note_by_query(&self, owner: Entity, query: &str) -> Option<(PathBuf, String, String)> {
        let source = self.resolver.roots.get(owner).ok()?.root().join("index.md");
        let entity = self.resolver.note_entity(owner, &source, query)?;
        let (_, note, name, _, _) = self.resolver.notes.get(entity).ok()?;
        Some((
            note.path.clone(),
            name.as_str().to_string(),
            note.text.clone(),
        ))
    }

    pub fn backlinks(&self, owner: Entity, path: &Path) -> Vec<KnowledgeBacklink> {
        let path = PathIdentity::resolve(path).into_path_buf();
        let Some(target) = self.resolver.note_entity(owner, &path, "") else {
            return Vec::new();
        };
        let mut results = Vec::new();
        let Ok(incoming) = self.incoming.get(target) else {
            return Vec::new();
        };
        for entity in incoming.iter() {
            let Ok((link, parent, order, _)) = self.links.get(entity) else {
                continue;
            };
            let Ok((_, note, name, source_order, note_owner)) =
                self.resolver.notes.get(parent.parent())
            else {
                continue;
            };
            if note_owner.parent() != owner {
                continue;
            }
            results.push((
                source_order.0,
                order.0,
                KnowledgeBacklink {
                    path: note.path.clone(),
                    title: name.as_str().to_string(),
                    line: link.line,
                    preview: link.preview.clone(),
                    unlinked: false,
                },
            ));
        }
        results.sort_by(|left, right| {
            left.2
                .title
                .to_lowercase()
                .cmp(&right.2.title.to_lowercase())
                .then(left.2.line.cmp(&right.2.line))
                .then(left.0.cmp(&right.0))
                .then(left.1.cmp(&right.1))
        });
        results.into_iter().map(|(_, _, result)| result).collect()
    }

    pub fn broken_links(&self, owner: Entity, path: &Path) -> Vec<KnowledgeBrokenLink> {
        let path = PathIdentity::resolve(path).into_path_buf();
        let Some(source) = self.resolver.note_entity(owner, &path, "") else {
            return Vec::new();
        };
        let mut results = Vec::new();
        let Ok(children) = self.children.get(source) else {
            return Vec::new();
        };
        for entity in children.iter() {
            let Ok((link, _, order, target)) = self.links.get(entity) else {
                continue;
            };
            if target.is_none() {
                results.push((
                    order.0,
                    KnowledgeBrokenLink {
                        target: link.link.note.clone(),
                        line: link.line,
                    },
                ));
            }
        }
        results.sort_by_key(|(order, _)| *order);
        results.into_iter().map(|(_, result)| result).collect()
    }

    pub fn unlinked_mentions(
        &self,
        owner: Entity,
        path: &Path,
        limit: usize,
    ) -> Vec<KnowledgeBacklink> {
        let linked = self
            .backlinks(owner, path)
            .into_iter()
            .map(|reference| reference.path)
            .collect::<HashSet<_>>();
        self.unlinked_mentions_excluding(owner, path, limit, &linked)
    }

    pub fn unlinked_mentions_excluding(
        &self,
        owner: Entity,
        path: &Path,
        limit: usize,
        linked: &HashSet<PathBuf>,
    ) -> Vec<KnowledgeBacklink> {
        let path = PathIdentity::resolve(path).into_path_buf();
        let Some(target) = self.resolver.note_entity(owner, &path, "") else {
            return Vec::new();
        };
        let (_, target, name, _, _) = self.resolver.notes.get(target).unwrap();
        let mut terms = vec![name.as_str().to_string()];
        terms.extend(target.aliases.clone());
        terms.retain(|term| term.chars().count() >= 3);
        let mut ordered = Vec::new();
        for (_, note, name, order, parent) in &self.resolver.notes {
            if parent.parent() == owner {
                ordered.push((order.0, note, name));
            }
        }
        ordered.sort_by_key(|(order, _, _)| *order);
        let mut results = Vec::new();
        for (_, note, name) in ordered {
            if note.path == path || linked.contains(&note.path) {
                continue;
            }
            for (line, value) in note.text.lines().enumerate() {
                if terms
                    .iter()
                    .any(|term| KnowledgeNote::contains_term(value, term))
                {
                    results.push(KnowledgeBacklink {
                        path: note.path.clone(),
                        title: name.as_str().to_string(),
                        line: line as u32,
                        preview: value.trim().chars().take(240).collect(),
                        unlinked: true,
                    });
                    break;
                }
            }
            if results.len() >= limit {
                break;
            }
        }
        results
    }

    pub fn search(&self, owner: Entity, query: &str, limit: usize) -> Vec<KnowledgeSearchHit> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        let terms = query.split_whitespace().collect::<Vec<_>>();
        let mut hits = Vec::new();
        for (_, note, name, _, parent) in &self.resolver.notes {
            if parent.parent() != owner {
                continue;
            }
            let title = name.as_str().to_lowercase();
            let relative = note.relative.to_lowercase();
            let aliases = note
                .aliases
                .iter()
                .map(|alias| alias.to_lowercase())
                .collect::<Vec<_>>();
            if !terms.iter().all(|term| {
                title.contains(term)
                    || relative.contains(term)
                    || aliases.iter().any(|alias| alias.contains(term))
                    || note.lowercase_text.contains(term)
            }) {
                continue;
            }
            let mut score = if title == query {
                1000
            } else if title.starts_with(&query) {
                750
            } else if title.contains(&query) {
                600
            } else if aliases.iter().any(|alias| alias == &query) {
                550
            } else if relative.contains(&query) {
                400
            } else {
                100
            };
            let mut line = 0;
            let mut preview = name.as_str().to_string();
            for (index, value) in note.text.lines().enumerate() {
                let lower = value.to_lowercase();
                if terms.iter().all(|term| lower.contains(term)) {
                    line = index as u32;
                    preview = value.trim().chars().take(240).collect();
                    score += 100;
                    break;
                }
            }
            hits.push(KnowledgeSearchHit {
                path: note.path.clone(),
                title: name.as_str().to_string(),
                line,
                preview,
                score,
            });
        }
        hits.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
                .then_with(|| left.path.cmp(&right.path))
        });
        hits.truncate(limit);
        hits
    }

    pub fn completions(&self, owner: Entity, prefix: &str, limit: usize) -> Vec<(String, String)> {
        let prefix = KnowledgeNote::normalized(prefix);
        let mut values = Vec::new();
        for (_, note, name, order, parent) in &self.resolver.notes {
            if parent.parent() != owner {
                continue;
            }
            let title = KnowledgeNote::normalized(name.as_str());
            let relative = KnowledgeNote::normalized(&note.relative);
            if prefix.is_empty()
                || title.contains(&prefix)
                || relative.contains(&prefix)
                || note
                    .aliases
                    .iter()
                    .any(|alias| KnowledgeNote::normalized(alias).contains(&prefix))
            {
                values.push((
                    !title.starts_with(&prefix),
                    name.as_str().to_string(),
                    note.relative.clone(),
                    order.0,
                ));
            }
        }
        values.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.to_lowercase().cmp(&right.1.to_lowercase()))
                .then(left.3.cmp(&right.3))
        });
        values
            .into_iter()
            .take(limit)
            .map(|(_, title, relative, _)| (title, relative))
            .collect()
    }

    pub fn resolve_blocks(&self, owner: Entity, source: &Path, blocks: &mut [NoteBlock]) {
        for block in blocks {
            self.resolve_block(owner, source, &mut block.block);
        }
    }
}

impl KnowledgeGraph<'_, '_> {
    fn resolve_inlines(&self, owner: Entity, source: &Path, inlines: &mut [MdInline]) {
        for inline in inlines {
            match inline {
                MdInline::Strong(children)
                | MdInline::Emph(children)
                | MdInline::Strike(children)
                | MdInline::Link {
                    inlines: children, ..
                } => self.resolve_inlines(owner, source, children),
                MdInline::WikiLink {
                    target,
                    label,
                    path,
                    line,
                    exists,
                    ..
                } => {
                    let (note, anchor) = target
                        .split_once('#')
                        .map(|(note, anchor)| (note, Some(anchor)))
                        .unwrap_or((target.as_str(), None));
                    let resolved = self.resolve(owner, source, note, anchor);
                    *path = resolved.path.to_string_lossy().into_owned();
                    *line = resolved.line;
                    *exists = resolved.exists;
                    if label.is_empty() {
                        *label = resolved.title;
                    }
                }
                MdInline::Text(_)
                | MdInline::Code(_)
                | MdInline::Image { .. }
                | MdInline::SoftBreak
                | MdInline::HardBreak => {}
            }
        }
    }

    fn resolve_block(&self, owner: Entity, source: &Path, block: &mut MdBlock) {
        match block {
            MdBlock::Heading { inlines, .. } | MdBlock::Paragraph { inlines } => {
                self.resolve_inlines(owner, source, inlines);
            }
            MdBlock::List { items, .. } => {
                for item in items {
                    for block in &mut item.blocks {
                        self.resolve_block(owner, source, block);
                    }
                }
            }
            MdBlock::BlockQuote { blocks } => {
                for block in blocks {
                    self.resolve_block(owner, source, block);
                }
            }
            MdBlock::Table { header, rows, .. } => {
                for cell in header {
                    self.resolve_inlines(owner, source, cell);
                }
                for row in rows {
                    for cell in row {
                        self.resolve_inlines(owner, source, cell);
                    }
                }
            }
            MdBlock::CodeBlock { .. } | MdBlock::ThematicBreak | MdBlock::Html { .. } => {}
        }
    }
}

fn retire(
    trigger: On<RetireKnowledge>,
    notes: Query<(Entity, &ChildOf), With<KnowledgeNote>>,
    mut commands: Commands,
) {
    for (entity, parent) in &notes {
        if parent.parent() == trigger.entity {
            commands.entity(entity).despawn();
        }
    }
    commands
        .entity(trigger.entity)
        .remove::<(KnowledgeIndex, Lookup)>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;

    struct GraphFixture {
        directory: tempfile::TempDir,
        app: App,
        owner: Entity,
    }

    impl GraphFixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(directory.path().join("projects")).unwrap();
            std::fs::write(directory.path().join("projects/alpha.md"), "---\ntitle: Alpha Project\naliases: [Alpha, A Project]\n---\n\n# Overview\n\nAlpha body ^alpha-block\n").unwrap();
            std::fs::write(
                directory.path().join("projects/source.md"),
                "# Source\n\nSee [[Alpha#Overview|the overview]] and [[Missing Note]].\n",
            )
            .unwrap();
            std::fs::write(
                directory.path().join("projects/mention.md"),
                "Alpha Project appears without a link.\n",
            )
            .unwrap();
            let mut app = App::new();
            app.add_plugins(KnowledgeGraphPlugin);
            let owner = app
                .world_mut()
                .spawn((KnowledgeIndex::default(), LiveKnowledge))
                .id();
            app.world_mut().flush();
            let mut fixture = Self {
                directory,
                app,
                owner,
            };
            fixture.refresh();
            fixture
        }

        fn refresh(&mut self) {
            let snapshot = RawSnapshot::capture(self.directory.path()).unwrap();
            self.app.world_mut().trigger(MaterializeKnowledge {
                entity: self.owner,
                snapshot,
            });
            self.app.world_mut().flush();
        }

        fn note(&mut self, path: &str) -> Entity {
            let path = self.directory.path().join(path);
            let path = PathIdentity::resolve(path).into_path_buf();
            self.app
                .world_mut()
                .query::<(Entity, &KnowledgeNote, &ChildOf)>()
                .iter(self.app.world())
                .find(|(_, note, owner)| owner.parent() == self.owner && note.path == path)
                .map(|(entity, _, _)| entity)
                .unwrap()
        }
    }

    #[test]
    fn resolves_titles_aliases_headings_and_safe_candidates_through_canonical_entities() {
        let mut fixture = GraphFixture::new();
        let owner = fixture.owner;
        let source = fixture.directory.path().join("projects/source.md");
        let expected =
            PathIdentity::resolve(fixture.directory.path().join("projects/Missing Note.md"))
                .into_path_buf();
        let (resolved, missing, block) = fixture
            .app
            .world_mut()
            .run_system_once(move |graph: KnowledgeGraph| {
                (
                    graph.resolve(owner, &source, "Alpha", Some("Overview")),
                    graph.resolve(owner, &source, "Missing Note", None),
                    graph.resolve(owner, &source, "A Project", Some("^alpha-block")),
                )
            })
            .unwrap();
        assert!(resolved.exists);
        assert_eq!(resolved.title, "Alpha Project");
        assert_eq!(resolved.line, Some(5));
        assert_eq!(block.line, Some(7));
        assert!(!missing.exists);
        assert_eq!(missing.path, expected);
    }

    #[test]
    fn backlinks_broken_links_mentions_search_and_completions_use_the_same_note_graph() {
        let mut fixture = GraphFixture::new();
        let owner = fixture.owner;
        let alpha = fixture.directory.path().join("projects/alpha.md");
        let source = fixture.directory.path().join("projects/source.md");
        let (backlinks, broken, mentions, search, completions) = fixture
            .app
            .world_mut()
            .run_system_once(move |graph: KnowledgeGraph| {
                (
                    graph.backlinks(owner, &alpha),
                    graph.broken_links(owner, &source),
                    graph.unlinked_mentions(owner, &alpha, 10),
                    graph.search(owner, "overview", 10),
                    graph.completions(owner, "alpha", 10),
                )
            })
            .unwrap();
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].title, "Source");
        assert_eq!(broken[0].target, "Missing Note");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].title, "mention");
        assert_eq!(search[0].title, "Alpha Project");
        assert_eq!(
            completions,
            [("Alpha Project".to_string(), "projects/alpha".to_string())]
        );
    }

    #[test]
    fn refresh_preserves_note_identity_and_retires_removed_references() {
        let mut fixture = GraphFixture::new();
        let alpha = fixture.note("projects/alpha.md");
        let source = fixture.note("projects/source.md");
        std::fs::remove_file(fixture.directory.path().join("projects/source.md")).unwrap();
        std::fs::write(
            fixture.directory.path().join("projects/alpha.md"),
            "# Replaced\n\nNew body\n",
        )
        .unwrap();
        fixture.refresh();
        assert_eq!(fixture.note("projects/alpha.md"), alpha);
        assert!(fixture.app.world().get_entity(source).is_err());
        let owner = fixture.owner;
        let path = fixture.directory.path().join("projects/alpha.md");
        let (resolved, backlinks) = fixture
            .app
            .world_mut()
            .run_system_once(move |graph: KnowledgeGraph| {
                (
                    graph.resolve(owner, &path, "Replaced", None),
                    graph.backlinks(owner, &path),
                )
            })
            .unwrap();
        assert_eq!(resolved.title, "Replaced");
        assert!(backlinks.is_empty());
    }

    #[test]
    fn anchor_parsing_ignores_fences_and_requires_terminal_whitespace_delimited_blocks() {
        let mut fixture = GraphFixture::new();
        std::fs::write(
            fixture.directory.path().join("projects/alpha.md"),
            "# Alpha\nvalid ^block-id\n2^10\nx ^ y\n`code ^bad`\n```\n# Hidden\n````\n# Visible\n",
        )
        .unwrap();
        fixture.refresh();
        let owner = fixture.owner;
        let source = fixture.directory.path().join("projects/source.md");
        let (valid, invalid, fenced, visible) = fixture
            .app
            .world_mut()
            .run_system_once(move |graph: KnowledgeGraph| {
                (
                    graph.resolve(owner, &source, "Alpha", Some("^block-id")),
                    graph.resolve(owner, &source, "Alpha", Some("^bad")),
                    graph.resolve(owner, &source, "Alpha", Some("Hidden")),
                    graph.resolve(owner, &source, "Alpha", Some("Visible")),
                )
            })
            .unwrap();
        assert_eq!(valid.line, Some(1));
        assert!(invalid.line.is_none());
        assert!(fenced.line.is_none());
        assert_eq!(visible.line, Some(8));
    }

    #[test]
    fn scoped_rename_catalog_does_not_replace_the_live_catalog_and_retirement_is_scoped() {
        let mut fixture = GraphFixture::new();
        let live_note = fixture.note("projects/alpha.md");
        let scope = fixture
            .app
            .world_mut()
            .spawn(KnowledgeIndex::default())
            .id();
        let root = fixture.directory.path().canonicalize().unwrap();
        let snapshot = RawSnapshot {
            root: root.clone(),
            files: vec![RawNote {
                path: root.join("projects/alpha.md"),
                text: "# Scoped title\n".to_string(),
            }],
        };
        fixture.app.world_mut().trigger(MaterializeKnowledge {
            entity: scope,
            snapshot,
        });
        fixture.app.world_mut().flush();
        let live = fixture.owner;
        let source = root.join("projects/source.md");
        let (public, scoped) = fixture
            .app
            .world_mut()
            .run_system_once(move |graph: KnowledgeGraph| {
                (
                    graph.resolve(live, &source, "Alpha", None),
                    graph.resolve(scope, &source, "Scoped title", None),
                )
            })
            .unwrap();
        assert_eq!(public.title, "Alpha Project");
        assert_eq!(scoped.title, "Scoped title");
        fixture
            .app
            .world_mut()
            .trigger(RetireKnowledge { entity: scope });
        fixture.app.world_mut().flush();
        assert!(fixture.app.world().get::<KnowledgeIndex>(scope).is_none());
        assert_eq!(fixture.note("projects/alpha.md"), live_note);
        assert!(
            fixture
                .app
                .world()
                .get::<KnowledgeIndex>(live)
                .unwrap()
                .loaded()
        );
    }
}
