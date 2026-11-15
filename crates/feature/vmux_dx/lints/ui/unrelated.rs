struct App;

impl App {
    fn add_systems(&mut self, _: (), _: fn()) {}
}

pub fn exported() {}

fn main() {
    App.add_systems((), exported);
}
