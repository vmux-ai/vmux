use super::{Arguments, Completed, Context, Io, Root, Sequence, Stage};
use crate::Settings;
use crate::ci_io::fs::{Bytes, Location, Print, ReadFile, Written};
use crate::ci_io::process::Captured;
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use serde_json::Value;
use std::ffi::OsStr;
use vmux_ecs::cli::CliInvocation;
#[derive(bevy_ecs :: system :: SystemParam)]
struct Results<'w, 's> {
    event_slot_bytes: super::Reads<'w, 's, Bytes, EventSlot>,
    ancestor_slot_captured: super::Reads<'w, 's, Captured, AncestorSlot>,
    descendant_slot_captured: super::Reads<'w, 's, Captured, DescendantSlot>,
    print_slot_written: super::Reads<'w, 's, Written, PrintSlot>,
    checkout_print_slot_written: super::Reads<'w, 's, Written, CheckoutPrintSlot>,
    parent_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, ParentSlot>,
    tip_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, TipSlot>,
    fetch_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, FetchSlot>,
    merge_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, MergeSlot>,
    tree_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, TreeSlot>,
    base_version_slot_captured:
        super::Reads<'w, 's, crate::ci_io::process::Captured, BaseVersionSlot>,
    head_version_slot_captured:
        super::Reads<'w, 's, crate::ci_io::process::Captured, HeadVersionSlot>,
    changed_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, ChangedSlot>,
    before_mode_slot_captured:
        super::Reads<'w, 's, crate::ci_io::process::Captured, BeforeModeSlot>,
    after_mode_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, AfterModeSlot>,
    before_workflow_slot_captured:
        super::Reads<'w, 's, crate::ci_io::process::Captured, BeforeWorkflowSlot>,
    after_workflow_slot_captured:
        super::Reads<'w, 's, crate::ci_io::process::Captured, AfterWorkflowSlot>,
    checkout_tree_slot_captured:
        super::Reads<'w, 's, crate::ci_io::process::Captured, CheckoutTreeSlot>,
    date_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, DateSlot>,
    commit_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, CommitSlot>,
    checkout_slot_captured: super::Reads<'w, 's, crate::ci_io::process::Captured, CheckoutSlot>,
}
pub(super) struct CiPlanPlugin;
impl bevy_app::Plugin for CiPlanPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                start,
                event,
                parents,
                tip,
                fetched,
                ancestry,
                merged,
                presentation_start,
                presentation_compare,
                emit,
                checkout_start,
                checkout_tree,
                checkout_date,
                checkout_commit,
                checkout_finish,
            )
                .chain()
                .in_set(Stage::Step),
        );
    }
}
#[derive(Component)]
#[require(Phase)]
pub(super) struct Plan {
    repository: String,
    checkout: String,
    head: String,
    base: String,
    supplied_base: String,
    base_branch: String,
    tip: String,
    tree: String,
    release: bool,
    presentation_only: bool,
}
impl Plan {
    pub(super) fn equivalent(before: &str, after: &str) -> bool {
        let before = match super::workflow::Syntax::parse(before)
            .and_then(|node| Self::normalized(&node, &[]))
        {
            Ok(value) => value,
            Err(_) => return false,
        };
        let after = match super::workflow::Syntax::parse(after)
            .and_then(|node| Self::normalized(&node, &[]))
        {
            Ok(value) => value,
            Err(_) => return false,
        };
        before == after
    }
    fn normalized(
        node: &super::workflow::Node,
        path: &[String],
    ) -> Result<super::workflow::Node, String> {
        match node {
            super::workflow::Node::Scalar { .. } => Ok(node.clone()),
            super::workflow::Node::Sequence(children) => {
                let mut result = Vec::new();
                for (index, child) in children.iter().enumerate() {
                    let mut path = path.to_vec();
                    path.push(index.to_string());
                    result.push(Self::normalized(child, &path)?);
                }
                Ok(super::workflow::Node::Sequence(result))
            }
            super::workflow::Node::Mapping(children) => {
                if children.len() % 2 != 0 {
                    return Err("Incomplete YAML mapping".into());
                }
                let mut names = std::collections::BTreeSet::new();
                let mut result = Vec::new();
                for pair in children.chunks_exact(2) {
                    let name = pair[0].name()?;
                    if !names.insert(name) {
                        return Err("Duplicate YAML mapping key".into());
                    }
                    if name == "name"
                        && path.len() == 2
                        && path[0] == "jobs"
                        && matches!(path[1].as_str(), "plan" | "changes" | "ci")
                    {
                        match &pair[1] {
                            super::workflow::Node::Scalar { value, tag, .. }
                                if tag == "None" && !value.contains("${{") =>
                            {
                                continue;
                            }
                            _ => return Err("Orchestration names must be literal scalars".into()),
                        }
                    }
                    let mut child_path = path.to_vec();
                    child_path.push(name.into());
                    result.push(pair[0].clone());
                    result.push(Self::normalized(&pair[1], &child_path)?);
                }
                Ok(super::workflow::Node::Mapping(result))
            }
        }
    }
    fn sha(value: &str) -> Result<(), String> {
        if value.len() != 40
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("Invalid CI Git object identifier".into());
        }
        Ok(())
    }
    fn encode(value: &str) -> String {
        let mut output = String::new();
        for byte in value.bytes() {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                output.push(byte as char);
            } else {
                output.push_str(&format!("%{byte:02X}"));
            }
        }
        output
    }
    fn pull(&mut self, payload: &Value) -> Result<(), String> {
        let pull = payload.get("pull_request").ok_or("Missing PR event")?;
        let head = pull
            .pointer("/head/sha")
            .and_then(Value::as_str)
            .ok_or("Missing PR head")?;
        let base = pull
            .pointer("/base/sha")
            .and_then(Value::as_str)
            .ok_or("Missing PR base")?;
        let branch = pull
            .pointer("/base/ref")
            .and_then(Value::as_str)
            .ok_or("Missing PR target")?;
        Self::sha(head)?;
        Self::sha(base)?;
        if branch.is_empty() || branch.bytes().any(|byte| byte.is_ascii_control()) {
            return Err("Invalid PR target".into());
        }
        self.head = head.into();
        self.supplied_base = base.into();
        self.base_branch = branch.into();
        Ok(())
    }
    fn version(source: &str) -> Result<String, String> {
        let manifest: toml::Value = toml::from_str(source).map_err(|error| error.to_string())?;
        let value = manifest
            .get("workspace")
            .and_then(|value| value.get("package"))
            .and_then(|value| value.get("version"))
            .and_then(toml::Value::as_str)
            .ok_or("Missing CI workspace version")?;
        Ok(value.into())
    }
    fn probe(
        &self,
        entity: Entity,
        context: &Context,
        settings: &Settings,
        sequence: &mut Sequence,
        io: &mut Io,
    ) {
        io.request(
            entity,
            sequence,
            context.git(
                &settings.0,
                &["merge-tree", "--write-tree", &self.base, &self.head],
            ),
            MergeSlot,
        );
        io.request(
            entity,
            sequence,
            context.git(
                &settings.0,
                &["rev-parse", &format!("{}^{{tree}}", self.checkout)],
            ),
            TreeSlot,
        );
        io.request(
            entity,
            sequence,
            context.git(&settings.0, &["show", &format!("{}:Cargo.toml", self.base)]),
            BaseVersionSlot,
        );
        io.request(
            entity,
            sequence,
            context.git(&settings.0, &["show", &format!("{}:Cargo.toml", self.head)]),
            HeadVersionSlot,
        );
    }
    fn verify_merge(
        &mut self,
        tree: &str,
        actual: &str,
        base: &str,
        head: &str,
    ) -> Result<(), String> {
        let tree = tree.lines().next().ok_or("Missing CI merge tree")?.trim();
        Self::sha(tree)?;
        if actual.trim() != tree {
            return Err("PR checkout differs from expected base/head merge tree".into());
        }
        self.tree = tree.into();
        self.release = Self::version(base)? != Self::version(head)?;
        Ok(())
    }
    fn output(&self) -> String {
        format!(
            "head={}\nbase={}\ntree={}\nrelease={}\npresentation_only={}\n",
            self.head, self.base, self.tree, self.release, self.presentation_only
        )
    }
}
#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    Event,
    Parents,
    Tip,
    Fetch,
    Ancestry,
    Merge,
    Presentation,
    Comparing,
    Emit,
    Printed,
}
#[derive(Component)]
struct EventSlot;
#[derive(Component)]
struct ParentSlot;
#[derive(Component)]
struct TipSlot;
#[derive(Component)]
struct FetchSlot;
#[derive(Component)]
struct AncestorSlot;
#[derive(Component)]
struct DescendantSlot;
#[derive(Component)]
struct MergeSlot;
#[derive(Component)]
struct TreeSlot;
#[derive(Component)]
struct BaseVersionSlot;
#[derive(Component)]
struct HeadVersionSlot;
#[derive(Component)]
struct PrintSlot;
type Plans<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static Settings,
        &'static mut Plan,
        &'static mut Phase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
fn start(mut requests: StartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, settings, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.plan") {
            continue;
        }
        let repository = match Arguments::text(&settings.0, "GITHUB_REPOSITORY") {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        if repository.split('/').count() != 2
            || repository.split('/').any(str::is_empty)
            || !repository
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_./".contains(&byte))
        {
            io.fail(entity, "Invalid CI repository");
            continue;
        }
        let event = match Arguments::text(&settings.0, "GITHUB_EVENT_NAME") {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let checkout = match Arguments::text(&settings.0, "GITHUB_SHA") {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        if let Err(error) = Plan::sha(&checkout) {
            io.fail(entity, error);
            continue;
        }
        let plan = Plan {
            repository,
            head: checkout.clone(),
            checkout,
            base: String::new(),
            supplied_base: String::new(),
            base_branch: String::new(),
            tip: String::new(),
            tree: String::new(),
            release: false,
            presentation_only: false,
        };
        if event == "pull_request" {
            let path = match Arguments::text(&settings.0, "GITHUB_EVENT_PATH") {
                Ok(value) => std::path::PathBuf::from(value),
                Err(error) => {
                    io.fail(entity, error);
                    continue;
                }
            };
            let Some(parent) = path.parent() else {
                io.fail(entity, "Missing CI event parent");
                continue;
            };
            let Some(name) = path.file_name() else {
                io.fail(entity, "Missing CI event filename");
                continue;
            };
            io.request(
                entity,
                &mut sequence,
                ReadFile {
                    location: Location {
                        root: parent.into(),
                        relative: name.into(),
                    },
                    limit: 1024 * 1024,
                },
                EventSlot,
            );
            io.commands.entity(entity).insert((plan, Phase::Event));
        } else {
            io.commands.entity(entity).insert((plan, Phase::Emit));
        }
    }
}
fn event(mut plans: Plans, mut io: Io, results: Results) {
    for (entity, context, settings, mut plan, mut phase, mut sequence) in &mut plans {
        if *phase != Phase::Event {
            continue;
        }
        let Some(bytes) = results.event_slot_bytes.get(entity) else {
            continue;
        };
        let payload: Value = match serde_json::from_slice(&bytes.0) {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error.to_string());
                continue;
            }
        };
        if let Err(error) = plan.pull(&payload) {
            io.fail(entity, error);
            continue;
        }
        io.request(
            entity,
            &mut sequence,
            context.git(&settings.0, &["show", "-s", "--format=%P", &plan.checkout]),
            ParentSlot,
        );
        *phase = Phase::Parents;
    }
}
fn parents(mut plans: Plans, mut io: Io, results: Results) {
    for (entity, context, settings, mut plan, mut phase, mut sequence) in &mut plans {
        if *phase != Phase::Parents {
            continue;
        }
        let Some(result) = results
            .parent_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let value = match result {
            Ok(value) => value.to_owned(),
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let parents: Vec<_> = value.split_whitespace().collect();
        if parents.len() != 2 || parents[1] != plan.head {
            io.fail(
                entity,
                "PR checkout does not match its expected merge parents",
            );
            continue;
        }
        plan.base = parents[0].into();
        if plan.base == plan.supplied_base {
            plan.probe(entity, context, settings, &mut sequence, &mut io);
            *phase = Phase::Merge;
        } else {
            let endpoint = format!(
                "repos/{}/branches/{}",
                plan.repository,
                Plan::encode(&plan.base_branch)
            );
            io.request(
                entity,
                &mut sequence,
                context.process(&settings.0, "gh", &["api", &endpoint]),
                TipSlot,
            );
            *phase = Phase::Tip;
        }
    }
}
fn tip(mut plans: Plans, mut io: Io, results: Results) {
    for (entity, context, settings, mut plan, mut phase, mut sequence) in &mut plans {
        if *phase != Phase::Tip {
            continue;
        }
        let Some(result) = results
            .tip_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let payload: Value = match result
            .and_then(|text| serde_json::from_str(text).map_err(|error| error.to_string()))
        {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let Some(tip) = payload.pointer("/commit/sha").and_then(Value::as_str) else {
            io.fail(entity, "Missing target branch tip");
            continue;
        };
        if let Err(error) = Plan::sha(tip) {
            io.fail(entity, error);
            continue;
        }
        plan.tip = tip.into();
        io.request(
            entity,
            &mut sequence,
            context.git(
                &settings.0,
                &[
                    "fetch",
                    "--no-tags",
                    "origin",
                    &plan.supplied_base,
                    &plan.tip,
                ],
            ),
            FetchSlot,
        );
        *phase = Phase::Fetch;
    }
}
fn fetched(mut plans: Plans, mut io: Io, results: Results) {
    for (entity, context, settings, plan, mut phase, mut sequence) in &mut plans {
        if *phase != Phase::Fetch {
            continue;
        }
        let Some(result) = results
            .fetch_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        if let Err(error) = result {
            io.fail(entity, error);
            continue;
        }
        io.request(
            entity,
            &mut sequence,
            context.git(
                &settings.0,
                &[
                    "merge-base",
                    "--is-ancestor",
                    &plan.supplied_base,
                    &plan.base,
                ],
            ),
            AncestorSlot,
        );
        io.request(
            entity,
            &mut sequence,
            context.git(
                &settings.0,
                &["merge-base", "--is-ancestor", &plan.base, &plan.tip],
            ),
            DescendantSlot,
        );
        *phase = Phase::Ancestry;
    }
}
fn ancestry(mut plans: Plans, mut io: Io, results: Results) {
    for (entity, context, settings, plan, mut phase, mut sequence) in &mut plans {
        if *phase != Phase::Ancestry {
            continue;
        }
        let Some(first) = results.ancestor_slot_captured.get(entity) else {
            continue;
        };
        let Some(second) = results.descendant_slot_captured.get(entity) else {
            continue;
        };
        if !first.0.status.success() || !second.0.status.success() {
            io.fail(entity, "PR checkout base is outside target branch ancestry");
            continue;
        }
        plan.probe(entity, context, settings, &mut sequence, &mut io);
        *phase = Phase::Merge;
    }
}
fn merged(mut plans: Plans, mut io: Io, results: Results) {
    for (entity, _, _, mut plan, mut phase, _) in &mut plans {
        if *phase != Phase::Merge {
            continue;
        }
        let Some(tree) = results
            .merge_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let Some(actual) = results
            .tree_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let Some(base) = results
            .base_version_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let Some(head) = results
            .head_version_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let result = match (tree, actual, base, head) {
            (Ok(tree), Ok(actual), Ok(base), Ok(head)) => {
                plan.verify_merge(tree, actual, base, head)
            }
            (Err(error), _, _, _)
            | (_, Err(error), _, _)
            | (_, _, Err(error), _)
            | (_, _, _, Err(error)) => Err(error),
        };
        match result {
            Ok(()) => *phase = Phase::Presentation,
            Err(error) => io.fail(entity, error),
        }
    }
}
#[derive(Component)]
struct ChangedSlot;
#[derive(Component)]
struct BeforeModeSlot;
#[derive(Component)]
struct AfterModeSlot;
#[derive(Component)]
struct BeforeWorkflowSlot;
#[derive(Component)]
struct AfterWorkflowSlot;
fn presentation_start(mut plans: Plans, mut io: Io) {
    for (entity, context, settings, plan, mut phase, mut sequence) in &mut plans {
        if *phase != Phase::Presentation {
            continue;
        }
        let file = ".github/workflows/ci.yml";
        io.request(
            entity,
            &mut sequence,
            context.git(
                &settings.0,
                &[
                    "diff",
                    "--name-only",
                    "--no-renames",
                    "-z",
                    &plan.base,
                    &plan.tree,
                    "--",
                ],
            ),
            ChangedSlot,
        );
        io.request(
            entity,
            &mut sequence,
            context.git(&settings.0, &["ls-tree", &plan.base, "--", file]),
            BeforeModeSlot,
        );
        io.request(
            entity,
            &mut sequence,
            context.git(&settings.0, &["ls-tree", &plan.tree, "--", file]),
            AfterModeSlot,
        );
        io.request(
            entity,
            &mut sequence,
            context.git(&settings.0, &["show", &format!("{}:{file}", plan.base)]),
            BeforeWorkflowSlot,
        );
        io.request(
            entity,
            &mut sequence,
            context.git(&settings.0, &["show", &format!("{}:{file}", plan.tree)]),
            AfterWorkflowSlot,
        );
        *phase = Phase::Comparing;
    }
}
fn presentation_compare(mut plans: Plans, results: Results) {
    for (entity, _, _, mut plan, mut phase, _) in &mut plans {
        if *phase != Phase::Comparing {
            continue;
        }
        let Some(changed) = results
            .changed_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let Some(before_mode) = results
            .before_mode_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let Some(after_mode) = results
            .after_mode_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let Some(before) = results
            .before_workflow_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let Some(after) = results
            .after_workflow_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        plan.presentation_only = changed.is_ok_and(|value| value == ".github/workflows/ci.yml\0")
            && before_mode.is_ok_and(|value| value.starts_with("100644 blob "))
            && after_mode.is_ok_and(|value| value.starts_with("100644 blob "));
        if plan.presentation_only {
            plan.presentation_only = match (before, after) {
                (Ok(before), Ok(after)) => Plan::equivalent(before, after),
                _ => false,
            };
        }
        *phase = Phase::Emit;
    }
}
fn emit(mut plans: Plans, mut io: Io, results: Results) {
    for (entity, _, _, plan, mut phase, mut sequence) in &mut plans {
        if *phase == Phase::Emit {
            io.emit(
                entity,
                &mut sequence,
                Print(plan.output().into_bytes()),
                PrintSlot,
            );
            *phase = Phase::Printed;
        } else if *phase == Phase::Printed && results.print_slot_written.get(entity).is_some() {
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}
#[derive(Component)]
#[require(CheckoutPhase)]
struct Checkout {
    head: String,
    base: String,
    tree: String,
    commit: String,
}
#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
enum CheckoutPhase {
    #[default]
    Tree,
    Date,
    Commit,
    Checkout,
    Printed,
}
#[derive(Component)]
struct CheckoutTreeSlot;
#[derive(Component)]
struct DateSlot;
#[derive(Component)]
struct CommitSlot;
#[derive(Component)]
struct CheckoutSlot;
#[derive(Component)]
struct CheckoutPrintSlot;
type Checkouts<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static Settings,
        &'static mut Checkout,
        &'static mut CheckoutPhase,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Completed>),
>;
fn checkout_start(mut requests: CheckoutStartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, settings, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.checkout") {
            continue;
        }
        let base = settings
            .0
            .get("VMUX_CI_BASE")
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_owned();
        let head = settings
            .0
            .get("VMUX_CI_HEAD")
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_owned();
        let tree = settings
            .0
            .get("VMUX_CI_TREE")
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_owned();
        let request = Checkout {
            head,
            base,
            tree,
            commit: String::new(),
        };
        if request.base.is_empty() {
            io.request(
                entity,
                &mut sequence,
                context.git(&settings.0, &["rev-parse", "HEAD"]),
                CommitSlot,
            );
            io.commands
                .entity(entity)
                .insert((request, CheckoutPhase::Commit));
            continue;
        }
        if [&request.head, &request.base, &request.tree]
            .iter()
            .any(|value| Plan::sha(value).is_err())
        {
            io.fail(entity, "Invalid CI checkout identifiers");
            continue;
        }
        io.request(
            entity,
            &mut sequence,
            context.git(
                &settings.0,
                &["merge-tree", "--write-tree", &request.base, &request.head],
            ),
            CheckoutTreeSlot,
        );
        io.commands
            .entity(entity)
            .insert((request, CheckoutPhase::Tree));
    }
}
fn checkout_tree(mut requests: Checkouts, mut io: Io, results: Results) {
    for (entity, context, settings, request, mut phase, mut sequence) in &mut requests {
        if *phase != CheckoutPhase::Tree {
            continue;
        }
        let Some(result) = results
            .checkout_tree_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let value = match result {
            Ok(value) => value.lines().next().unwrap_or("").trim().to_owned(),
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        if value != request.tree {
            io.fail(entity, "CI merge tree changed after planning");
            continue;
        }
        io.request(
            entity,
            &mut sequence,
            context.git(&settings.0, &["show", "-s", "--format=%cI", &request.head]),
            DateSlot,
        );
        *phase = CheckoutPhase::Date;
    }
}
fn checkout_date(mut requests: Checkouts, mut io: Io, results: Results) {
    for (entity, context, settings, request, mut phase, mut sequence) in &mut requests {
        if *phase != CheckoutPhase::Date {
            continue;
        }
        let Some(result) = results
            .date_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let date = match result {
            Ok(value) => value.trim().to_owned(),
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let mut process = context.git(
            &settings.0,
            &[
                "commit-tree",
                &request.tree,
                "-p",
                &request.base,
                "-p",
                &request.head,
            ],
        );
        for (key, value) in [
            ("GIT_AUTHOR_NAME", "CI"),
            ("GIT_COMMITTER_NAME", "CI"),
            ("GIT_AUTHOR_EMAIL", "ci@vmux.ai"),
            ("GIT_COMMITTER_EMAIL", "ci@vmux.ai"),
            ("GIT_AUTHOR_DATE", date.as_str()),
            ("GIT_COMMITTER_DATE", date.as_str()),
        ] {
            process.environment.set(key, value);
        }
        process.input = Some(b"CI merge validation\n".to_vec());
        io.request(entity, &mut sequence, process, CommitSlot);
        *phase = CheckoutPhase::Commit;
    }
}
fn checkout_commit(mut requests: Checkouts, mut io: Io, results: Results) {
    for (entity, context, settings, mut request, mut phase, mut sequence) in &mut requests {
        if *phase != CheckoutPhase::Commit {
            continue;
        }
        let Some(result) = results
            .commit_slot_captured
            .get(entity)
            .map(crate::ci_io::process::Captured::text)
        else {
            continue;
        };
        let commit = match result {
            Ok(value) => value.trim().to_owned(),
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        if let Err(error) = Plan::sha(&commit) {
            io.fail(entity, error);
            continue;
        }
        request.commit = commit;
        if request.base.is_empty() {
            io.emit(
                entity,
                &mut sequence,
                Print(format!("commit={}\n", request.commit).into_bytes()),
                CheckoutPrintSlot,
            );
            *phase = CheckoutPhase::Printed;
        } else {
            io.request(
                entity,
                &mut sequence,
                context.git(&settings.0, &["checkout", "--detach", &request.commit]),
                CheckoutSlot,
            );
            *phase = CheckoutPhase::Checkout;
        }
    }
}
fn checkout_finish(mut requests: Checkouts, mut io: Io, results: Results) {
    for (entity, _, _, request, mut phase, mut sequence) in &mut requests {
        if *phase == CheckoutPhase::Checkout {
            let Some(result) = results
                .checkout_slot_captured
                .get(entity)
                .map(crate::ci_io::process::Captured::text)
            else {
                continue;
            };
            if let Err(error) = result {
                io.fail(entity, error);
                continue;
            }
            io.emit(
                entity,
                &mut sequence,
                Print(format!("commit={}\n", request.commit).into_bytes()),
                CheckoutPrintSlot,
            );
            *phase = CheckoutPhase::Printed;
        } else if *phase == CheckoutPhase::Printed
            && results.checkout_print_slot_written.get(entity).is_some()
        {
            io.commands.entity(entity).insert(Completed(Ok(0)));
        }
    }
}

type StartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Settings,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Plan>, Without<Completed>),
>;
type CheckoutStartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static Settings,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Checkout>, Without<Completed>),
>;

#[cfg(test)]
mod tests {
    use crate::ci::testing::Fixture;
    use serde_json::json;
    #[test]
    fn merge_parents_and_tree_are_verified_and_checkout_is_deterministic() {
        let mut fixture = Fixture::new();
        let base = fixture.git(&["rev-parse", "HEAD"]);
        fixture.git(&["checkout", "-qb", "feature"]);
        fixture.write("feature.txt", "head");
        let head = fixture.commit();
        fixture.git(&["checkout", "--detach", &base]);
        fixture.git(&["merge", "--no-ff", "-m", "Merge", &head]);
        let checkout = fixture.git(&["rev-parse", "HEAD"]);
        let tree = fixture.git(&["rev-parse", "HEAD^{tree}"]);
        fixture . json ("event.json" , & json ! ({ "pull_request" : { "head" : { "sha" : head } , "base" : { "sha" : base , "ref" : "main" } } }) ,) ;
        fixture.environment.set(
            "GITHUB_EVENT_PATH",
            fixture.root().join("event.json").as_os_str(),
        );
        fixture.environment.set("GITHUB_SHA", &checkout);
        fixture.environment.set("GIT_DIR", "/unrelated/git");
        let outputs = Fixture::outputs(&fixture.run("plan", &[]).unwrap());
        assert_eq!(outputs["base"], base);
        assert_eq!(outputs["head"], head);
        assert_eq!(outputs["tree"], tree);
        assert_eq!(outputs["release"], "false");
        fixture.environment.set("VMUX_CI_BASE", &base);
        fixture.environment.set("VMUX_CI_HEAD", &head);
        fixture.environment.set("VMUX_CI_TREE", &tree);
        fixture.run("checkout", &[]).unwrap();
        let first = fixture.git(&["rev-parse", "HEAD"]);
        assert_eq!(fixture.git(&["rev-parse", "HEAD^{tree}"]), tree);
        fixture.run("checkout", &[]).unwrap();
        assert_eq!(fixture.git(&["rev-parse", "HEAD"]), first);
        fixture.environment.set("GITHUB_SHA", &head);
        assert!(
            fixture
                .run("plan", &[])
                .unwrap_err()
                .contains("merge parents")
        );
    }
    #[test]
    fn an_advanced_merge_base_must_remain_on_the_actual_target_branch() {
        let mut fixture = Fixture::new();
        let supplied = fixture.git(&["rev-parse", "HEAD"]);
        fixture.git(&["checkout", "-qb", "feature"]);
        fixture.write("feature.txt", "head");
        let head = fixture.commit();
        fixture.git(&["checkout", "--detach", &supplied]);
        fixture.write("base.txt", "advanced");
        let base = fixture.commit();
        fixture.git(&["merge", "--no-ff", "-m", "Merge", &head]);
        fixture.git(&["remote", "add", "origin", fixture.root().to_str().unwrap()]);
        fixture . json ("event.json" , & json ! ({ "pull_request" : { "head" : { "sha" : head } , "base" : { "sha" : supplied , "ref" : "release/staging" } } })) ;
        fixture.environment.set(
            "GITHUB_EVENT_PATH",
            fixture.root().join("event.json").as_os_str(),
        );
        fixture
            .environment
            .set("GITHUB_SHA", fixture.git(&["rev-parse", "HEAD"]));
        fixture . mock_gh (& format ! ("#!/bin/sh\nprintf '%s\\n' \"$2\" > api-url\necho '{{\"commit\":{{\"sha\":\"{base}\"}}}}'\n")) ;
        assert_eq!(
            Fixture::outputs(&fixture.run("plan", &[]).unwrap())["base"],
            base
        );
        assert_eq!(
            std::fs::read_to_string(fixture.root().join("api-url"))
                .unwrap()
                .trim(),
            "repos/fixture/repository/branches/release%2Fstaging"
        );
        fixture.mock_gh(&format!(
            "#!/bin/sh\necho '{{\"commit\":{{\"sha\":\"{head}\"}}}}'\n"
        ));
        assert!(fixture.run("plan", &[]).unwrap_err().contains("ancestry"));
    }
    #[test]
    fn workflow_names_only_skip_when_regular_blobs_and_execution_match() {
        for executable in [false, true] {
            let mut fixture = Fixture::new();
            let base = fixture.git(&["rev-parse", "HEAD"]);
            fixture.git(&["checkout", "-qb", "feature"]);
            let path = fixture.root().join(".github/workflows/ci.yml");
            let source = std::fs::read_to_string(&path).unwrap();
            fixture.write(
                ".github/workflows/ci.yml",
                source.replace(
                    if executable {
                        "run: true"
                    } else {
                        "name: Results"
                    },
                    if executable {
                        "run: false"
                    } else {
                        "name: Check Results"
                    },
                ),
            );
            let head = fixture.commit();
            fixture.git(&["checkout", "--detach", &base]);
            fixture.git(&["merge", "--no-ff", "-m", "Merge", &head]);
            fixture . json ("event.json" , & json ! ({ "pull_request" : { "head" : { "sha" : head } , "base" : { "sha" : base , "ref" : "main" } } })) ;
            fixture.environment.set(
                "GITHUB_EVENT_PATH",
                fixture.root().join("event.json").as_os_str(),
            );
            fixture
                .environment
                .set("GITHUB_SHA", fixture.git(&["rev-parse", "HEAD"]));
            let output = Fixture::outputs(&fixture.run("plan", &[]).unwrap());
            assert_eq!(output["presentation_only"], (!executable).to_string());
        }
    }
}
