//! A phone's layout, drawn from the ECS: tabs along the bottom, a stack within the
//! selected one, sheets on top of that. Nothing here is Dioxus or UIKit.

use bevy_app::App;
use bevy_ecs::prelude::*;
use vmux_mobile::nav::{
    Dismiss, Dropped, GoBack, Local, NavPlugin, OpenBlank, Present, Push, Report, Route, Select,
    Selected, Sheet, Shows, Tab,
};

#[derive(Clone, PartialEq)]
struct Page(&'static str);

impl Route for Page {
    type Name = Name;

    fn name(&self) -> Name {
        Name(self.0)
    }

    fn title(&self) -> String {
        self.0.to_string()
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Name(&'static str);

fn main() {
    let mut app = App::new();
    app.add_plugins(NavPlugin::<Page>::default());

    step(
        &mut app,
        "the Mac reports two tabs, focused on Notes",
        |world| {
            world.write_message(Report {
                tabs: vec![
                    ("inbox".to_string(), Page("Inbox")),
                    ("notes".to_string(), Page("Notes")),
                ],
                focused: Some("notes".to_string()),
            });
        },
    );

    step(&mut app, "push twice into Notes", |world| {
        world.write_message(Push(Page("Groceries")));
    });
    step(&mut app, "", |world| {
        world.write_message(Push(Page("Edit")));
    });

    step(&mut app, "a sheet, then a sheet over it", |world| {
        world.write_message(Present(Page("Discard?")));
    });
    step(&mut app, "", |world| {
        world.write_message(Present(Page("Really?")));
    });

    step(&mut app, "dismiss the top sheet only", |world| {
        world.write_message(Dismiss);
    });

    step(
        &mut app,
        "switch to Inbox — its own depth is nil",
        |world| {
            world.write_message(Select("inbox".to_string()));
        },
    );

    step(&mut app, "back to Notes, which kept everything", |world| {
        world.write_message(Select("notes".to_string()));
    });

    step(&mut app, "a back-swipe UIKit already ran", |world| {
        world.write_message(Dropped(1));
    });

    step(&mut app, "back, twice", |world| {
        world.write_message(GoBack);
    });
    step(&mut app, "", |world| {
        world.write_message(GoBack);
    });

    step(&mut app, "a tab of the phone's own", |world| {
        world.write_message(OpenBlank(Page("Untitled")));
    });

    step(&mut app, "the Mac closes Notes", |world| {
        world.write_message(Report {
            tabs: vec![("inbox".to_string(), Page("Inbox"))],
            focused: None,
        });
    });
}

fn step(app: &mut App, note: &str, act: impl FnOnce(&mut World)) {
    act(app.world_mut());
    app.update();
    if !note.is_empty() {
        println!("\n{note}");
    }
    app.world_mut().run_system_cached(draw).ok();
}

type Tabs<'w, 's> =
    Query<'w, 's, (Entity, &'static Shows<Page>, Option<&'static Local>), With<Tab>>;

fn draw(
    tabs: Tabs,
    selection: Query<Entity, With<Selected>>,
    children: Query<&Children>,
    sheets: Query<&Sheet>,
    shows: Query<&Shows<Page>>,
) {
    let selected = selection.iter().next();
    let mut open: Vec<(String, bool, bool)> = Vec::new();
    for (entity, Shows(page), local) in tabs.iter() {
        open.push((page.title(), local.is_some(), Some(entity) == selected));
    }
    open.sort_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));

    let mut levels = Vec::new();
    if let Some(tab) = selected {
        if let Ok(Shows(root)) = shows.get(tab) {
            levels.push((root.title(), "root"));
        }
        let mut at = tab;
        while let Ok(kids) = children.get(at) {
            let Some(next) = kids.last().copied() else {
                break;
            };
            let kind = if sheets.get(next).is_ok() {
                "sheet"
            } else {
                "pushed"
            };
            if let Ok(Shows(page)) = shows.get(next) {
                levels.push((page.title(), kind));
            }
            at = next;
        }
    }

    println!("    ╭──────────────────────────────╮");
    for (title, kind) in levels.iter().rev() {
        println!("    │ {title:<20} {kind:>7} │");
    }
    println!("    ╰──────────────────────────────╯");

    let mut bar = String::from("     ");
    for (title, local, mark) in &open {
        let name = if *local {
            format!("{title}*")
        } else {
            title.clone()
        };
        if *mark {
            bar.push_str(&format!("[{name}] "));
        } else {
            bar.push_str(&format!(" {name}  "));
        }
    }
    println!("{bar}");
}
